#!/usr/bin/env python3
"""Read the retained frontier root using strict response-byte budgets; functional only."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'core/crates/layerfs-daemon/tests'))
import history_route as route
sys.path.insert(0, str(ROOT / 'core/crates/layerfs-workspace/tests'))
import payload_route as payload
sys.path.insert(0, str(ROOT / 'core/benchmark/fs-bench-pro-storage-content/shared'))
import isolation


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--store', type=Path, required=True)
    parser.add_argument('--root', required=True)
    parser.add_argument('--binaries', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.store = args.store.resolve(); args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False); route.BIN = args.binaries.resolve()
    root = bytes.fromhex(args.root); assert len(root) == 32
    sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    report = {'status': 'FAIL', 'mode': 'strict-fragmented-saved-root', 'performance_claim': False,
              'cache_claim': None, 'hard_budget_seconds': 60, 'filesystem_root': root.hex(),
              'source': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'product_inputs_sha256': payload.product_inputs(), 'driver_sha256': sha(Path(__file__)),
              'binaries': {name: sha(route.BIN / name) for name in ('layerfs-server', 'layerfs-daemon', 'examples/public_key')},
              'fixture': {'store': str(args.store), 'sha256': sha(args.store), 'clone_method': 'independent-byte-copy',
                          'use': 'read-only saved objects; no resumed history authority'}, 'rows': []}
    start = time.monotonic(); service = daemon = None
    def expired(_signal, _frame): raise TimeoutError('complete functional command exceeded60seconds')
    signal.signal(signal.SIGALRM, expired); signal.alarm(60)
    try:
        space = isolation.namespace()
        for path in (args.store, args.output, route.BIN): space.assert_owned(path, 'functional proof')
        report['resource_isolation'] = isolation.observe(space)
        with space.lock_path.open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            shutil.copyfile(args.store, args.output / 'store.sqlite')
            assert sha(args.output / 'store.sqlite') == report['fixture']['sha256']
            private, client = os.urandom(32).hex(), os.urandom(32).hex()
            public, peer = route.public_key(private), route.public_key(client)
            service, ready = route.start_service(args.output, 0, private, f'1,{peer},{int(time.time())+3600},255')
            port = int(ready.rsplit(':', 1)[1])
            daemon, _ = route.start_daemon(args.output, port, client, public, 1, None)
            kind, body = route.exchange(daemon, 1, 2, root + b'\x04' + route.blob(b'data.bin'))
            assert kind == 6 and body[0] == 9
            content = body[18:50]; report['content_root'] = content.hex()
            daemon.stdin.close(); assert daemon.wait(timeout=5) == 0; daemon = None
            for length in (257, 258, 514):
                daemon, _ = route.start_daemon(args.output, port, client, public, 1, None)
                metadata = struct.pack('>QIHIQB', 1, 1, 1, 10000, length, 1) + content + struct.pack('>QQ', 0, length)
                daemon.stdin.write(route.frame(2, 1, metadata) + route.frame(4, 1, struct.pack('>Q', 0)))
                daemon.stdin.flush(); data = bytearray(); frames = 0; end = time.monotonic() + 10
                while True:
                    header = route.read_exact(daemon.stdout.fileno(), 20, end)
                    assert header[:4] == b'LFB1'
                    kind, flags, reserved, identity, size = struct.unpack('>BBHQI', header[4:])
                    assert flags == reserved == 0 and identity == 1 and size <= 32768
                    body = route.read_exact(daemon.stdout.fileno(), size, end)
                    if kind != 5: break
                    data.extend(body); frames += 1; assert len(data) <= length
                expected = bytes((ord('y') if i < 128 else ord('x')) if i < 512 and i % 2 == 0 else i % 251 for i in range(length))
                row = {'requested_bytes': length, 'response_budget': length, 'data_frames': frames,
                       'received_bytes': len(data), 'terminal_kind': kind, 'terminal_hex': body.hex(),
                       'exact_bytes': bytes(data) == expected}
                report['rows'].append(row)
                assert kind == 6 and body == b'\x01' + struct.pack('>Q', length) and row['exact_bytes']
                assert frames == 1
                daemon.stdin.close(); assert daemon.wait(timeout=5) == 0; daemon = None
            report['status'] = 'PASS'
    except BaseException as error:
        report['failure'] = repr(error); raise
    finally:
        if daemon is not None:
            daemon.kill(); daemon.wait(timeout=5)
        if service is not None:
            service.stdin.close(); service.wait(timeout=5)
            (args.output / 'service.stderr').write_bytes(service.stderr.read())
            report['service_exit'] = service.returncode
        signal.alarm(0); report['command_wall_seconds'] = time.monotonic() - start
        (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__': main()
