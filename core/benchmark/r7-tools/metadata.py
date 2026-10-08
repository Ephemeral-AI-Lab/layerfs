"""Reconcile supported metadata on an owned prepared copy before its first Init."""
import argparse
import json
import os
from pathlib import Path
import shutil
import stat


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--source", type=Path, required=True)
    p.add_argument("--copy", type=Path, required=True)
    args = p.parse_args()
    if not args.copy.is_relative_to(Path("/tmp")) or not args.copy.name.startswith("layerfs-r7-"):
        p.error("copy must be this stage's owned /tmp prepared fixture")
    counts = dict(entries=0, symlink_times_corrected=0, ownership_mismatches=0,
                  mode_mismatches=0, mtime_mismatches_after=0)
    for directory, dirs, files in os.walk(args.source, followlinks=False):
        for name in [".", *dirs, *files]:
            source = Path(directory) / name
            copied = args.copy / source.relative_to(args.source)
            original, actual = source.lstat(), copied.lstat()
            counts["entries"] += 1
            counts["ownership_mismatches"] += (original.st_uid, original.st_gid) != (actual.st_uid, actual.st_gid)
            counts["mode_mismatches"] += original.st_mode != actual.st_mode
            if stat.S_ISLNK(original.st_mode) and original.st_mtime_ns != actual.st_mtime_ns:
                shutil.copystat(source, copied, follow_symlinks=False)
                counts["symlink_times_corrected"] += 1
            counts["mtime_mismatches_after"] += original.st_mtime_ns != copied.lstat().st_mtime_ns
    counts["status"] = "PASS" if not any(counts[k] for k in
        ("ownership_mismatches", "mode_mismatches", "mtime_mismatches_after")) else "FAILED_METADATA"
    print(json.dumps(counts, indent=2))
    if counts["status"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
