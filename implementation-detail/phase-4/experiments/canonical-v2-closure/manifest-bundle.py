#!/usr/bin/env python3
"""Write or verify complete path/SHA-256/size evidence manifests."""

import csv
import hashlib
import sys
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(repo, root, manifest, verification):
    excluded = {manifest.resolve(), verification.resolve()}
    paths = sorted(path for path in root.rglob("*") if path.is_file() and path.resolve() not in excluded)
    manifest.write_text("path\tsha256\tsize_bytes\n" + "".join(
        f"{path.relative_to(repo)}\t{digest(path)}\t{path.stat().st_size}\n" for path in paths))
    return verify(repo, manifest)


def verify(repo, manifest):
    rows = list(csv.DictReader(manifest.open(), delimiter="\t"))
    mismatches = []
    for row in rows:
        path = repo / row["path"]
        actual = digest(path) if path.is_file() else "MISSING"
        size = path.stat().st_size if path.is_file() else -1
        if actual != row["sha256"] or size != int(row["size_bytes"]):
            mismatches.append(row["path"])
    return rows, mismatches


def main():
    if len(sys.argv) not in (4, 6) or sys.argv[1] not in {"write", "verify"}:
        raise SystemExit("usage: manifest-bundle.py verify REPO MANIFEST | write REPO ROOT MANIFEST VERIFICATION")
    mode, repo = sys.argv[1], Path(sys.argv[2]).resolve()
    if mode == "verify":
        manifest, verification = Path(sys.argv[3]).resolve(), None
        rows, mismatches = verify(repo, manifest)
    else:
        root, manifest, verification = map(lambda value: Path(value).resolve(), sys.argv[3:6])
        rows, mismatches = write(repo, root, manifest, verification)
        verification.write_text(
            f"status={'PASS' if not mismatches else 'FAIL'}\nentries={len(rows)}\n"
            f"mismatches={len(mismatches)}\nmanifest_sha256={digest(manifest)}\n")
    print(f"status={'PASS' if not mismatches else 'FAIL'} entries={len(rows)} mismatches={len(mismatches)}")
    return bool(mismatches)


if __name__ == "__main__":
    raise SystemExit(main())
