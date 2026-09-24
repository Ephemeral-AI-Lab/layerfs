#!/usr/bin/env python3
"""Native host Service proof for shared prepared new directories; no FUSE claim.

Each selection byte-copies the closed content master and creates fresh live C5
authority. No copied catalog is reopened for writing and no allocation is replayed.
"""
import argparse
from contextlib import contextmanager
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
sys.path.insert(0, str(ROOT / 'core/crates/layerfs-workspace/tests'))
import history_route as route
import metadata_route as metadata
import mounted_read as mount
import stage_route

CASES = {
    'nested': ['reserved-direct-candidate', 'fresh-reserved-Commit', 'immutable-roots-and-history', 'cleanup'],
    'refusals': ['shared-admission-refusals', 'retained-history-and-reservations', 'cleanup'],
    'unreferenced': ['unreferenced-directories-omitted', 'combined-inode-cap', 'cleanup'],
    'metadata_followup': ['G1-explicit-create-Commit', 'G2-explicit-metadata-only-Commit', 'cleanup'],
}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def printable(value):
    if isinstance(value, bytes):
        return value.hex()
    if isinstance(value, dict):
        return {key: printable(item) for key, item in value.items()}
    if isinstance(value, (tuple, list)):
        return [printable(item) for item in value]
    return value


@contextmanager
def check(report, name):
    row = next(row for row in report['checks'] if row['id'] == name)
    row['status'] = 'INCOMPLETE'
    try:
        yield row
    except BaseException as error:
        row.update(status='FAIL', failure=repr(error))
        raise
    else:
        row['status'] = 'PASS'


class Peer:
    """One fresh authenticated production client per operation, with no replay."""
    def __init__(self, directory, port, private, public):
        self.connection = directory, port, private, public
        self.client = None
        self.sequence = 0

    def call(self, opcode, payload, profile=1, failure=None, allow_local=False):
        assert self.client is None
        self.sequence += 1
        self.client = metadata.Client(*self.connection)
        try:
            kind, body, output = self.client.call(opcode, payload, profile)
        except (EOFError, BrokenPipeError):
            assert failure is not None and allow_local, 'a remote refusal lost its terminal'
            stderr = self.client.close(1)
            expected = {1: 'InvalidInput', 4: 'Capacity', 14: 'NotFound'}[failure]
            assert expected in stderr and 'unknown=true' not in stderr, stderr
            result = {'route': 'local shared request validation', 'code': failure,
                      'remote_result': 'NOT_SUBMITTED', 'stderr': stderr}
        else:
            assert not output
            if failure is None:
                assert kind == 6, (kind, body)
                stderr = self.client.close()
                result = body
            else:
                assert kind == 7 and body[:3] == bytes((failure, 0, 0)), (kind, body)
                stderr = self.client.close(1)
                result = {'route': 'authenticated Service refusal', 'code': body[0],
                          'terminal_hex': body.hex(), 'unknown': False, 'cleanup': None,
                          'stderr': stderr}
        (self.connection[0] / f'client-{self.sequence:03d}.stderr').write_text(stderr)
        self.client = None
        return result

    def query(self, payload):
        return route.history(self.call(6, payload, 2))

    def command(self, payload):
        return route.history(self.call(7, payload, 2))

    def branch(self, identity):
        tag, value = self.query(b'\x03' + identity)
        assert tag == 'BranchSnapshot'
        return value

    def attributes(self, root, path):
        reader = route.Reader(self.call(2, root + b'\x04' + route.blob(path)))
        assert reader.u8() == 9
        value = {'serial': reader.u64(), 'kind': reader.u8(), 'references': reader.u64(),
                 'content': reader.take(32), 'metadata': reader.take(32),
                 'mode': int.from_bytes(reader.take(4), 'big'),
                 'mtime': int.from_bytes(reader.take(8), 'big', signed=True),
                 'nanoseconds': int.from_bytes(reader.take(4), 'big'), 'size': reader.u64()}
        reader.done()
        return value

    def listing(self, root, path):
        reader = route.Reader(self.call(2, root + b'\x02' + route.blob(path)
            + route.blob(b'') + struct.pack('>HI', 128, 16384)))
        assert reader.u8() == 5
        continuation = reader.blob()
        entries = [(reader.blob(), reader.u64()) for _ in range(reader.u16())]
        reader.done()
        assert not continuation
        return entries

    def commits(self, branch):
        tag, value = self.query(b'\x06' + branch + route.optional(None) + route.blob(b'') + struct.pack('>H', 8))
        assert tag == 'Commits' and not value['continuation']
        return value['records']

    def reserve(self, scope, count):
        tag, value = self.command(b'\x08' + scope + struct.pack('>Q', count))
        assert tag == 'Reservation' and value['scope'] == scope and value['count'] == count
        return value

    def cleanup(self):
        if self.client is not None:
            self.client.terminate()
            self.client = None
            return True
        return False


def preparation(snapshot, directories=(), new=(), inodes=(), patches=()):
    return {'base': snapshot['effective_root'], 'scope': snapshot['scope'],
            'root_serial': int.from_bytes(snapshot['root_serial'], 'big'),
            'directories': list(directories), 'inodes': list(inodes), 'new': list(new), 'patches': list(patches)}


def declaration(serial, mode=0o1777, seconds=-2, nanos=987654321):
    return serial, mode, seconds, nanos


def nested(snapshot, serial):
    return preparation(snapshot, [(int.from_bytes(snapshot['root_serial'], 'big'), [(b'new', serial)]),
        (serial, [(b'nested', serial + 1)]), (serial + 1, [])],
        [declaration(serial), declaration(serial + 1, 0o700, 3, 4)],
        patches=[declaration(int.from_bytes(snapshot['root_serial'], 'big'), 0o755, 29, 31)])


def encode_preparation(value):
    body = value['base'] + value['scope'] + struct.pack('>QH', value['root_serial'], len(value['directories']))
    for parent, names in value['directories']:
        body += struct.pack('>QH', parent, len(names))
        for name, serial in names:
            body += route.blob(name) + struct.pack('>Q', serial or 0)
    body += struct.pack('>H', len(value['inodes']))
    for inode in value['inodes']:
        body += struct.pack('>QB', inode['serial'], inode['kind']) + inode['content'] + inode['metadata']
    if value['new'] or value['patches']:
        body += b'\x01' + struct.pack('>H', len(value['new']))
        body += b''.join(struct.pack('>QIqI', *entry) for entry in value['new'])
        body += struct.pack('>H', len(value['patches']))
        body += b''.join(struct.pack('>QIqI', *entry) for entry in value['patches'])
    return body


def captured(snapshot, value, command=5, generation=1):
    return (bytes((command,)) + b'\x75' * 32 + snapshot['branch']['branch']
            + route.optional(snapshot['branch']['head_commit']) + snapshot['branch']['base_layer']
            + struct.pack('>Q', generation) + encode_preparation(value))


def saved(peer, value):
    reader = route.Reader(peer.call(5, encode_preparation(value)))
    assert reader.u8() == 7
    result = {'root': reader.take(32), 'inserted': reader.u64(), 'reused': reader.u64()}
    reader.done()
    return result


def verify_nested(peer, root, serial):
    values = {}
    for path, expected in [(b'new', declaration(serial)), (b'new/nested', declaration(serial + 1, 0o700, 3, 4))]:
        value = peer.attributes(root, path)
        assert (value['serial'], value['kind'], value['references'], value['size']) == (expected[0], 2, 1, 0)
        assert (value['mode'], value['mtime'], value['nanoseconds']) == expected[1:]
        values[path.decode()] = value
    assert peer.listing(root, b'new') == [(b'nested', serial + 1)]
    assert peer.listing(root, b'new/nested') == []
    return values


def exercise(peer, snapshot, case, report):
    branch, old = snapshot['branch']['branch'], snapshot['effective_root']
    original = {name.decode(): peer.attributes(old, name) for name in (b'', b'data.bin', b'alias', b'other.bin')}
    assert original['data.bin'] == original['alias']
    original_listing, history = peer.listing(old, b''), peer.commits(branch)
    report['initial'] = printable({'snapshot': snapshot, 'attributes': original,
                                   'listing': original_listing, 'history': history})
    count = {'nested': 4, 'metadata_followup': 2}.get(case, 128)
    reservation = peer.reserve(snapshot['scope'], count)
    report['reservation'] = printable(reservation)
    first = reservation['start']
    if case == 'nested':
        with check(report, 'reserved-direct-candidate') as row:
            candidate = saved(peer, nested(snapshot, first))
            candidate_attrs = verify_nested(peer, candidate['root'], first)
            assert peer.branch(branch) == snapshot and peer.commits(branch) == history
            assert peer.listing(old, b'') == original_listing
            parent = peer.attributes(candidate['root'], b'')
            assert (parent['mode'], parent['mtime'], parent['nanoseconds']) == (0o755, 29, 31)
            row.update(candidate=printable(candidate), attributes=printable(candidate_attrs), parent=printable(parent),
                       branch_unchanged=True, allocation='first two reserved serials')
        with check(report, 'fresh-reserved-Commit') as row:
            tag, commit = peer.command(captured(snapshot, nested(snapshot, first + 2)))
            assert tag == 'Committed' and commit['parent'] == snapshot['branch']['head_commit']
            assert commit['base_layer'] == snapshot['branch']['base_layer']
            assert commit['root'] not in (old, candidate['root'])
            attrs = verify_nested(peer, commit['root'], first + 2)
            for path, value in attrs.items():
                # Directory content embeds child serials, so its root is also identity-dependent.
                for field in ('kind', 'references', 'mode', 'mtime', 'nanoseconds', 'size', 'metadata'):
                    assert value[field] == candidate_attrs[path][field]
            row.update(commit=printable(commit), attributes=printable(attrs),
                       allocation='second two reserved serials; no re-exposure',
                       root_equality_claim=False)
        with check(report, 'immutable-roots-and-history') as row:
            current = peer.branch(branch)
            assert current['effective_root'] == commit['root']
            assert current['branch']['head_commit'] == commit['commit']
            assert current['branch']['base_layer'] == snapshot['branch']['base_layer']
            assert peer.commits(branch) == [commit] + history
            peer.call(6, b'\x09' + b'\x75' * 32, 2, failure=14)
            assert verify_nested(peer, candidate['root'], first) == candidate_attrs
            for root in (old, candidate['root'], commit['root']):
                for path in (b'data.bin', b'alias', b'other.bin'):
                    assert peer.attributes(root, path) == original[path.decode()]
            assert peer.listing(old, b'') == original_listing
            assert peer.attributes(old, b'') == original['']
            parent = peer.attributes(commit['root'], b'')
            assert (parent['mode'], parent['mtime'], parent['nanoseconds']) == (0o755, 29, 31)
            row.update(current=printable(current), history_records=len(history) + 1,
                       old_root_readable=True, independent_candidate_readable=True, stage_consumed=True)
    elif case == 'metadata_followup':
        with check(report, 'G1-explicit-create-Commit') as row:
            tag, first_commit = peer.command(captured(snapshot, nested(snapshot, first)))
            assert tag == 'Committed'
            initial_dirs = verify_nested(peer, first_commit['root'], first)
            current = peer.branch(branch)
            assert current['effective_root'] == first_commit['root']
            assert peer.commits(branch) == [first_commit] + history
            row.update(commit=printable(first_commit), directories=printable(initial_dirs))
        with check(report, 'G2-explicit-metadata-only-Commit') as row:
            value = preparation(current, patches=[declaration(first, 0o711, -9, 123)])
            tag, second_commit = peer.command(captured(current, value, generation=2))
            assert tag == 'Committed' and second_commit['parent'] == first_commit['commit']
            updated = peer.attributes(second_commit['root'], b'new')
            assert updated == initial_dirs['new'] | {'mode': 0o711, 'mtime': -9, 'nanoseconds': 123,
                                                    'metadata': updated['metadata']}
            assert updated['metadata'] != initial_dirs['new']['metadata']
            assert peer.attributes(first_commit['root'], b'new') == initial_dirs['new']
            assert peer.attributes(second_commit['root'], b'new/nested') == initial_dirs['new/nested']
            assert peer.listing(second_commit['root'], b'new') == [(b'nested', first + 1)]
            assert peer.commits(branch) == [second_commit, first_commit] + history
            assert peer.branch(branch)['effective_root'] == second_commit['root']
            assert peer.listing(old, b'') == original_listing
            peer.call(6, b'\x09' + b'\x75' * 32, 2, failure=14)
            row.update(commit=printable(second_commit), changed_directory=printable(updated),
                       new_directory_declarations=0, directory_binding_changes=0,
                       explicit_workload_commits=2, generic_attribute_transport_claim=False)
    elif case == 'unreferenced':
        value = preparation(snapshot, [(serial, []) for serial in range(first, first + 126)],
                            [declaration(serial) for serial in range(first, first + 126)],
                            [original['data.bin']], patches=[declaration(original['']['serial'], original['']['mode'],
                                original['']['mtime'], original['']['nanoseconds'])])
        with check(report, 'unreferenced-directories-omitted') as row:
            result = saved(peer, value)
            assert result['root'] == old and peer.branch(branch) == snapshot
            undeclared = preparation(snapshot, [(value['root_serial'], [(b'missing', first)])])
            refusal = peer.call(5, encode_preparation(undeclared), failure=1)
            row.update(saved=printable(result), new_directories=126, existing_inode_rows=1, directory_metadata_rows=1,
                       absent_serial_refusal=refusal, semantics='unreferenced new directories omitted')
        with check(report, 'combined-inode-cap') as row:
            value['directories'].append((first + 126, [])); value['new'].append(declaration(first + 126))
            row.update(refusal=peer.call(5, encode_preparation(value), failure=4, allow_local=True),
                       combined_inode_rows=129, directory_rows=127)
    else:
        import copy
        baseline = nested(snapshot, first)
        cases = []
        for name, serial in [('zero', 0), ('above-limit', 1 << 63), ('root', baseline['root_serial']),
                             ('existing-file', original['data.bin']['serial'])]:
            cases.append((name, preparation(snapshot, [(serial, [])], [declaration(serial)])))
        value = copy.deepcopy(baseline); value['new'][0] = declaration(first, 0o4755); cases.append(('mode', value))
        value = copy.deepcopy(baseline); value['new'][0] = declaration(first, nanos=1_000_000_000); cases.append(('nanos', value))
        value = copy.deepcopy(baseline); value['new'].reverse(); cases.append(('order', value))
        value = copy.deepcopy(baseline); value['new'][1] = declaration(first); cases.append(('duplicate', value))
        value = copy.deepcopy(baseline); value['directories'].pop(); cases.append(('missing-own-directory-row', value))
        value = copy.deepcopy(baseline); value['new'].pop(); cases.append(('undeclared-parent', value))
        value = copy.deepcopy(baseline); value['directories'][1][1][0] = (b'nested', first + 2); cases.append(('undeclared-child', value))
        value = copy.deepcopy(baseline); value['inodes'] = [original['data.bin'] | {'serial': first}]; cases.append(('overlap', value))
        value = copy.deepcopy(baseline); value['directories'][0][1].append((b'second', first)); cases.append(('multiple-parents', value))
        value = copy.deepcopy(baseline); value['directories'].pop(0)
        value['directories'][1][1].append((b'cycle', first)); cases.append(('disconnected-cycle', value))
        value = copy.deepcopy(baseline); value['directories'][1][1].append((b'self', first)); cases.append(('self-cycle', value))
        for name, patches in [
            ('patch-overlap-new', [declaration(first)]),
            ('patch-file', [declaration(original['data.bin']['serial'])]),
            ('patch-absent', [declaration(first + 2)]),
            ('patch-duplicate', [declaration(baseline['root_serial']), declaration(baseline['root_serial'])]),
            ('patch-mode', [declaration(baseline['root_serial'], 0o4755)]),
            ('patch-nanos', [declaration(baseline['root_serial'], nanos=1_000_000_000)]),
        ]:
            value = copy.deepcopy(baseline); value['patches'] = patches; cases.append((name, value))
        local_cases = {'zero', 'above-limit', 'root', 'mode', 'nanos', 'order', 'duplicate',
                       'missing-own-directory-row', 'overlap', 'patch-overlap-new',
                       'patch-duplicate', 'patch-mode', 'patch-nanos'}
        tag, stage = peer.command(captured(snapshot, preparation(snapshot), command=3))
        assert tag == 'Stage'
        with check(report, 'shared-admission-refusals') as row:
            row['attempts'] = []
            for name, value in cases:
                for label, opcode, profile, payload in [('update', 5, 1, encode_preparation(value)),
                    ('stage', 7, 2, captured(snapshot, value, 3)), ('commit', 7, 2, captured(snapshot, value))]:
                    result = peer.call(opcode, payload, profile, failure=1, allow_local=name in local_cases)
                    assert peer.branch(branch) == snapshot and peer.commits(branch) == history
                    assert peer.query(b'\x09' + b'\x75' * 32) == ('Stage', stage)
                    row['attempts'].append({'case': name, 'operation': label, **result})
        with check(report, 'retained-history-and-reservations') as row:
            assert peer.attributes(old, b'data.bin') == original['data.bin']
            assert peer.listing(old, b'') == original_listing
            assert saved(peer, preparation(snapshot))['root'] == old
            tag, removed = peer.command(b'\x07' + b'\x75' * 32 + struct.pack('>Q', stage['token']))
            assert tag == 'Discarded' and removed == 1
            row.update(branch_unchanged=True, history_unchanged=True, prior_stage_preserved_then_explicitly_discarded=True)
    after = peer.reserve(snapshot['scope'], 1)
    assert after['start'] == first + count
    report['subsequent_reservation'] = printable(after)


def execute(args, report):
    fixture = json.loads(args.fixture.read_text())
    assert fixture['status'] == 'PASS'
    master = args.fixture.parent / 'service/store.sqlite'
    digest = sha(master)
    assert digest == fixture['closed_master_sha256']['store.sqlite']
    directory = args.output / 'service'; directory.mkdir()
    for suffix in ('-wal', '-shm', '-journal'):
        assert not Path(str(master) + suffix).exists()
    copied = time.monotonic()
    shutil.copyfile(master, directory / 'store.sqlite')
    assert sha(directory / 'store.sqlite') == digest
    report['fixture_reuse'] = {'receipt': str(args.fixture), 'receipt_sha256': sha(args.fixture),
        'closed_store_sha256': digest, 'clone_method': 'independent-byte-copy',
        'copy_seconds': time.monotonic() - copied, 'history': 'fresh live CREATE=1; no catalog copied',
        'cache_claim': None}
    private, server_private = os.urandom(32).hex(), os.urandom(32).hex()
    public, server_public = route.public_key(private), route.public_key(server_private)
    env = os.environ.copy()
    env.update(LAYERFS_PRIVATE_KEY=server_private,
        LAYERFS_PEERS=f'1,{public},{int(time.time()) + 3600},255', LAYERFS_LISTEN='127.0.0.1:0',
        LAYERFS_STORE=str(directory / 'store.sqlite'), LAYERFS_TELEMETRY='off',
        LAYERFS_HISTORY_CATALOG=str(directory / 'history.sqlite'), LAYERFS_HISTORY_CREATE='1',
        LAYERFS_HISTORY_BINDING='pair1-stage', LAYERFS_HISTORY_INCARNATION='1',
        LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(), LAYERFS_CONSTRUCTION_WORKERS='1')
    service = subprocess.Popen([route.BIN / 'layerfs-server'], env=env, stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    peer = None; readiness = ''; cleanup_errors = []
    report['owned_service_pid'] = service.pid
    try:
        readiness = mount.line_until(service); assert 'ready' in readiness
        port = int(readiness.strip().rsplit(':', 1)[1])
        boot = stage_route.bootstrap(directory, port, private, server_public, bytes.fromhex(fixture['file_root']), False)
        report['bootstrap'] = boot
        peer = Peer(directory, port, private, server_public)
        snapshot = peer.branch(bytes.fromhex(boot['branch']))
        exercise(peer, snapshot, args.case, report)
    finally:
        try:
            if peer is not None:
                report['client_forced_cleanup'] = peer.cleanup()
        except BaseException as error:
            cleanup_errors.append({'owner': 'client', 'error': repr(error)})
        try:
            if service.poll() is None:
                service.stdin.close()
                try:
                    service.wait(timeout=6)
                except subprocess.TimeoutExpired:
                    report['service_forced_cleanup'] = True
                    service.kill(); service.wait(timeout=6)
        except BaseException as error:
            cleanup_errors.append({'owner': 'service', 'pid': service.pid, 'error': repr(error)})
        report['cleanup_errors'] = cleanup_errors
        report['service_exit'] = service.poll()
        if service.poll() is not None:
            (args.output / 'service.stderr').write_bytes(readiness.encode() + service.stderr.read())
    with check(report, 'cleanup') as row:
        assert not cleanup_errors and service.returncode == 0 and not report.get('service_forced_cleanup')
        assert not report.get('client_forced_cleanup')
        assert sha(master) == digest
        row.update(service_exit=service.returncode, closed_master_unchanged=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--binaries', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--case', choices=CASES, required=True)
    args = parser.parse_args()
    for name in ('fixture', 'binaries', 'output'):
        setattr(args, name, getattr(args, name).resolve())
    route.BIN = args.binaries
    args.output.mkdir(parents=True, exist_ok=False)
    os.environ['LAYERFS_CONSTRUCTION_WORKERS'] = '1'
    report = {'status': 'FAIL', 'mode': 'functional-native-service-prepared-directories', 'case': args.case,
        'checks': [{'id': name, 'status': 'NOT_RUN'} for name in CASES[args.case]],
        'hard_budget_seconds': 60, 'request_budget_ms': 5000, 'performance_claim': False, 'cache_claim': None,
        'allocation_provenance_claim': False, 'construction_workers': 1,
        'container_cpu_quota': 'not applicable: host Service and host native client; no Docker container',
        'not_run': ['Workspace namespace overlay', 'kernel FUSE mkdir', 'prepared npm workload',
                    'unknown Reserve outcome/no-replay schedule', 'hard RSS/performance qualification']}
    started = time.monotonic()
    def expired(_signal, _frame):
        raise TimeoutError('complete native prepared-directory selection exceeded 60 seconds')
    signal.signal(signal.SIGALRM, expired); signal.alarm(60)
    try:
        space = stage_route.isolation.namespace()
        for path in (args.fixture, args.binaries, args.output):
            space.assert_owned(path, 'prepared directory proof input/output')
        report.update(source=mount.checked(['git', 'rev-parse', 'HEAD'], text=True).stdout.strip(),
            product_inputs_sha256=mount.product_inputs(), driver_sha256=sha(Path(__file__)),
            binaries={name: sha(route.BIN / name) for name in ('layerfs-server', 'layerfs-daemon', 'examples/public_key')},
            resource_isolation=space.as_fields(),
            caller_dependencies_sha256={str(Path(module.__file__).resolve().relative_to(ROOT)): sha(Path(module.__file__))
                for module in tuple(sys.modules.values()) if getattr(module, '__file__', None)
                and Path(module.__file__).resolve().is_relative_to(ROOT) and Path(module.__file__).suffix == '.py'})
        with space.lock_path.open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            execute(args, report)
        report['status'] = 'PASS'
    except BaseException as error:
        report['failure'] = repr(error)
        raise
    finally:
        signal.alarm(0)
        report['command_wall_seconds'] = time.monotonic() - started
        (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
