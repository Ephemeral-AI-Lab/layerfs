#!/usr/bin/env python3
"""Freeze one arm's exact identity: source, product/harness hashes, binary, patches."""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent
REPO = CAMPAIGN.parents[5]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def tracked(prefix: str) -> dict[str, str]:
    files = subprocess.check_output(
        ["git", "ls-files", prefix], cwd=REPO, text=True).split()
    return {name: sha256(REPO / name) for name in sorted(files)}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arm")
    parser.add_argument("binary")
    parser.add_argument("--patches", nargs="*", default=[])
    args = parser.parse_args()
    binary = Path(args.binary).resolve()
    document = {
        "source_commit": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip(),
        "source_tree": subprocess.check_output(
            ["git", "rev-parse", "HEAD^{tree}"], cwd=REPO, text=True).strip(),
        "source_dirty": bool(subprocess.check_output(
            ["git", "status", "--porcelain"], cwd=REPO, text=True).strip()),
        "product_files": tracked("core/crates"),
        "harness_files": tracked("core/benchmark"),
        "locks": {name: sha256(REPO / name)
                  for name in ("core/Cargo.lock",
                               "core/benchmark/fs-bench-pro-storage-content/Cargo.lock")},
        "patches": {Path(p).name: sha256(Path(p).resolve()) for p in args.patches},
        "binary": str(binary),
        "binary_sha256": sha256(binary),
        "build_reuse": "incremental host build in this worktree's own Cargo target; "
                       "immutable executable copy",
        "cache_contract": "fresh-growing-store; uncontrolled OS/intra-chain residency; "
                          "admission INELIGIBLE",
    }
    out = CAMPAIGN / f"{args.arm}-identity.json"
    assert not out.exists(), f"refusing to overwrite {out}"
    out.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"arm": args.arm, "binary_sha256": document["binary_sha256"],
                      "dirty": document["source_dirty"],
                      "files": len(document["product_files"])}, indent=2))


if __name__ == "__main__":
    main()
