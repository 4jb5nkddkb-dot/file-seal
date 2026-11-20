//! file-seal — local SHA-256 integrity ledger core library.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// The fixed parent hash for the first record in a ledger.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Error)]
pub enum SealError {
    #[error("{0}")]
    Msg(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("ledger parse error at line {line}: {message}")]
    LedgerParse { line: usize, message: String },
    #[error("ledger record {index} is in the legacy format and has no hash-chain proof")]
    LegacyLedger { index: usize },
    #[error("ledger chain is invalid at record {index}: {reason}")]
    ChainBroken { index: usize, reason: String },
    #[error("no seal found for {0}")]
    NoSeal(String),
    #[error("hash mismatch for {path}: recorded {recorded}, current {current}")]
    HashMismatch {
        path: String,
        recorded: String,
        current: String,
    },
}

pub type Result<T> = std::result::Result<T, SealError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealRecord {
    /// Monotonically increasing position in the ledger.
    #[serde(default)]
    pub seq: u64,
    pub ts: DateTime<Utc>,
    pub path: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Hash of the preceding record, or [`GENESIS_HASH`] for the first record.
    #[serde(default)]
    pub prev_hash: String,
    /// SHA-256 of the canonical record payload, excluding this field.
    #[serde(default)]
    pub record_hash: String,
}

#[derive(Serialize)]
struct RecordHashInput<'a> {
    seq: u64,
    ts: &'a DateTime<Utc>,
    path: &'a str,
    sha256: &'a str,
    size: u64,
    message: &'a Option<String>,
    prev_hash: &'a str,
}

impl SealRecord {
    /// Calculate the hash committed by this record, excluding `record_hash` itself.
    pub fn compute_hash(&self) -> Result<String> {
        let payload = RecordHashInput {
            seq: self.seq,
            ts: &self.ts,
            path: &self.path,
            sha256: &self.sha256,
            size: self.size,
            message: &self.message,
            prev_hash: &self.prev_hash,
        };
        let bytes = serde_json::to_vec(&payload)?;
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Ok(hex::encode(hasher.finalize()))
    }

    fn is_legacy(&self) -> bool {
        self.seq == 0 && self.prev_hash.is_empty() && self.record_hash.is_empty()
    }
}

/// Resolve ledger path. Default: ~/.local/share/file-seal/seals.jsonl (or platform equivalent).
pub fn resolve_ledger(override_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = override_path {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        return Ok(p.to_path_buf());
    }
    let base = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .or_else(|| env::var_os("LOCALAPPDATA").map(PathBuf::from)) // Windows
        .ok_or_else(|| {
            SealError::Msg("cannot resolve data dir (set HOME or provide ledger path)".into())
        })?;
    let dir = base.join("file-seal");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("seals.jsonl"))
}

pub fn hash_file(path: &Path) -> Result<(String, u64)> {
    let mut file =
        File::open(path).map_err(|e| SealError::Msg(format!("open {}: {e}", path.display())))?;
    let mut hasher = Sha256::new();
    let n = std::io::copy(&mut file, &mut hasher)
        .map_err(|e| SealError::Msg(format!("read {}: {e}", path.display())))?;
    let digest = hasher.finalize();
    Ok((hex::encode(digest), n))
}

pub fn canonical_path(path: &Path) -> Result<String> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    let resolved = fs::canonicalize(&abs).unwrap_or(abs);
    Ok(resolved.display().to_string())
}

/// Append a record only when it is the next valid record in the existing chain.
pub fn append_record(ledger: &Path, rec: &SealRecord) -> Result<()> {
    let records = read_all(ledger)?;
    verify_chain_records(&records)?;
    let expected_seq = records.len() as u64;
    let expected_prev = records
        .last()
        .map(|r| r.record_hash.as_str())
        .unwrap_or(GENESIS_HASH);

    if rec.seq != expected_seq {
        return Err(SealError::ChainBroken {
            index: expected_seq as usize,
            reason: format!("expected sequence {expected_seq}, got {}", rec.seq),
        });
    }
    if rec.prev_hash != expected_prev {
        return Err(SealError::ChainBroken {
            index: expected_seq as usize,
            reason: "previous hash does not match the current chain tip".into(),
        });
    }
    let expected_hash = rec.compute_hash()?;
    if rec.record_hash != expected_hash {
        return Err(SealError::ChainBroken {
            index: expected_seq as usize,
            reason: "record hash does not match its canonical payload".into(),
        });
    }

    if let Some(parent) = ledger.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(ledger)?;
    let line = serde_json::to_string(rec)?;
    writeln!(f, "{line}")?;
    f.sync_data()?;
    Ok(())
}

pub fn read_all(ledger: &Path) -> Result<Vec<SealRecord>> {
    if !ledger.exists() {
        return Ok(vec![]);
    }
    let f = File::open(ledger)?;
    let reader = BufReader::new(f);
    let mut out = Vec::new();
    for (line_index, line) in reader.lines().enumerate() {
        let line_number = line_index + 1;
        let line = line?;
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let record = serde_json::from_str::<SealRecord>(t).map_err(|e| SealError::LedgerParse {
            line: line_number,
            message: e.to_string(),
        })?;
        out.push(record);
    }
    Ok(out)
}

/// Verify every record's sequence, parent hash, and canonical record hash.
pub fn verify_chain(ledger: &Path) -> Result<()> {
    let records = read_all(ledger)?;
    verify_chain_records(&records)
}

fn verify_chain_records(records: &[SealRecord]) -> Result<()> {
    for (index, record) in records.iter().enumerate() {
        if record.is_legacy() {
            return Err(SealError::LegacyLedger { index });
        }
        let expected_seq = index as u64;
        if record.seq != expected_seq {
            return Err(SealError::ChainBroken {
                index,
                reason: format!("expected sequence {expected_seq}, got {}", record.seq),
            });
        }
        let expected_prev = records
            .get(index.wrapping_sub(1))
            .map(|r| r.record_hash.as_str())
            .unwrap_or(GENESIS_HASH);
        if record.prev_hash != expected_prev {
            return Err(SealError::ChainBroken {
                index,
                reason: "previous hash does not match the preceding record".into(),
            });
        }
        let expected_hash = record.compute_hash()?;
        if record.record_hash != expected_hash {
            return Err(SealError::ChainBroken {
                index,
                reason: "record hash does not match its canonical payload".into(),
            });
        }
    }
    Ok(())
}

pub fn latest_for_path<'a>(records: &'a [SealRecord], path_key: &str) -> Option<&'a SealRecord> {
    records.iter().rev().find(|r| r.path == path_key)
}

pub fn seal_file(ledger: &Path, path: &Path, message: Option<String>) -> Result<SealRecord> {
    if !path.is_file() {
        return Err(SealError::Msg(format!("not a file: {}", path.display())));
    }
    let records = read_all(ledger)?;
    verify_chain_records(&records)?;
    let (sha256, size) = hash_file(path)?;
    let path_key = canonical_path(path)?;
    let seq = records.len() as u64;
    let prev_hash = records
        .last()
        .map(|r| r.record_hash.clone())
        .unwrap_or_else(|| GENESIS_HASH.to_string());
    let mut rec = SealRecord {
        seq,
        ts: Utc::now(),
        path: path_key,
        sha256,
        size,
        message,
        prev_hash,
        record_hash: String::new(),
    };
    rec.record_hash = rec.compute_hash()?;
    append_record(ledger, &rec)?;
    Ok(rec)
}

pub fn verify_file(ledger: &Path, path: &Path) -> Result<SealRecord> {
    if !path.is_file() {
        return Err(SealError::Msg(format!("not a file: {}", path.display())));
    }
    let records = read_all(ledger)?;
    verify_chain_records(&records)?;
    let path_key = canonical_path(path)?;
    let Some(prev) = latest_for_path(&records, &path_key) else {
        return Err(SealError::NoSeal(path_key));
    };
    let (sha256, size) = hash_file(path)?;
    if sha256 == prev.sha256 && size == prev.size {
        return Ok(prev.clone());
    }
    Err(SealError::HashMismatch {
        path: path_key,
        recorded: short_hash(&prev.sha256),
        current: short_hash(&sha256),
    })
}

pub fn list_seals(
    ledger: &Path,
    path_filter: Option<&Path>,
    limit: usize,
) -> Result<Vec<SealRecord>> {
    let mut records = read_all(ledger)?;
    verify_chain_records(&records)?;
    if let Some(p) = path_filter {
        let key = canonical_path(p).unwrap_or_else(|_| p.display().to_string());
        records.retain(|r| r.path == key || r.path.ends_with(&key));
    }
    records.reverse();
    records.truncate(limit);
    Ok(records)
}

pub fn short_hash(h: &str) -> String {
    if h.len() <= 16 {
        h.to_string()
    } else {
        format!("{}\u{2026}{}", &h[..8], &h[h.len() - 4..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let ledger = dir.path().join("seals.jsonl");
        (dir, ledger)
    }

    fn write_records(ledger: &Path, records: &[SealRecord]) {
        let body = records
            .iter()
            .map(|r| serde_json::to_string(r).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(ledger, format!("{body}\n")).unwrap();
    }

    #[test]
    fn test_hash_and_seal_verify() {
        let (dir, ledger) = setup();
        let file = dir.path().join("test.txt");
        fs::write(&file, b"hello world").unwrap();

        let rec = seal_file(&ledger, &file, Some("test note".into())).unwrap();
        assert_eq!(rec.seq, 0);
        assert_eq!(rec.prev_hash, GENESIS_HASH);
        assert_eq!(rec.message, Some("test note".into()));
        assert_eq!(rec.size, 11);
        assert_eq!(rec.record_hash, rec.compute_hash().unwrap());

        verify_chain(&ledger).unwrap();
        verify_file(&ledger, &file).unwrap();
    }

    #[test]
    fn test_hash_mismatch() {
        let (dir, ledger) = setup();
        let file = dir.path().join("test.txt");
        fs::write(&file, b"hello world").unwrap();
        seal_file(&ledger, &file, None).unwrap();

        fs::write(&file, b"hello universe").unwrap();
        let result = verify_file(&ledger, &file);
        assert!(matches!(result, Err(SealError::HashMismatch { .. })));
    }

    #[test]
    fn test_hash_chain_tamper_record() {
        let (dir, ledger) = setup();
        let file = dir.path().join("test.txt");
        fs::write(&file, b"hello world").unwrap();
        seal_file(&ledger, &file, Some("original".into())).unwrap();
        seal_file(&ledger, &file, Some("second".into())).unwrap();

        let mut records = read_all(&ledger).unwrap();
        records[0].message = Some("forged".into());
        write_records(&ledger, &records);

        assert!(matches!(
            verify_chain(&ledger),
            Err(SealError::ChainBroken { index: 0, .. })
        ));
    }

    #[test]
    fn test_hash_chain_tamper_deleted_middle_record() {
        let (dir, ledger) = setup();
        let file = dir.path().join("test.txt");
        fs::write(&file, b"hello world").unwrap();
        seal_file(&ledger, &file, Some("one".into())).unwrap();
        seal_file(&ledger, &file, Some("two".into())).unwrap();
        seal_file(&ledger, &file, Some("three".into())).unwrap();

        let mut records = read_all(&ledger).unwrap();
        records.remove(1);
        write_records(&ledger, &records);

        assert!(matches!(
            verify_chain(&ledger),
            Err(SealError::ChainBroken { index: 1, .. })
        ));
    }

    #[test]
    fn test_hash_chain_tamper_forged_append() {
        let (dir, ledger) = setup();
        let file = dir.path().join("test.txt");
        fs::write(&file, b"hello world").unwrap();
        seal_file(&ledger, &file, None).unwrap();

        let mut records = read_all(&ledger).unwrap();
        let mut forged = records[0].clone();
        forged.seq = 1;
        forged.prev_hash = records[0].record_hash.clone();
        forged.record_hash = forged.compute_hash().unwrap();
        forged.message = Some("forged after hashing".into());
        records.push(forged);
        write_records(&ledger, &records);

        assert!(matches!(
            verify_chain(&ledger),
            Err(SealError::ChainBroken { index: 1, .. })
        ));
    }

    #[test]
    fn test_hash_chain_corrupt_json_is_an_error() {
        let (_dir, ledger) = setup();
        fs::write(&ledger, "{not valid json}\n").unwrap();
        assert!(matches!(
            read_all(&ledger),
            Err(SealError::LedgerParse { line: 1, .. })
        ));
    }

    #[test]
    fn test_legacy_ledger_requires_explicit_migration() {
        let (_dir, ledger) = setup();
        let legacy = serde_json::json!({
            "ts": "2026-01-01T00:00:00Z",
            "path": "/tmp/example.txt",
            "sha256": "abc",
            "size": 3,
            "message": null
        });
        fs::write(&ledger, format!("{}\n", legacy)).unwrap();
        assert!(matches!(
            verify_chain(&ledger),
            Err(SealError::LegacyLedger { index: 0 })
        ));
    }
}
