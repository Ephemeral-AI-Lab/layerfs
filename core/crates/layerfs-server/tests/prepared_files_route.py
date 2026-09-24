#!/usr/bin/env python3
"""Fresh regular files on existing prepared Service routes; no Workspace CREATE."""
import argparse
import copy
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
import construct_metadata_route as metadata_constructor

DATA = b'Round42 fresh file payload\x00\xff'
WORKSPACE = b'\x79' * 32
CASES = {
    'files': ['fresh-content-and-portable-metadata', 'direct-candidate', 'explicit-stage-and-CommitStaged',
              'fresh-composite-Commit', 'immutable-roots-and-exact-history', 'cleanup'],
    'refusals': ['fresh-file-declaration-role-and-unbound-refusals', 'unchanged-namespace-stage-and-allocation', 'cleanup'],
}


class Peer(shared.Peer):
    def content(self, data):
        assert self.client is None and len(data) <= shared.route.FRAME_BYTES
        self.sequence += 1; self.client = shared.metadata.Client(*self.connection)
        process = self.client.process
        process.stdin.write(shared.route.begin(1, 3, struct.pack('>Q', len(data)), deadline_ms=5000))
        if data: process.stdin.write(shared.route.frame(3, 1, data))
        process.stdin.write(shared.route.frame(4, 1, struct.pack('>Q', len(data)))); process.stdin.flush()
        kind, body = shared.route.receive(process, timeout=6)
        assert kind == 6
        r = shared.route.Reader(body); assert r.u8() == 2
        value = {'root': r.take(32), 'length': r.u64(), 'inserted': r.u64(), 'reused': r.u64()}; r.done()
        assert value['length'] == len(data)
        (self.connection[0] / f'client-{self.sequence:03d}.stderr').write_text(self.client.close())
        self.client = None
        return value

    def read_file(self, root, expected):
        assert self.client is None
        self.sequence += 1; self.client = shared.metadata.Client(*self.connection)
        kind, body, data = self.client.call(1, root + struct.pack('>QQ', 0, len(expected)), response_bytes=len(expected))
        assert kind == 6 and body == b'\x01' + struct.pack('>Q', len(expected)) and data == expected
        (self.connection[0] / f'client-{self.sequence:03d}.stderr').write_text(self.client.close())
        self.client = None


def encode(value):
    if not value['files']:
        return shared.encode_preparation(value)
    body = shared.encode_preparation(value | {'new': [], 'patches': []})
    body += b'\x02' + struct.pack('>H', len(value['new']))
    body += b''.join(struct.pack('>QIqI', *row) for row in value['new'])
    body += struct.pack('>H', len(value['patches']))
    body += b''.join(struct.pack('>QIqI', *row) for row in value['patches'])
    body += struct.pack('>H', len(value['files']))
    return body + b''.join(struct.pack('>Q', serial) for serial in value['files'])


def captured(snapshot, value, command, generation=1):
    return (bytes((command,)) + WORKSPACE + snapshot['branch']['branch']
            + shared.route.optional(snapshot['branch']['head_commit']) + snapshot['branch']['base_layer']
            + struct.pack('>Q', generation) + encode(value))


def preparation(snapshot, first, empty, content, metadata, prefix=b''):
    parent = int.from_bytes(snapshot['root_serial'], 'big')
    value = shared.preparation(snapshot, [(parent, [(prefix + b'empty', first),
        (prefix + b'file', first + 1), (prefix + b'file-alias', first + 1)])], inodes=[
            {'serial': first, 'kind': 1, 'content': empty, 'metadata': metadata},
            {'serial': first + 1, 'kind': 1, 'content': content, 'metadata': metadata}])
    return value | {'files': [first, first + 1]}


def saved(peer, value):
    reader = shared.route.Reader(peer.call(5, encode(value))); assert reader.u8() == 7
    value = {'root': reader.take(32), 'inserted': reader.u64(), 'reused': reader.u64()}; reader.done()
    return value


def verify(peer, root, first, empty, content, metadata, prefix=b''):
    attributes = {}
    for name, serial, expected, references, file_root in [(b'empty', first, b'', 1, empty),
        (b'file', first + 1, DATA, 2, content), (b'file-alias', first + 1, DATA, 2, content)]:
        value = peer.attributes(root, prefix + name)
        assert value == {'serial': serial, 'kind': 1, 'references': references, 'content': file_root,
                         'metadata': metadata, 'mode': 0o640, 'mtime': -2, 'nanoseconds': 17, 'size': len(expected)}
        peer.read_file(file_root, expected)
        attributes[(prefix + name).decode()] = value
    return shared.printable(attributes)


def exercise(peer, snapshot, case, report):
    branch, old = snapshot['branch']['branch'], snapshot['effective_root']
    old_attrs = {name.decode(): peer.attributes(old, name) for name in (b'', b'data.bin', b'alias')}
    old_listing, old_history = peer.listing(old, b''), peer.commits(branch)
    count = 6 if case == 'files' else 2
    reservation = peer.reserve(snapshot['scope'], count); first = reservation['start']
    report['reservation'] = shared.printable(reservation)
    empty, content = peer.content(b''), peer.content(DATA)
    meta = metadata_constructor.constructed(peer.call(15, metadata_constructor.payload(1, 0o640, -2, 17)))
    empty, content, metadata = empty['root'], content['root'], meta['metadata']
    assert peer.branch(branch) == snapshot and peer.commits(branch) == old_history
    if case == 'files':
        with shared.check(report, 'fresh-content-and-portable-metadata') as row:
            peer.read_file(empty, b''); peer.read_file(content, DATA)
            row.update(empty=empty.hex(), content=content.hex(), logical_bytes=len(DATA), metadata=shared.printable(meta))
        with shared.check(report, 'direct-candidate') as row:
            direct = saved(peer, preparation(snapshot, first, empty, content, metadata))
            row.update(candidate=shared.printable(direct), attributes=verify(peer, direct['root'], first, empty, content, metadata))
            assert peer.branch(branch) == snapshot and peer.commits(branch) == old_history
            peer.call(6, b'\x09' + WORKSPACE, 2, failure=14)
        with shared.check(report, 'explicit-stage-and-CommitStaged') as row:
            tag, stage = peer.command(captured(snapshot, preparation(snapshot, first + 2, empty, content, metadata), 3))
            assert tag == 'Stage' and stage['candidate_root'] != direct['root']
            row['attributes'] = verify(peer, stage['candidate_root'], first + 2, empty, content, metadata)
            assert peer.branch(branch) == snapshot and peer.commits(branch) == old_history
            tag, one = peer.command(b'\x04' + WORKSPACE + struct.pack('>Q', stage['token']))
            assert tag == 'Committed' and one['root'] == stage['candidate_root']
            assert one['parent'] == snapshot['branch']['head_commit']
            peer.call(6, b'\x09' + WORKSPACE, 2, failure=14)
            assert peer.commits(branch) == [one] + old_history
            row.update(stage=shared.printable(stage), commit=shared.printable(one))
        with shared.check(report, 'fresh-composite-Commit') as row:
            current = peer.branch(branch); assert current['effective_root'] == one['root']
            tag, two = peer.command(captured(current, preparation(current, first + 4, empty, content, metadata, b'next-'), 5, generation=2))
            assert tag == 'Committed' and two['parent'] == one['commit']
            assert peer.branch(branch)['effective_root'] == two['root']
            row.update(commit=shared.printable(two), attributes=verify(peer, two['root'], first + 4, empty, content, metadata, b'next-'))
        with shared.check(report, 'immutable-roots-and-exact-history') as row:
            verify(peer, direct['root'], first, empty, content, metadata)
            verify(peer, one['root'], first + 2, empty, content, metadata)
            verify(peer, two['root'], first + 2, empty, content, metadata)
            for root in (old, direct['root'], one['root'], two['root']):
                for name in (b'data.bin', b'alias'): assert peer.attributes(root, name) == old_attrs[name.decode()]
            assert peer.listing(old, b'') == old_listing and peer.commits(branch) == [two, one] + old_history
            peer.call(2, old + b'\x04' + shared.route.blob(b'empty'), failure=7)
            row.update(old_root_unchanged=True, independent_candidate_readable=True,
                       explicit_CommitStaged_calls=1, explicit_CompositeCommit_calls=1,
                       workload_history_records_added=2, declaration_reexposure=False)
    else:
        baseline = preparation(snapshot, first, empty, content, metadata)
        no_change = shared.preparation(snapshot) | {'files': []}
        tag, previous_stage = peer.command(captured(snapshot, no_change, 3)); assert tag == 'Stage'
        cases = []
        for name, serial in [('zero', 0), ('above-limit', 1 << 63), ('root', baseline['root_serial'])]:
            value = copy.deepcopy(baseline); value['files'][0] = serial; cases.append((name, value, 1, True))
        value = copy.deepcopy(baseline); value['files'][1] = first + 9; cases.append(('missing-subset', value, 1, True))
        value = copy.deepcopy(baseline); value['files'].reverse(); cases.append(('order', value, 1, True))
        value = copy.deepcopy(baseline); value['files'][1] = first; cases.append(('duplicate', value, 1, True))
        value = copy.deepcopy(baseline); value['inodes'][0]['kind'] = 2; cases.append(('non-regular', value, 1, True))
        value = copy.deepcopy(baseline); value['files'].append(first + 2); cases.append(('F-exceeds-I', value, 4, True))
        value = copy.deepcopy(baseline); existing = old_attrs['data.bin']['serial']
        value['files'][0] = existing; value['inodes'][0]['serial'] = existing
        value['directories'][0][1][0] = (b'empty', existing); cases.append(('existing-base-serial', value, 1, False))
        for name, key, root, code in [('wrong-content-role', 'content', old_attrs['']['content'], 1),
            ('wrong-metadata-role', 'metadata', content, 1), ('missing-content', 'content', b'\xee' * 32, 6),
            ('missing-metadata', 'metadata', b'\xee' * 32, 6)]:
            value = copy.deepcopy(baseline); value['inodes'][0][key] = root; cases.append((name, value, code, False))
        value = copy.deepcopy(baseline); value['directories'].clear(); cases.append(('unbound-regular', value, 1, False))
        value = copy.deepcopy(baseline); value['files'].clear(); cases.append(('undeclared-fresh', value, 1, False))
        value = copy.deepcopy(baseline); value['scope'] = b'\xee' * 32; cases.append(('scope', value, 1, False))
        value = copy.deepcopy(baseline); value['patches'] = [shared.declaration(first)]; cases.append(('patch-overlap', value, 1, True))
        value = copy.deepcopy(baseline); value['new'] = [shared.declaration(first)]
        value['directories'].append((first, [])); cases.append(('directory-overlap', value, 1, True))
        with shared.check(report, 'fresh-file-declaration-role-and-unbound-refusals') as row:
            row['attempts'] = []
            for name, value, code, local in cases:
                for label, opcode, profile, body in [('direct', 5, 1, encode(value)),
                    ('stage', 7, 2, captured(snapshot, value, 3)), ('commit', 7, 2, captured(snapshot, value, 5))]:
                    failure = peer.call(opcode, body, profile, failure=code, allow_local=local)
                    assert peer.branch(branch) == snapshot and peer.commits(branch) == old_history
                    assert peer.query(b'\x09' + WORKSPACE) == ('Stage', previous_stage)
                    row['attempts'].append({'case': name, 'route': label, **failure})
        with shared.check(report, 'unchanged-namespace-stage-and-allocation') as row:
            assert peer.listing(old, b'') == old_listing
            for name, attributes in old_attrs.items(): assert peer.attributes(old, name.encode()) == attributes
            tag, discarded = peer.command(b'\x07' + WORKSPACE + struct.pack('>Q', previous_stage['token']))
            assert tag == 'Discarded' and discarded == 1
            row.update(old_root_unchanged=True, prior_stage_preserved_then_explicitly_discarded=True,
                       Commit_records_added=0, unbound_regular_file='InvalidInput; no directory-style omission')
    after = peer.reserve(snapshot['scope'], 1)
    assert after['start'] == first + count
    report['subsequent_reservation'] = shared.printable(after)
    report['allocation_scope'] = 'explicit live C5 reservation; prepared operations consumed no additional range'


def execute(args, report):
    fixture = json.loads(args.fixture.read_text()); assert fixture['status'] == 'PASS'
    master = args.fixture.parent / 'service/store.sqlite'; digest = shared.sha(master)
    assert digest == fixture['closed_master_sha256']['store.sqlite']
    for suffix in ('-wal', '-shm', '-journal'): assert not Path(str(master) + suffix).exists()
    directory = args.output / 'service'; directory.mkdir(); shutil.copyfile(master, directory / 'store.sqlite')
    assert shared.sha(directory / 'store.sqlite') == digest
    report['fixture_reuse'] = {'receipt': str(args.fixture), 'receipt_sha256': shared.sha(args.fixture),
        'closed_store_sha256': digest, 'clone_method': 'independent-byte-copy',
        'history': 'fresh live CREATE=1; no copied catalog', 'cache_claim': None}
    owners = args.output / 'owners.jsonl'
    owners.write_text(json.dumps({'event': 'planned', 'case': args.case, 'output': str(args.output),
        'worktree': str(shared.ROOT), 'driver_pid': os.getpid(), 'service_pid': None,
        'service_executable': str(args.binaries / 'layerfs-server'), 'service_directory': str(directory)}) + '\n')
    report['ownership_journal'] = str(owners)
    private, server_private = os.urandom(32).hex(), os.urandom(32).hex()
    public, server_public = shared.route.public_key(private), shared.route.public_key(server_private)
    env = os.environ.copy(); env.update(LAYERFS_PRIVATE_KEY=server_private,
        LAYERFS_PEERS=f'1,{public},{int(time.time()) + 3600},255', LAYERFS_LISTEN='127.0.0.1:0',
        LAYERFS_STORE=str(directory / 'store.sqlite'), LAYERFS_TELEMETRY='off',
        LAYERFS_HISTORY_CATALOG=str(directory / 'history.sqlite'), LAYERFS_HISTORY_CREATE='1',
        LAYERFS_HISTORY_BINDING='pair1-stage', LAYERFS_HISTORY_INCARNATION='1',
        LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(), LAYERFS_CONSTRUCTION_WORKERS='1')
    service = subprocess.Popen([args.binaries / 'layerfs-server'], env=env, stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    with owners.open('a') as log: log.write(json.dumps({'event': 'service-started', 'service_pid': service.pid}) + '\n')
    peer = None; ready = ''; errors = []
    try:
        ready = shared.mount.line_until(service); assert 'ready' in ready; port = int(ready.strip().rsplit(':', 1)[1])
        report['bootstrap'] = shared.stage_route.bootstrap(directory, port, private, server_public, bytes.fromhex(fixture['file_root']), False)
        peer = Peer(directory, port, private, server_public)
        exercise(peer, peer.branch(bytes.fromhex(report['bootstrap']['branch'])), args.case, report)
    finally:
        try:
            if peer is not None and peer.cleanup(): report['forced_client'] = True
        except BaseException as error: errors.append({'owner': 'client', 'error': repr(error)})
        try:
            if service.poll() is None:
                service.stdin.close()
                try: service.wait(timeout=6)
                except subprocess.TimeoutExpired:
                    report['service_forced_cleanup'] = True; service.kill(); service.wait(timeout=6)
        except BaseException as error: errors.append({'owner': 'service', 'pid': service.pid, 'error': repr(error)})
        report['cleanup_errors'] = errors; report['service_exit'] = service.poll()
        if service.poll() is not None: (args.output / 'service.stderr').write_bytes(ready.encode() + service.stderr.read())
    with shared.check(report, 'cleanup') as row:
        assert not errors and service.returncode == 0 and not report.get('forced_client') and not report.get('service_forced_cleanup')
        assert shared.sha(master) == digest
        row.update(service_exit=0, closed_master_unchanged=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('fixture', 'binaries', 'output'): parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--case', choices=CASES, required=True); args = parser.parse_args()
    for name in ('fixture', 'binaries', 'output'): setattr(args, name, getattr(args, name).resolve())
    args.output.mkdir(parents=True, exist_ok=False); shared.route.BIN = args.binaries
    os.environ['LAYERFS_CONSTRUCTION_WORKERS'] = '1'
    report = {'status': 'FAIL', 'mode': 'functional-native-prepared-fresh-files', 'case': args.case,
        'checks': [{'id': name, 'status': 'NOT_RUN'} for name in CASES[args.case]],
        'hard_budget_seconds': 60, 'request_budget_ms': 5000, 'construction_workers': 1,
        'performance_claim': False, 'cache_claim': None, 'container_cpu_quota': 'not applicable: host Service/client',
        'not_run': ['Workspace/kernel CREATE', 'prepared npm workload', 'R6/RSS/performance',
                    'reservation ownership provenance enforcement', 'unknown prepared save recovery']}
    started = time.monotonic()
    def expired(_signal, _frame): raise TimeoutError('complete fresh-file selection exceeded 60 seconds')
    signal.signal(signal.SIGALRM, expired); signal.alarm(60)
    try:
        space = shared.stage_route.isolation.namespace()
        for path in (args.fixture, args.binaries, args.output): space.assert_owned(path, 'fresh-file proof input/output')
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
    except BaseException as error: report['failure'] = repr(error); raise
    finally:
        signal.alarm(0); report['command_wall_seconds'] = time.monotonic() - started
        (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__': main()
