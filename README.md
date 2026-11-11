# file-seal

**Local file integrity seals** — fingerprint a file, remember it, prove later that it did not change.

> No upload. No account. SHA-256 + a local ledger.

```bash
file-seal seal ./contract.pdf
file-seal verify ./contract.pdf
file-seal list
```

## Why

You send a contract, design, or manuscript. Later someone claims a different version.
If you **sealed** the file first, you can show: *this exact bytes existed at this time on my machine*.

This is **not** a notary, blockchain, or court-grade service. It is a practical local evidence trail.

## Install

```bash
cargo install --git https://github.com/4jb5nkddkb-dot/file-seal --locked
# or from a clone:
cargo build --release
# binary: target/release/file-seal
```

## Usage

```bash
# Seal a file (records SHA-256 + timestamp + size)
file-seal seal ./contract.pdf
file-seal seal ./contract.pdf -m "sent to client A"

# Verify against the latest seal for that path
file-seal verify ./contract.pdf

# List seals (newest first)
file-seal list
file-seal list --path ./contract.pdf
```

Ledger default: `~/.local/share/file-seal/seals.jsonl`  
Override: `FILE_SEAL_LEDGER=/path/to/seals.jsonl` or `--ledger`.

## What it is / is not

| Is | Is not |
|----|--------|
| Local SHA-256 + timestamp ledger | Blockchain / public timestamp authority |
| Detects any byte change after seal | Proof you are the author |
| Offline by design | Legal substitute for notarization |
| Simple CLI | Full DMS / e-sign product |

## Demo (30 seconds)

```bash
echo "version one" > /tmp/demo.txt
file-seal seal /tmp/demo.txt
file-seal verify /tmp/demo.txt          # OK
echo "version two" > /tmp/demo.txt
file-seal verify /tmp/demo.txt          # FAIL hash mismatch
```

## Status

**v0.1.0** — minimal product slice: `seal` / `verify` / `list`.

## License

MIT
