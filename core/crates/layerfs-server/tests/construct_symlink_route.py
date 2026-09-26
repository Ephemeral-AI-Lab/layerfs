#!/usr/bin/env python3
"""Symlink object construction with existing-inode readback; no fresh symlink admission."""
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
    'construct': ['opaque-target-construction-and-root-reuse', 'existing-symlink-readback', 'unchanged-branch-and-old-root', 'cleanup'],
    'refusals': ['target-and-independent-grant-refusals', 'unchanged-branch-and-old-root', 'cleanup'],
}
TARGETS = [('relative', b'../round45-relative'), ('absolute', b'/round45/absolute'),
           ('opaque', b'\xff/\x80'), ('empty', b''), ('maximum', b'x' * 4096)]


def saved(body, length):
    assert len(body) == 57
    r = shared.route.Reader(body); assert r.u8() == 2
    value = {'root': r.take(32), 'length': r.u64(), 'inserted': r.u64(), 'reused': r.u64()}
    r.done(); assert value['length'] == length
    return value


def readlink(peer, root):
    r = shared.route.Reader(peer.call(2, root + b'\x03' + shared.route.blob(b'link')))
    assert r.u8() == 6
    value = r.blob(); r.done(); return value


class Principal(shared.Peer):
    def __init__(self, directory, port, private, public, selector):
        directory.mkdir()
        super().__init__(directory, port, private, public)
        self.connection += (selector,)
        self.attempts = []

    def call(self, opcode, payload, profile=1, failure=None, allow_local=False):
        self.attempts.append({'opcode': opcode, 'profile': profile, 'input_body_bytes': 0})
        return super().call(opcode, payload, profile, failure, allow_local)


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
        'service_executable': str(args.binaries / 'layerfs-server'), 'service_directory': str(directory)}) + '\n')
    report['ownership_journal'] = str(ownership)
    keys = [os.urandom(32).hex() for _ in range(6)]
    public = [shared.route.public_key(key) for key in keys]
    masks = [255, 4, 31, 128, 64]
    peers = ';'.join(f'{i + 1},{public[i + 1]},{int(time.time()) + 3600},{mask}' for i, mask in enumerate(masks))
    env = os.environ.copy()
    env.update(LAYERFS_PRIVATE_KEY=keys[0], LAYERFS_PEERS=peers, LAYERFS_LISTEN='127.0.0.1:0',
        LAYERFS_STORE=str(directory / 'store.sqlite'), LAYERFS_TELEMETRY='off',
        LAYERFS_HISTORY_CATALOG=str(directory / 'history.sqlite'), LAYERFS_HISTORY_CREATE='1',
        LAYERFS_HISTORY_BINDING='pair1-stage', LAYERFS_HISTORY_INCARNATION='1',
        LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(), LAYERFS_CONSTRUCTION_WORKERS='1')
    service = subprocess.Popen([args.binaries / 'layerfs-server'], env=env, stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    with ownership.open('a') as log:
        log.write(json.dumps({'event': 'service-started', 'service_pid': service.pid}) + '\n')
    clients = []; ready = ''; errors = []
    report['owned_service_pid'] = service.pid
    try:
        ready = shared.mount.line_until(service); assert 'ready' in ready
        port = int(ready.strip().rsplit(':', 1)[1])
        report['bootstrap'] = shared.stage_route.bootstrap(directory, port, keys[1], public[0],
                                                           bytes.fromhex(fixture['file_root']), False, manifest_extras=(
            shared.route.manifest_entry(0, b'link', 3, 0o777, 1700000030, 1, target=b'round45-existing'),))
        for i in range(5):
            clients.append(Principal(directory / f'principal-{i + 1}', port, keys[i + 1], public[0], i + 1))
        full, construct_only, legacy31, metadata_only, history_only = clients
        branch = bytes.fromhex(report['bootstrap']['branch'])
        snapshot = full.branch(branch); root = snapshot['effective_root']
        old_attributes = {p.decode(): full.attributes(root, p) for p in (b'', b'data.bin', b'alias', b'link')}
        old_listing = full.listing(root, b''); old_history = full.commits(branch)
        before_store = shared.sha(directory / 'store.sqlite')
        if args.case == 'construct':
            constructed = []
            with shared.check(report, 'opaque-target-construction-and-root-reuse') as row:
                row['results'] = []
                for label, target in TARGETS:
                    first = saved(construct_only.call(16, shared.route.blob(target)), len(target))
                    assert first['inserted'] + first['reused'] == 1
                    repeat = saved(construct_only.call(16, shared.route.blob(target)), len(target))
                    assert repeat['root'] == first['root'] and (repeat['inserted'], repeat['reused']) == (0, 1)
                    constructed.append((label, target, first))
                    row['results'].append(shared.printable({'label': label, 'target': target, 'first': first, 'repeat': repeat}))
                legacy = saved(legacy31.call(16, shared.route.blob(TARGETS[-1][1])), 4096)
                assert legacy['root'] == first['root'] and (legacy['inserted'], legacy['reused']) == (0, 1)
                row.update(grant4_sufficient=True, legacy31_permits=True, request_body_bytes=0, response_data_bytes=0)
            with shared.check(report, 'existing-symlink-readback') as row:
                row['candidates'] = []
                old_link = old_attributes['link']; assert old_link['kind'] == 3
                for label, target, result in constructed:
                    prepared = shared.preparation(snapshot, inodes=[{'serial': old_link['serial'], 'kind': 3,
                        'content': result['root'], 'metadata': old_link['metadata']}])
                    candidate = shared.saved(full, prepared)
                    assert full.attributes(candidate['root'], b'link') == old_link | {'content': result['root'], 'size': len(target)}
                    assert readlink(full, candidate['root']) == target
                    row['candidates'].append(shared.printable({'label': label, 'filesystem_save': candidate,
                        'same_existing_serial': old_link['serial'], 'readlink': target}))
                row.update(new_symlink_declarations=0, explicit_candidate_saves=len(constructed), workload_Commits=0)
        else:
            with shared.check(report, CASES[args.case][0]) as row:
                row['attempts'] = []
                for mask, client in [(128, metadata_only), (64, history_only)]:
                    row['attempts'].append({'grant': mask, **client.call(16, shared.route.blob(b'target'), failure=3)})
                row['attempts'].append({'target': 'NUL', **construct_only.call(16, shared.route.blob(b'a\0b'), failure=1, allow_local=True)})
                row['attempts'].append({'target_bytes': 4097, **construct_only.call(16, shared.route.blob(b'x' * 4097), failure=4, allow_local=True)})
                row['attempts'].append({'grant4_cannot_construct_metadata': True,
                    **construct_only.call(15, struct.pack('>BIqI', 3, 0o777, 0, 0), failure=3)})
                row['attempts'].append({'grant4_cannot_query_history': True,
                    **construct_only.call(6, b'\x03' + branch, 2, failure=3)})
                assert shared.sha(directory / 'store.sqlite') == before_store
                row['content_store_unchanged'] = True
        with shared.check(report, 'unchanged-branch-and-old-root') as row:
            assert full.branch(branch) == snapshot and full.commits(branch) == old_history
            assert full.listing(root, b'') == old_listing and readlink(full, root) == b'round45-existing'
            for name, attributes in old_attributes.items():
                assert full.attributes(root, name.encode()) == attributes
            # These are attempted framed requests; local failures are explicitly NOT_SUBMITTED.
            attempts = [operation for client in clients for operation in client.attempts]
            assert all(a['opcode'] in (2, 5, 6, 15, 16) for a in attempts)
            row.update(branch_unchanged=True, history_unchanged=True, old_root_unchanged=True,
                       attempted_filesystem_saves=sum(a['opcode'] == 5 for a in attempts),
                       attempted_Commits=0, attempted_ReserveInodes=0,
                       internal_reservation_probe='not added; direct public constructor test has no HistoryCatalog')
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
    report = {'status': 'FAIL', 'mode': 'functional-native-construct-symlink', 'case': args.case,
        'checks': [{'id': name, 'status': 'NOT_RUN'} for name in CASES[args.case]],
        'hard_budget_seconds': 60, 'request_budget_ms': 5000, 'performance_claim': False, 'cache_claim': None,
        'construction_workers': 1, 'container_cpu_quota': 'not applicable: host Service/client; no Docker runtime',
        'not_run': ['fresh symlink/inode allocation', 'Workspace/kernel SYMLINK', 'prepared npm workload',
                    'R6/RSS/performance', 'late constructor-save SQL failure',
                    'native extra-input/deadline/save-ownership injection (public Rust tests cover)',
                    'native daemon terminal corruption/loss (authenticated Bridge loopback tests cover)']}
    started = time.monotonic()
    def expired(_signal, _frame): raise TimeoutError('complete constructor selection exceeded 60 seconds')
    signal.signal(signal.SIGALRM, expired); signal.alarm(60)
    try:
        space = shared.stage_route.isolation.namespace()
        for path in (args.fixture, args.binaries, args.output): space.assert_owned(path, 'constructor proof input/output')
        report.update(source=shared.mount.checked(['git', 'rev-parse', 'HEAD'], text=True).stdout.strip(),
            product_inputs_sha256=shared.mount.product_inputs(), driver_sha256=shared.sha(Path(__file__)),
            binaries={name: shared.sha(args.binaries / name) for name in ('layerfs-server', 'layerfs-daemon', 'examples/public_key')},
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
