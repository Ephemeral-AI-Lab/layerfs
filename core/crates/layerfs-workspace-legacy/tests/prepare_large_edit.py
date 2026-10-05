#!/usr/bin/env python3
"""Prepare one closed 64 MiB immutable-base fixture through existing public operations.

This is reusable fixture preparation, not a Workspace edit or a measurement.
A byte copy reuses the R1 Store; a fresh history catalog supplies its own live
producer, because reopening the old catalog does not grant mutation continuity.
"""
import argparse
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
import mounted_read as mounted

SIZE = 64 * 1024 * 1024


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--store-master', required=True, type=Path)
    parser.add_argument('--binaries', required=True, type=Path)
    parser.add_argument('--producer-source', required=True)
    parser.add_argument('--producer-seal', required=True)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    args.output = args.output.resolve(); args.output.mkdir(parents=True, exist_ok=False)
    route.BIN = args.binaries.resolve()
    started = time.monotonic(); service = daemon = None; readiness = ''
    report = {'status': 'FAIL', 'mode': 'prepared-local-edit-large-fixture', 'logical_bytes': SIZE,
              'recipe': 'byte at absolute offset i is i % 251', 'performance_claim': False,
              'cache_claim': None, 'producer_source': args.producer_source,
              'producer_product_inputs_sha256': args.producer_seal,
              'driver_sha256': sha(Path(__file__)), 'hard_budget_seconds': 60}
    def expired(_signal, _frame):
        raise TimeoutError('large fixture preparation exceeded its 60-second budget')
    signal.signal(signal.SIGALRM, expired); signal.alarm(60)
    try:
        report['binary_sha256'] = {name: sha(route.BIN / name) for name in ('layerfs-daemon', 'layerfs-server', 'examples/public_key')}
        data = args.output / 'service'; data.mkdir()
        report['store_master_sha256'] = sha(args.store_master)
        shutil.copyfile(args.store_master, data / 'store.sqlite')
        report['clone_method'] = 'independent-byte-copy of closed Store; fresh history catalog'
        server_key, client_key = os.urandom(32).hex(), os.urandom(32).hex()
        server_public, client_public = route.public_key(server_key), route.public_key(client_key)
        env = os.environ.copy()
        # The recipe is `byte at absolute offset i is i % 251`; write it in
        # bounded windows so preparation never holds the file resident.
        source = data / 'source'; source.mkdir()
        cycle = bytes(range(251)) * 67
        with (source / 'data.bin').open('wb') as output:
            for offset in range(0, SIZE, route.FRAME_BYTES):
                length = min(route.FRAME_BYTES, SIZE - offset)
                chunk = cycle[offset % 251:offset % 251 + length]
                assert len(chunk) == length
                output.write(chunk)
        assert (source / 'data.bin').stat().st_size == SIZE
        env.update(LAYERFS_IMPORT_ROOT=str(source),
                   LAYERFS_PRIVATE_KEY=server_key, LAYERFS_PEERS=f'1,{client_public},{int(time.time())+3600},127',
                   LAYERFS_LISTEN='127.0.0.1:0', LAYERFS_STORE=str(data / 'store.sqlite'),
                   LAYERFS_HISTORY_CATALOG=str(data / 'history.sqlite'), LAYERFS_HISTORY_CREATE='1',
                   LAYERFS_HISTORY_BINDING='pair1-mounted-read', LAYERFS_HISTORY_INCARNATION='1',
                   LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(), LAYERFS_TELEMETRY='off',
                   LAYERFS_CONSTRUCTION_WORKERS='1')
        service = subprocess.Popen([route.BIN / 'layerfs-server'], env=env, stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        readiness = mounted.line_until(service, timeout=10)
        assert 'ready' in readiness
        port = int(readiness.strip().rsplit(':', 1)[1])
        daemon, _ = route.start_daemon(data, port, client_key, server_public, 1, None)
        daemon.stdin.write(route.begin(1, 20, route.save_file_metadata(SIZE)))
        daemon.stdin.write(route.frame(3, 1, struct.pack('>QQQ', 1, 0, SIZE)))
        digest = hashlib.sha256()
        for offset in range(0, SIZE, route.FRAME_BYTES):
            length = min(route.FRAME_BYTES, SIZE-offset)
            chunk = cycle[offset % 251:offset % 251 + length]
            assert len(chunk) == length
            digest.update(chunk); daemon.stdin.write(route.frame(3, 1, chunk))
        daemon.stdin.write(route.frame(4, 1, struct.pack('>Q', SIZE + 24))); daemon.stdin.flush()
        kind, body = route.receive(daemon, timeout=10)
        assert kind == 6 and body[0] == 2, body
        content = body[1:33]; report['input_sha256'] = digest.hexdigest(); report['file_root'] = content.hex()
        report['namespace_route'] = 'native-directory import of the same bytes under the same canonical root'
        init = b'\x09' + b'\x91'*16 + route.blob(b'large-local-edit') + bytes(range(32))
        kind, body = route.exchange(daemon, 2, route.COMMAND_OPCODE, init, route.HISTORY_PROFILE)
        assert kind == 6, body
        tag, created = route.history(body); assert tag == 'StackCreated'
        fork = b'\x02' + created['stack'] + b'\x92'*16 + route.blob(b'large') + b'\x01' + created['head_layer']
        kind, body = route.exchange(daemon, 3, route.COMMAND_OPCODE, fork, route.HISTORY_PROFILE)
        assert kind == 6, body
        tag, snapshot = route.history(body); assert tag == 'BranchSnapshot'
        branch = snapshot['branch']['branch']
        kind, body = route.exchange(daemon, 4, 2, created['root'] + b'\x01' + route.blob(b'data.bin'))
        assert kind == 6 and body[0] == 4, body
        serial = struct.unpack('>Q', body[1:9])[0]
        prepared = (b'\x05' + b'\x93'*32 + branch + route.optional(None) + created['head_layer']
                    + struct.pack('>Q', 1) + created['root'] + snapshot['scope']
                    + struct.pack('>QH', created['root_serial'], 1) + struct.pack('>QH', created['root_serial'], 1)
                    + route.blob(b'alias') + struct.pack('>QH', serial, 0))
        kind, body = route.exchange(daemon, 5, route.COMMAND_OPCODE, prepared, route.HISTORY_PROFILE)
        assert kind == 6, body
        tag, committed = route.history(body); assert tag == 'Committed'
        report.update(root=committed['root'].hex(), branch=branch.hex(), root_serial=created['root_serial'])
        daemon.stdin.close(); assert daemon.wait(timeout=6) == 0
        service.stdin.close(); assert service.wait(timeout=6) == 0
        assert sha(args.store_master) == report['store_master_sha256']
        report['closed_master_sha256'] = {name: sha(data / name) for name in ('store.sqlite', 'history.sqlite')}
        report['status'] = 'PASS'; report['cleanup'] = 'PASS: producer processes exited; closed master retained for byte-copy reuse'
    except BaseException as error:
        report['failure'] = repr(error); raise
    finally:
        signal.alarm(0)
        for label, process in [('daemon', daemon), ('service', service)]:
            if process is not None:
                if process.poll() is None:
                    process.kill(); process.wait(timeout=6); report[label + '_forced_cleanup'] = True
                (args.output / (label + '.stderr')).write_bytes((readiness.encode() if label == 'service' else b'') + process.stderr.read())
        report['command_wall_seconds'] = time.monotonic() - started
        (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
