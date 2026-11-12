//! file-seal — local SHA-256 integrity ledger core library.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SealError {
    #[error("{0}")]
    Msg(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
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
    pub ts: DateTime<Utc>,
    pub path: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
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

pub fn append_record(ledger: &Path, rec: &SealRecord) -> Result<()> {
    if let Some(parent) = ledger.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(ledger)?;
    let line = serde_json::to_string(rec)?;
    writeln!(f, "{line}")?;
    Ok(())
}

pub fn read_all(ledger: &Path) -> Result<Vec<SealRecord>> {
    if !ledger.exists() {
        return Ok(vec![]);
    }
    let f = File::open(ledger)?;
    let reader = BufReader::new(f);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Ok(r) = serde_json::from_str::<SealRecord>(t) {
            out.push(r);
        }
    }
    Ok(out)
}

pub fn latest_for_path<'a>(records: &'a [SealRecord], path_key: &str) -> Option<&'a SealRecord> {
    records.iter().rev().find(|r| r.path == path_key)
}

pub fn seal_file(ledger: &Path, path: &Path, message: Option<String>) -> Result<SealRecord> {
    if !path.is_file() {
        return Err(SealError::Msg(format!("not a file: {}", path.display())));
    }
    let (sha256, size) = hash_file(path)?;
    let path_key = canonical_path(path)?;
    let rec = SealRecord {
        ts: Utc::now(),
        path: path_key,
        sha256,
        size,
        message,
    };
    append_record(ledger, &rec)?;
    Ok(rec)
}

pub fn verify_file(ledger: &Path, path: &Path) -> Result<SealRecord> {
    if !path.is_file() {
        return Err(SealError::Msg(format!("not a file: {}", path.display())));
    }
    let path_key = canonical_path(path)?;
    let records = read_all(ledger)?;
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

    #[test]
    fn test_hash_and_seal_verify() {
        let (dir, ledger) = setup();
        let file = dir.path().join("test.txt");
        fs::write(&file, b"hello world").unwrap();

        let rec = seal_file(&ledger, &file, Some("test note".into())).unwrap();
        assert_eq!(rec.message, Some("test note".into()));
        assert_eq!(rec.size, 11);

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
}
