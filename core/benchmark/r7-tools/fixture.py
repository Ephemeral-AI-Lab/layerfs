"""One independent byte copy of the protected R7 fixture, preserving aliases.

All commands run in the destination. The source is opened only for reading.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--receipt", type=Path, required=True)
    args = parser.parse_args()
    if args.destination.exists() or args.receipt.exists():
        parser.error("destination and receipt must be fresh")
    if args.destination.is_relative_to(args.source):
        parser.error("destination must be outside source")
    start = time.monotonic_ns()
    aliases = {}
    counts = dict(files=0, directories=0, symlinks=0, regular_bytes=0, linked_aliases=0)
    digest = hashlib.sha256()
    args.destination.mkdir(parents=True)
    for directory, dirs, files in os.walk(args.source, followlinks=False):
        relative = Path(directory).relative_to(args.source)
        destination = args.destination / relative
        names = sorted(dirs + files)
        for name in names:
            source = Path(directory) / name
            target = destination / name
            metadata = source.lstat()
            identity = str(relative / name).encode(errors="surrogateescape")
            digest.update(identity + b"\0" + str(metadata.st_mode).encode() + b"\0")
            if stat.S_ISLNK(metadata.st_mode):
                link = os.readlink(source)
                target.symlink_to(link)
                shutil.copystat(source, target, follow_symlinks=False)
                digest.update(os.fsencode(link) + b"\0")
                counts["symlinks"] += 1
            elif stat.S_ISDIR(metadata.st_mode):
                target.mkdir()
                counts["directories"] += 1
            elif stat.S_ISREG(metadata.st_mode):
                key = (metadata.st_dev, metadata.st_ino)
                if metadata.st_nlink > 1 and key in aliases:
                    os.link(aliases[key], target)
                    counts["linked_aliases"] += 1
                else:
                    shutil.copy2(source, target, follow_symlinks=False)
                    if metadata.st_nlink > 1:
                        aliases[key] = target
                counts["files"] += 1
                counts["regular_bytes"] += metadata.st_size
                digest.update(str(metadata.st_size).encode() + b"\0")
            else:
                raise RuntimeError(f"unsupported fixture kind: {source}")
            os.chown(target, metadata.st_uid, metadata.st_gid, follow_symlinks=False)
        # Directory metadata is restored after children below.
    for directory, _, _ in os.walk(args.destination, topdown=False, followlinks=False):
        source = args.source / Path(directory).relative_to(args.destination)
        shutil.copystat(source, directory, follow_symlinks=False)
        metadata = source.lstat()
        os.chown(directory, metadata.st_uid, metadata.st_gid, follow_symlinks=False)
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=args.destination, text=True).strip()
    result = dict(schema="layerfs-r7-fixture-copy-v1", source=str(args.source),
                  destination=str(args.destination), copied_commit=commit,
                  expected_commit="639ed015397290b3745d163aafe02ffee4aa3f84",
                  counts=counts, shape_sha256=digest.hexdigest(),
                  copy_ns=time.monotonic_ns() - start, clone_method="independent byte copy; symlinks and destination-only hard links",
                  complete_fixture=True, admission_eligible=False,
                  status="PASS" if commit == "639ed015397290b3745d163aafe02ffee4aa3f84" else "FAILED_IDENTITY")
    args.receipt.parent.mkdir(parents=True, exist_ok=True)
    args.receipt.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    if result["status"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
