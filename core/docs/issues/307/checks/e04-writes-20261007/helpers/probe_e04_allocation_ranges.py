"""Two intentional identical successful range requests per fresh file; no retry."""
import ctypes
import json
import os
from pathlib import Path
import subprocess
import time

libc = ctypes.CDLL(None, use_errno=True)
libc.fallocate.argtypes = (ctypes.c_int, ctypes.c_int, ctypes.c_longlong, ctypes.c_longlong)
libc.fallocate.restype = ctypes.c_int
paths = [Path('/work/core/target/cluster2-307/e04-reservation-range-probe-20261007.bin'),
         Path('/tmp/e04-reservation-range-control-20261007.bin')]
started = time.monotonic_ns()
rows = []
failed = None
for path in paths:
    fd = None
    row = {'path': str(path), 'calls': [], 'profile': 'diagnostic-two-identical-64k-keep-size-ranges',
           'close_error': None}
    rows.append(row)
    def snapshot():
        value = os.fstat(fd)
        return {'dev': value.st_dev, 'inode': value.st_ino, 'links': value.st_nlink,
                'logical_bytes': value.st_size, 'allocated_bytes': value.st_blocks * 512}
    try:
        row['filesystem'] = subprocess.check_output(
            ['stat', '-f', '--printf', '%T %t %s %S', str(path.parent)], timeout=2).decode()
        row['mountinfo'] = [s for s in Path('/proc/self/mountinfo').read_text().splitlines()
                            if ' /work ' in s or ' / ' in s]
        fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_RDWR, 0o600)
        row['before'] = snapshot()
        for index in range(2):
            ctypes.set_errno(0)
            opened = time.monotonic_ns()
            result = libc.fallocate(fd, 1, 0, 65536)
            closed = time.monotonic_ns()
            error = ctypes.get_errno()
            state = snapshot()
            row['calls'].append({'index': index, 'flags': 'FALLOC_FL_KEEP_SIZE=1', 'offset': 0,
                                 'length': 65536, 'result': result, 'errno': error,
                                 'opened_ns': opened, 'closed_ns': closed, 'after': state})
            if result != 0:
                raise OSError(error, os.strerror(error))
            if state['logical_bytes'] != 0 or any(state[k] != row['before'][k] for k in ['dev', 'inode', 'links']):
                raise RuntimeError('original file identity/logical length changed')
        row['same_range_added_bytes'] = row['calls'][1]['after']['allocated_bytes'] - row['calls'][0]['after']['allocated_bytes']
        row['range_idempotence'] = 'OBSERVED_EQUAL' if row['same_range_added_bytes'] == 0 else 'OBSERVED_ADDITIVE'
    except BaseException as error:
        failed = repr(error)
        row['original_error'] = failed
    finally:
        if fd is not None:
            try:
                os.close(fd)
            except BaseException as error:
                row['close_error'] = repr(error)
                if failed is None:
                    failed = row['close_error']
    if failed is not None:
        break
print(json.dumps({'schema': 'e04-exact-range-cause-diagnostic-v1', 'rows': rows,
                  'status': 'RECORDED' if failed is None else 'FAILED', 'original_error': failed,
                  'elapsed_ns': time.monotonic_ns() - started, 'files_retained': True,
                  'sample_count': 0, 'qualification_status': 'NOT_EVALUATED',
                  'cache': 'uncontrolled', 'scope': 'primitive idempotence only, not full-device capacity qualification'}, indent=2))
raise SystemExit(0 if failed is None else 1)
