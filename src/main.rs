//! file-seal CLI — local SHA-256 integrity ledger.

use clap::{Parser, Subcommand};
use file_seal::{self, SealError};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "file-seal",
    version,
    about = "Local file integrity seals (SHA-256 + ledger)"
)]
struct Cli {
    /// Override ledger path (default: platform data dir / file-seal / seals.jsonl)
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

fn run() -> file_seal::Result<()> {
    let cli = Cli::parse();
    let ledger = file_seal::resolve_ledger(cli.ledger.as_deref())?;

    match cli.cmd {
        Command::Seal { path, message } => {
            let rec = file_seal::seal_file(&ledger, &path, message)?;
            println!("sealed  {}", file_seal::short_hash(&rec.sha256));
            println!("path    {}", rec.path);
            println!("size    {} bytes", rec.size);
            println!("time    {}", rec.ts.to_rfc3339());
            println!("ledger  {}", ledger.display());
            Ok(())
        }
        Command::Verify { path } => {
            let prev = file_seal::verify_file(&ledger, &path)?;
            println!("OK     matches seal at {}", prev.ts.to_rfc3339());
            println!("hash   {}", file_seal::short_hash(&prev.sha256));
            println!("path   {}", prev.path);
            Ok(())
        }
        Command::List { path, limit } => {
            let records = file_seal::list_seals(&ledger, path.as_deref(), limit)?;
            if records.is_empty() {
                println!("(no seals in {})", ledger.display());
                return Ok(());
            }
            println!("{:<20}  {:<14}  {:>10}  path", "time", "sha256", "size");
            for r in &records {
                let t = r.ts.format("%Y-%m-%d %H:%M:%S").to_string();
                println!(
                    "{:<20}  {}  {:>10}  {}",
                    t,
                    file_seal::short_hash(&r.sha256),
                    r.size,
                    r.path
                );
                if let Some(ref m) = r.message {
                    println!("                      note: {m}");
                }
            }
            println!("ledger: {}  (showing {})", ledger.display(), records.len());
            Ok(())
        }
    }
}
