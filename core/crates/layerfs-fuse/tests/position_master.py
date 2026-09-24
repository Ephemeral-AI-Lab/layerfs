#!/usr/bin/env python3
"""Validate one atomically published v3 master and copy its Store/history."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil

from position_manifest import SIZES


def digest(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1 << 20), b""):
            value.update(block)
    return value.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--master", required=True, type=Path)
    parser.add_argument("--master-sha256", required=True)
    parser.add_argument("--size", required=True)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    raw = args.master.read_bytes()
    if hashlib.sha256(raw).hexdigest() != args.master_sha256:
        raise ValueError("master.json SHA-256 differs from declared identity")
    record = json.loads(raw)
    fixture = next((item for item in SIZES if item[0] == args.size), None)
    if fixture is None:
        raise ValueError("unknown fixture label")
    _, size, sha, root, count = fixture
    expected = {
        "schema": "core-fs-bench-pro-exec-fuse-edit-master-v3",
        "fixture_bytes": size,
        "fixture_sha256": sha,
        "fixture_canonical_root": root,
        "fixture_extent_count": count,
    }
    for key, value in expected.items():
        if record.get(key) != value:
            raise ValueError(f"master {key} mismatch")
    key = record.get("compatibility_key")
    if not isinstance(key, dict) or key.get("fixture_bytes") != size or key.get("fixture_sha256") != sha:
        raise ValueError("master compatibility key fixture mismatch")
    for field in ("benchmark_init_sha256", "cargo_lock_sha256"):
        if not isinstance(key.get(field), str) or len(key[field]) != 64:
            raise ValueError(f"master compatibility key missing {field}")
    if key.get("fixture_generator_seed") != 0x4C41594552465331:
        raise ValueError("master fixture generator mismatch")
    key_digest = hashlib.sha256(json.dumps(key, sort_keys=True).encode()).hexdigest()[:16]
    if record.get("compatibility_key_sha256") != key_digest:
        raise ValueError("master compatibility key digest mismatch")
    if args.master.parent.name != f"master-v3-{size}-{key_digest}":
        raise ValueError("master directory identity mismatch")
    if record.get("route") != "sdk-exec-fuse-range-splice-commit-v1":
        raise ValueError("master route mismatch")
    if (record.get("init_receipt") or {}).get("status") != "COMPLETE":
        raise ValueError("master Init was not complete")
    if set(path.name for path in args.master.parent.iterdir()) != {
        "master.json", "store.sqlite", "history.sqlite"
    }:
        raise ValueError("master directory is not the atomic three-file seal")
    for field, length in (("producer_source_commit", 40), ("producer_source_tree", 40),
                          ("producer_binary_sha256", 64)):
        value = record.get(field)
        if not isinstance(value, str) or len(value) != length:
            raise ValueError(f"master {field} identity missing")
        int(value, 16)
    if record["producer_binary_sha256"] != key["benchmark_init_sha256"]:
        raise ValueError("master producer binary differs from compatibility key")
    args.out.mkdir()  # fresh private directory; no copy may overwrite evidence
    copied = {}
    for name in ("store", "history"):
        source = Path(record[name]).resolve()
        if source != (args.master.parent / f"{name}.sqlite").resolve() or not source.is_file():
            raise ValueError(f"master {name} path is not in the closed master directory")
        if source.stat().st_size != record[f"{name}_bytes"]:
            raise ValueError(f"master {name} size mismatch")
        source_sha256 = digest(source)
        if source_sha256 != record[f"{name}_sha256"]:
            raise ValueError(f"master {name} digest mismatch")
        target = args.out / f"{name}.sqlite"
        with target.open("xb") as destination, source.open("rb") as original:
            shutil.copyfileobj(original, destination, 1 << 20)
        if source_sha256 != digest(target):
            raise ValueError(f"master {name} copy digest mismatch")
        copied[name] = (source_sha256, source.stat().st_size)
    fields = ("project_id", "genesis_layer", "genesis_root", "genesis_root_serial")
    if any(field not in record for field in fields):
        raise ValueError("master project identity incomplete")
    with (args.out / "master-spec.tsv").open("x") as output:
        for field in fields:
            output.write(f"{field}\t{record[field]}\n")
        output.write(f"master_sha256\t{args.master_sha256}\n")
        output.write(f"producer_source_commit\t{record['producer_source_commit']}\n")
        output.write(f"producer_source_tree\t{record['producer_source_tree']}\n")
        output.write(f"producer_binary_sha256\t{record['producer_binary_sha256']}\n")
        output.write(f"compatibility_key_sha256\t{key_digest}\n")
        output.write("clone_method\tindependent-writable-byte-copy\n")
        for name, (sha256, size) in copied.items():
            output.write(f"{name}_sha256\t{sha256}\n")
            output.write(f"{name}_bytes\t{size}\n")
    print(f"master copy accepted: {args.size} {args.master_sha256} {key_digest}")


if __name__ == "__main__":
    main()
