#!/usr/bin/env python3
"""Static acceptance gate for roadmap stages.

This checker is intentionally dependency-free. It does not replace unit,
fuzz, integration, or platform tests; it catches missing implementation and
CI contracts before those tests are scheduled.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


STAGES = ("baseline", "multi-ledger", "signing", "hardening", "release")


def files_text(repo: Path) -> str:
    chunks: list[str] = []
    for root in (repo / "src", repo / ".github" / "workflows"):
        if root.exists():
            for path in sorted(root.rglob("*")):
                if path.is_file() and path.suffix in {".rs", ".yml", ".yaml", ".toml", ".md"}:
                    chunks.append(path.read_text(encoding="utf-8", errors="replace"))
    return "\n".join(chunks)


def evaluate(repo: Path, stage: str) -> list[dict[str, object]]:
    text = files_text(repo).lower()
    results: list[dict[str, object]] = []

    def check(name: str, ok: bool, detail: str) -> None:
        results.append({"name": name, "passed": ok, "detail": detail})

    check("repository manifest", (repo / "Cargo.toml").is_file(), "Cargo.toml exists")
    check("repository source", (repo / "src").is_dir(), "src/ exists")
    check(
        "workflow directory",
        (repo / ".github" / "workflows").is_dir(),
        ".github/workflows/ exists",
    )

    if stage in {"baseline", "multi-ledger", "signing", "hardening", "release"}:
        for token in ("verify_chain", "prev_hash", "record_hash", "compute_hash"):
            check(
                f"hash-chain symbol: {token}",
                token in text,
                f"source contains {token}",
            )
        for token in ("tamper", "corrupt", "legacy"):
            check(
                f"negative-test marker: {token}",
                token in text,
                f"source or tests contain {token}",
            )
        check("strict Rust CI", "cargo test" in text and "cargo clippy" in text, "quality commands exist")

    if stage == "multi-ledger":
        for token in ("ledger list", "ledger create", "ledger migrate"):
            check(
                f"multi-ledger command: {token}",
                token in text,
                f"CLI contract contains {token}",
            )
        check("migration safety", "atomic" in text and "legacy" in text, "migration terminology exists")

    if stage == "signing":
        for token in ("ed25519", "signature", "argon2"):
            check(f"signing primitive or API: {token}", token in text, f"source contains {token}")
        check(
            "authenticated encryption",
            "chacha20-poly1305" in text or "aes-gcm" in text,
            "an AEAD implementation is named",
        )
        check("trust model", "trusted" in text and "fingerprint" in text, "trust metadata exists")

    if stage == "hardening":
        for token in ("lock", "atomic", "sync", "sarif"):
            check(f"hardening marker: {token}", token in text, f"source or workflow contains {token}")
        check("recovery or conflict language", "recover" in text or "conflict" in text, "recovery contract exists")

    if stage == "release":
        check(
            "release packaging",
            "upload-artifact" in text and ("release" in text or "gh release" in text),
            "artifact upload and release publication exist",
        )
        check(
            "checksums",
            "sha256sum" in text or "get-filehash" in text,
            "SHA-256 generation exists",
        )
        check("release write permission", "contents: write" in text, "publish permission is declared")

    return results


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("repo", type=Path, help="repository to inspect")
    parser.add_argument(
        "--stage",
        choices=(*STAGES, "all"),
        default="baseline",
        help="roadmap stage to gate; default: baseline",
    )
    parser.add_argument("--json", action="store_true", help="emit machine-readable JSON")
    args = parser.parse_args()

    repo = args.repo.resolve()
    stages = STAGES if args.stage == "all" else (args.stage,)
    results: list[dict[str, object]] = []
    for stage in stages:
        stage_results = evaluate(repo, stage)
        results.extend({"stage": stage, **item} for item in stage_results)

    failed = [item for item in results if not item["passed"]]
    if args.json:
        print(json.dumps({"repo": str(repo), "results": results, "failed": len(failed)}, ensure_ascii=False, indent=2))
    else:
        print(f"Roadmap acceptance: {repo}")
        for item in results:
            status = "PASS" if item["passed"] else "FAIL"
            print(f"[{status}] {item['stage']}: {item['name']} — {item['detail']}")
        print(f"Result: {len(results) - len(failed)}/{len(results)} checks passed")

    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
