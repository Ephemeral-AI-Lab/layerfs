"""Verify supported metadata; reconcile an owned copy only when explicitly selected."""
import argparse
import json
import os
from pathlib import Path
import shutil
import stat
from seal_fixture import checked_roots, entries, within_path


def verify(source, copied, reconcile=False):
    source_root, copied_root = checked_roots(source, copied)
    counts = dict(entries=0, symlink_times_corrected=0, ownership_corrected=0,
                  ownership_mismatches=0, modes_corrected=0,
                  mode_mismatches=0, mtime_mismatches_after=0,
                  missing_copy_entries=0, unexpected_copy_entries=0, kind_mismatches=0)
    for relative, source in entries(source_root):
        copied = within_path(copied_root, relative)
        original = source.lstat()
        counts["entries"] += 1
        try:
            actual = copied.lstat()
        except FileNotFoundError:
            counts['missing_copy_entries'] += 1
            continue
        if stat.S_IFMT(original.st_mode) != stat.S_IFMT(actual.st_mode):
            counts['kind_mismatches'] += 1
            continue
        if reconcile and (original.st_uid, original.st_gid) != (actual.st_uid, actual.st_gid):
            os.chown(copied, original.st_uid, original.st_gid, follow_symlinks=False)
            counts["ownership_corrected"] += 1
        if reconcile:
            # chown may clear set-id bits; assess/reconcile fresh metadata.
            current = copied.lstat()
            if stat.S_IMODE(original.st_mode) != stat.S_IMODE(current.st_mode) and not stat.S_ISLNK(original.st_mode):
                os.chmod(copied, stat.S_IMODE(original.st_mode), follow_symlinks=False)
                counts['modes_corrected'] += 1
        current = copied.lstat()
        if reconcile and original.st_mtime_ns != current.st_mtime_ns:
            shutil.copystat(source, copied, follow_symlinks=False)
            counts["symlink_times_corrected"] += stat.S_ISLNK(original.st_mode)
        final = copied.lstat()
        counts["ownership_mismatches"] += (original.st_uid, original.st_gid) != (final.st_uid, final.st_gid)
        counts["mode_mismatches"] += stat.S_IMODE(original.st_mode) != stat.S_IMODE(final.st_mode)
        counts["mtime_mismatches_after"] += original.st_mtime_ns != final.st_mtime_ns
    for relative, _ in entries(copied_root):
        counterpart = within_path(source_root, relative)
        try:
            counterpart.lstat()
        except FileNotFoundError:
            counts['unexpected_copy_entries'] += 1
    counts['schema'] = 'r7-fixture-metadata-v2'
    counts['mode'] = 'explicit_reconcile' if reconcile else 'read_only_verify'
    counts['root_verified'] = True
    counts['source_written'] = False
    counts['copy_write_authorized'] = reconcile
    counts["status"] = "PASS" if not any(counts[k] for k in
        ("ownership_mismatches", "mode_mismatches", "mtime_mismatches_after",
         'missing_copy_entries', 'unexpected_copy_entries', 'kind_mismatches')) else "FAILED_METADATA"
    return counts


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--source", type=Path, required=True)
    p.add_argument("--copy", type=Path, required=True)
    p.add_argument("--reconcile", action="store_true")
    args = p.parse_args()
    try:
        counts = verify(args.source, args.copy, args.reconcile)
    except (OSError, ValueError) as error:
        counts = dict(schema='r7-fixture-metadata-v2', status='INCOMPLETE',
                      original_error=str(error), errno=getattr(error, 'errno', None),
                      source_written=False, copy_write_authorized=args.reconcile)
    print(json.dumps(counts, indent=2))
    return 0 if counts['status'] == 'PASS' else 1


if __name__ == "__main__":
    raise SystemExit(main())
