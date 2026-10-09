#!/usr/bin/env python3
"""Untimed metadata-only sealing of the actual native fixture's regular paths."""
import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import resource
import stat
import time

from residency import validate_path
from stream_manifest import MANIFEST_SCHEMA, pinned_identity


def generate(root, output):
    root = validate_path(root).resolve(strict=True)
    output = validate_path(output)
    actual_output = output.parent.resolve(strict=True) / output.name
    if actual_output == root or root in actual_output.parents:
        raise ValueError('sealed manifest must live outside actual native fixture')
    initial = root.stat()
    start = time.monotonic_ns()
    records = []
    # Preparation intentionally retains/sorts metadata; its population and
    # own process high-water mark are reported in the setup domain only.
    def fail_walk(error):
        raise error
    for directory, _, names in os.walk(root, followlinks=False, onerror=fail_walk):
        for name in names:
            path = Path(directory) / name
            info = path.lstat()
            if stat.S_ISREG(info.st_mode):
                records.append(dict(path=path.relative_to(root).as_posix(), **pinned_identity(info)))
    records.sort(key=lambda row: (row['device'], row['inode'], row['path']))
    physical = sum(index == 0 or (row['device'], row['inode']) !=
                   (records[index-1]['device'], records[index-1]['inode'])
                   for index, row in enumerate(records))
    digest = hashlib.sha256()
    header = dict(schema=MANIFEST_SCHEMA, root=str(root), root_device=initial.st_dev,
                  root_inode=initial.st_ino, order='device-inode-path',
                  regular_files=len(records), physical_files=physical,
                  source='actual native copy metadata only; no regular payload read')
    with output.open('xb') as stream:
        previous_inode = canonical = None
        for row in itertools.chain((header,), records):
            if row is not header:
                inode = row['device'], row['inode']
                if inode == previous_inode:
                    row['alias_of'] = canonical
                else:
                    canonical = row['path']
                previous_inode = inode
            raw = (json.dumps(row, sort_keys=True) + '\n').encode()
            stream.write(raw)
            digest.update(raw)
    after = root.stat()
    if (initial.st_dev, initial.st_ino) != (after.st_dev, after.st_ino):
        raise ValueError('actual root identity changed during manifest preparation')
    return dict(schema=MANIFEST_SCHEMA, status='SEALED_SETUP_ONLY', root=str(root),
                manifest=str(output), sha256=digest.hexdigest(), regular_files=len(records),
                physical_files=physical, declared_aliases=len(records)-physical,
                setup_metadata_rows_retained=len(records), payload_bytes_read=0,
                setup_ns=time.monotonic_ns()-start,
                setup_process_ru_maxrss=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                setup_ru_maxrss_units='KiB on Linux; bytes on macOS; process lifetime high-water',
                resource_domain='manifest preparation only; never daemon or measured phase',
                attempts=0, admission_eligible=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(generate(args.root, args.output), sort_keys=True))


if __name__ == '__main__':
    main()
