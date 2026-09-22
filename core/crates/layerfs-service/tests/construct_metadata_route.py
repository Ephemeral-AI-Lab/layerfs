#!/usr/bin/env python3
"""Standalone portable metadata over actual native Service; no file allocation."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import sys
import time

import prepared_directories_route as shared

CASES = {
    'construct': ['typed-construction-root-reuse-and-public-noop-readback', 'unchanged-namespace-and-history', 'cleanup'],
    'refusals': ['grant128-and-portable-field-refusals', 'unchanged-namespace-and-history', 'cleanup'],
}


class Principal(shared.Peer):
    def __init__(self, directory, port, private, public, selector):
        directory.mkdir()
        super().__init__(directory, port, private, public)
        self.connection += (selector,)
        self.attempts = []

    def call(self, opcode, payload, profile=1, failure=None, allow_local=False):
        self.attempts.append({'opcode': opcode, 'profile': profile, 'input_body_bytes': 0})
        return super().call(opcode, payload, profile, failure, allow_local)


def payload(kind, mode, seconds, nanoseconds):
    return struct.pack('>BIqI', kind, mode, seconds, nanoseconds)


def constructed(body):
    assert len(body) == 66
    r = shared.route.Reader(body)
    assert r.u8() == 19
    value = {'kind': r.u8(), 'mode': int.from_bytes(r.take(4), 'big'),
             'mtime': int.from_bytes(r.take(8), 'big', signed=True),
             'nanoseconds': int.from_bytes(r.take(4), 'big'), 'metadata': r.take(32),
             'inserted': r.u64(), 'reused': r.u64()}
    r.done()
    return value


def execute(args, report):
    fixture = json.loads(args.fixture.read_text())
    assert fixture['status'] == 'PASS'
    master = args.fixture.parent / 'service/store.sqlite'
    digest = shared.sha(master)
    assert digest == fixture['closed_master_sha256']['store.sqlite']
    for suffix in ('-wal', '-shm', '-journal'):
        assert not Path(str(master) + suffix).exists()
    directory = args.output / 'service'; directory.mkdir()
    shutil.copyfile(master, directory / 'store.sqlite')
    assert shared.sha(directory / 'store.sqlite') == digest
    report['fixture_reuse'] = {'receipt': str(args.fixture), 'receipt_sha256': shared.sha(args.fixture),
        'closed_store_sha256': digest, 'clone_method': 'independent-byte-copy',
        'history': 'fresh live CREATE=1; no copied catalog', 'cache_claim': None}
    ownership = args.output / 'owners.jsonl'
    ownership.write_text(json.dumps({'event': 'planned', 'case': args.case, 'output': str(args.output),
        'worktree': str(shared.ROOT), 'driver_pid': os.getpid(), 'service_pid': None,
        'service_executable': str(args.binaries / 'layerfs-service'), 'service_directory': str(directory)}) + '\n')
    report['ownership_journal'] = str(ownership)
    keys = [os.urandom(32).hex() for _ in range(5)]
    public = [shared.route.public_key(key) for key in keys]
    masks = [255, 128, 31, 127]
    peers = ';'.join(f'{i + 1},{public[i + 1]},{int(time.time()) + 3600},{mask}' for i, mask in enumerate(masks))
    env = os.environ.copy()
    env.update(LAYERFS_PRIVATE_KEY=keys[0], LAYERFS_PEERS=peers, LAYERFS_LISTEN='127.0.0.1:0',
        LAYERFS_STORE=str(directory / 'store.sqlite'), LAYERFS_TELEMETRY='off',
        LAYERFS_HISTORY_CATALOG=str(directory / 'history.sqlite'), LAYERFS_HISTORY_CREATE='1',
        LAYERFS_HISTORY_BINDING='pair1-stage', LAYERFS_HISTORY_INCARNATION='1',
        LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(), LAYERFS_CONSTRUCTION_WORKERS='1')
    service = subprocess.Popen([args.binaries / 'layerfs-service'], env=env, stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    with ownership.open('a') as log:
        log.write(json.dumps({'event': 'service-started', 'service_pid': service.pid}) + '\n')
    clients = []; ready = ''; errors = []
    report['owned_service_pid'] = service.pid
    try:
        ready = shared.mount.line_until(service); assert 'ready' in ready
        port = int(ready.strip().rsplit(':', 1)[1])
        report['bootstrap'] = shared.stage_route.bootstrap(directory, port, keys[1], public[0],
                                                           bytes.fromhex(fixture['file_root']), False)
        for i in range(4):
            clients.append(Principal(directory / f'principal-{i + 1}', port, keys[i + 1], public[0], i + 1))
        full, metadata_only, legacy31, legacy127 = clients
        branch = bytes.fromhex(report['bootstrap']['branch'])
        snapshot = full.branch(branch); root = snapshot['effective_root']
        old_attributes = {p.decode(): full.attributes(root, p) for p in (b'', b'data.bin', b'alias')}
        old_listing = full.listing(root, b''); old_history = full.commits(branch)
        before_store = shared.sha(directory / 'store.sqlite')
        if args.case == 'construct':
            with shared.check(report, CASES[args.case][0]) as row:
                row['results'] = []
                for kind, mode, seconds, nanos in [(1, 0o640, -(1 << 63), 0),
                    (2, 0o1777, -2, 750_000_000), (3, 0o777, (1 << 63) - 1, 999_999_999)]:
                    body = payload(kind, mode, seconds, nanos)
                    first = constructed(metadata_only.call(15, body))
                    assert (first['kind'], first['mode'], first['mtime'], first['nanoseconds']) == (kind, mode, seconds, nanos)
                    assert first['inserted'] > 0 and first['inserted'] + first['reused'] > 0
                    repeat = constructed(metadata_only.call(15, body))
                    assert repeat['metadata'] == first['metadata'] and repeat['inserted'] == 0 and repeat['reused'] > 0
                    noop = shared.metadata.metadata_result(metadata_only.call(9, first['metadata'] + body))
                    assert noop['base'] == noop['metadata'] == first['metadata'] and noop['inserted'] == 0
                    assert (noop['kind'], noop['mode'], noop['mtime'], noop['nanoseconds']) == (kind, mode, seconds, nanos)
                    row['results'].append(shared.printable({'constructed': first, 'repeat': repeat, 'public_metadata_noop': noop}))
                full_result = constructed(full.call(15, body))
                assert full_result['metadata'] == first['metadata'] and full_result['inserted'] == 0
                row['grant255_result'] = shared.printable(full_result)
                row.update(authority='primary calls grant128; separate grant255 repeat', request_body_bytes=0, response_data_bytes=0)
        else:
            with shared.check(report, CASES[args.case][0]) as row:
                row['attempts'] = []
                for mask, client in [(31, legacy31), (127, legacy127)]:
                    row['attempts'].append({'grant': mask, **client.call(15, payload(1, 0o640, -2, 17), failure=3)})
                for kind, mode, nanos in [(0, 0o644, 0), (4, 0o644, 0), (1, 0o4755, 0),
                    (2, 0o2777, 0), (3, 0o755, 0), (1, 0o644, 1_000_000_000)]:
                    row['attempts'].append({'kind': kind, 'mode': mode, 'nanoseconds': nanos,
                        **metadata_only.call(15, payload(kind, mode, 0, nanos), failure=1, allow_local=True)})
                assert shared.sha(directory / 'store.sqlite') == before_store
                row['content_store_unchanged'] = True
        with shared.check(report, 'unchanged-namespace-and-history') as row:
            assert full.branch(branch) == snapshot and full.commits(branch) == old_history
            assert full.listing(root, b'') == old_listing
            for name, attributes in old_attributes.items():
                assert full.attributes(root, name.encode()) == attributes
            # These are attempted framed client requests after bootstrap.
            # Local refusals are NOT_SUBMITTED; no internal Service calls are instrumented.
            assert all(operation['opcode'] in (2, 6, 9, 15) for client in clients for operation in client.attempts)
            row.update(branch_unchanged=True, history_unchanged=True, old_root_unchanged=True,
                       attempted_filesystem_saves=0, attempted_Commits=0, attempted_ReserveInodes=0,
                       internal_reservation_probe='not added; Rust public constructor test has no HistoryCatalog')
        report['attempted_framed_client_requests'] = {str(i + 1): client.attempts for i, client in enumerate(clients)}
    finally:
        for i, client in enumerate(clients):
            try:
                if client.cleanup():
                    report.setdefault('forced_clients', []).append(i + 1)
            except BaseException as error:
                errors.append({'owner': f'client-{i + 1}', 'error': repr(error)})
        try:
            if service.poll() is None:
                service.stdin.close()
                try: service.wait(timeout=6)
                except subprocess.TimeoutExpired:
                    report['service_forced_cleanup'] = True
                    service.kill(); service.wait(timeout=6)
        except BaseException as error:
            errors.append({'owner': 'service', 'pid': service.pid, 'error': repr(error)})
        report['cleanup_errors'] = errors; report['service_exit'] = service.poll()
        if service.poll() is not None:
            (args.output / 'service.stderr').write_bytes(ready.encode() + service.stderr.read())
    with shared.check(report, 'cleanup') as row:
        assert not errors and service.returncode == 0
        assert not report.get('forced_clients') and not report.get('service_forced_cleanup')
        assert shared.sha(master) == digest
        row.update(service_exit=0, closed_master_unchanged=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('fixture', 'binaries', 'output'): parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--case', choices=CASES, required=True)
    args = parser.parse_args()
    for name in ('fixture', 'binaries', 'output'): setattr(args, name, getattr(args, name).resolve())
    args.output.mkdir(parents=True, exist_ok=False); shared.route.BIN = args.binaries
    os.environ['LAYERFS_CONSTRUCTION_WORKERS'] = '1'
    report = {'status': 'FAIL', 'mode': 'functional-native-construct-portable-metadata', 'case': args.case,
        'checks': [{'id': name, 'status': 'NOT_RUN'} for name in CASES[args.case]],
        'hard_budget_seconds': 60, 'request_budget_ms': 5000, 'performance_claim': False, 'cache_claim': None,
        'construction_workers': 1, 'container_cpu_quota': 'not applicable: host Service/client; no Docker runtime',
        'not_run': ['file creation or fresh inode allocation', 'Workspace/kernel CREATE', 'prepared npm workload',
                    'R6/RSS/performance', 'late constructor-save SQL failure',
                    'native extra-input injection (direct public Service test covers rejection)']}
    started = time.monotonic()
    def expired(_signal, _frame): raise TimeoutError('complete constructor selection exceeded 60 seconds')
    signal.signal(signal.SIGALRM, expired); signal.alarm(60)
    try:
        space = shared.stage_route.isolation.namespace()
        for path in (args.fixture, args.binaries, args.output): space.assert_owned(path, 'constructor proof input/output')
        report.update(source=shared.mount.checked(['git', 'rev-parse', 'HEAD'], text=True).stdout.strip(),
            product_inputs_sha256=shared.mount.product_inputs(), driver_sha256=shared.sha(Path(__file__)),
            binaries={name: shared.sha(args.binaries / name) for name in ('layerfs-service', 'layerfs-daemon', 'examples/public_key')},
            resource_isolation=space.as_fields(),
            caller_dependencies_sha256={str(Path(module.__file__).resolve().relative_to(shared.ROOT)): shared.sha(Path(module.__file__))
                for module in tuple(sys.modules.values()) if getattr(module, '__file__', None)
                and Path(module.__file__).resolve().is_relative_to(shared.ROOT) and Path(module.__file__).suffix == '.py'})
        with space.lock_path.open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB); execute(args, report)
        report['status'] = 'PASS'
    except BaseException as error:
        report['failure'] = repr(error); raise
    finally:
        signal.alarm(0); report['command_wall_seconds'] = time.monotonic() - started
        (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__': main()
