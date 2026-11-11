//! file-seal — local SHA-256 integrity ledger CLI.

use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Parser, Debug)]
#[command(
    name = "file-seal",
    version,
    about = "Local file integrity seals (SHA-256 + ledger)"
)]
struct Cli {
    /// Override ledger path (default: ~/.local/share/file-seal/seals.jsonl)
    #[arg(long, global = true, env = "FILE_SEAL_LEDGER")]
    ledger: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Record a SHA-256 seal for a file
    Seal {
        /// File to seal
        path: PathBuf,
        /// Optional note stored with the seal
        #[arg(short, long)]
        message: Option<String>,
    },
    /// Verify file bytes against the latest seal for that path
    Verify {
        /// File to verify
        path: PathBuf,
    },
    /// List seals (newest first)
    List {
        /// Only show seals for this path
        #[arg(long)]
        path: Option<PathBuf>,
        /// Max rows (default 50)
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
}

#[derive(Debug, Error)]
enum SealError {
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

type Result<T> = std::result::Result<T, SealError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SealRecord {
    ts: DateTime<Utc>,
    path: String,
    sha256: String,
    size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

fn main() {
    if let Err(e) = run() {
        match e {
            SealError::NoSeal(path) => {
                eprintln!("FAIL  no seal found for {path}");
                eprintln!("hint  run: file-seal seal <path>");
                std::process::exit(2);
            }
            SealError::HashMismatch {
                path,
                recorded,
                current,
            } => {
                eprintln!("FAIL  hash mismatch");
                eprintln!("path     {path}");
                eprintln!("recorded {recorded}");
                eprintln!("current  {current}");
                std::process::exit(3);
            }
            _ => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let ledger = resolve_ledger(cli.ledger.as_deref())?;

    match cli.cmd {
        Command::Seal { path, message } => cmd_seal(&ledger, &path, message),
        Command::Verify { path } => cmd_verify(&ledger, &path),
        Command::List { path, limit } => cmd_list(&ledger, path.as_deref(), limit),
    }
}

fn resolve_ledger(override_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = override_path {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        return Ok(p.to_path_buf());
    }
    let base = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .ok_or_else(|| SealError::Msg("cannot resolve data dir (set HOME or --ledger)".into()))?;
    let dir = base.join("file-seal");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("seals.jsonl"))
}

fn hash_file(path: &Path) -> Result<(String, u64)> {
    let mut file =
        File::open(path).map_err(|e| SealError::Msg(format!("open {}: {e}", path.display())))?;
    let mut hasher = Sha256::new();
    let n = std::io::copy(&mut file, &mut hasher)
        .map_err(|e| SealError::Msg(format!("read {}: {e}", path.display())))?;
    let digest = hasher.finalize();
    Ok((hex::encode(digest), n))
}

fn canonical_path(path: &Path) -> Result<String> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    let resolved = fs::canonicalize(&abs).unwrap_or(abs);
    Ok(resolved.display().to_string())
}

fn append_record(ledger: &Path, rec: &SealRecord) -> Result<()> {
    if let Some(parent) = ledger.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(ledger)?;
    let line = serde_json::to_string(rec)?;
    writeln!(f, "{line}")?;
    Ok(())
}

fn read_all(ledger: &Path) -> Result<Vec<SealRecord>> {
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

fn latest_for_path<'a>(records: &'a [SealRecord], path_key: &str) -> Option<&'a SealRecord> {
    records.iter().rev().find(|r| r.path == path_key)
}

fn cmd_seal(ledger: &Path, path: &Path, message: Option<String>) -> Result<()> {
    if !path.is_file() {
        return Err(SealError::Msg(format!("not a file: {}", path.display())));
    }
    let (sha256, size) = hash_file(path)?;
    let path_key = canonical_path(path)?;
    let rec = SealRecord {
        ts: Utc::now(),
        path: path_key.clone(),
        sha256: sha256.clone(),
        size,
        message,
    };
    append_record(ledger, &rec)?;
    println!("sealed  {}", short_hash(&sha256));
    println!("path    {path_key}");
    println!("size    {size} bytes");
    println!("time    {}", rec.ts.to_rfc3339());
    println!("ledger  {}", ledger.display());
    Ok(())
}

fn cmd_verify(ledger: &Path, path: &Path) -> Result<()> {
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
        println!("OK     matches seal at {}", prev.ts.to_rfc3339());
        println!("hash   {}", short_hash(&sha256));
        println!("path   {path_key}");
        return Ok(());
    }
    Err(SealError::HashMismatch {
        path: path_key,
        recorded: short_hash(&prev.sha256),
        current: short_hash(&sha256),
    })
}

fn cmd_list(ledger: &Path, path_filter: Option<&Path>, limit: usize) -> Result<()> {
    let mut records = read_all(ledger)?;
    if let Some(p) = path_filter {
        let key = canonical_path(p).unwrap_or_else(|_| p.display().to_string());
        records.retain(|r| r.path == key || r.path.ends_with(&key));
    }
    records.reverse();
    let n = records.len().min(limit);
    if n == 0 {
        println!("(no seals in {})", ledger.display());
        return Ok(());
    }
    println!("{:<20}  {:<14}  {:>10}  path", "time", "sha256", "size");
    for r in records.iter().take(n) {
        let t = r.ts.format("%Y-%m-%d %H:%M:%S").to_string();
        println!(
            "{:<20}  {}  {:>10}  {}",
            t,
            short_hash(&r.sha256),
            r.size,
            r.path
        );
        if let Some(ref m) = r.message {
            println!("                      note: {m}");
        }
    }
    println!("ledger: {}  (showing {n})", ledger.display());
    Ok(())
}

fn short_hash(h: &str) -> String {
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

        cmd_seal(&ledger, &file, Some("test note".into())).unwrap();

        // Verify should succeed
        cmd_verify(&ledger, &file).unwrap();

        assert!(ledger.exists());
        let recs = read_all(&ledger).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].message, Some("test note".into()));
        assert_eq!(recs[0].size, 11);
    }

    #[test]
    fn test_short_hash() {
        assert_eq!(short_hash("abc"), "abc");
        assert_eq!(short_hash("1234567890abcdef"), "1234567890abcdef");
        let long = "1234567890abcdef1234567890abcdef";
        assert!(short_hash(long).contains('\u{2026}'));
    }
}
