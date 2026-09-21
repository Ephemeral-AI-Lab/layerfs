#!/usr/bin/env python3
"""Real writable daemon, mounted edits and authenticated explicit Commit.

Closed content bytes are reused; every selection creates its own live C5 producer.
"""
import ctypes
import hashlib
import itertools
import json
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import sys
import time
import uuid

import control_unmount as driver
import control_commit_wire as wire
import stage_route

driver.MODE = 'functional-authenticated-workspace-commit'
driver.ENTRY_SOURCE = Path(__file__)
driver.CASES = {
    'commit_repeated': ['mounted-existing-file-A-B-A-explicit-Commits-install-exact-C5-heads-and-revisions'],
    'commit_dirty_signal': ['dirty-signal-detaches-with-control-live-until-explicit-Commit-remount-and-close'],
    'commit_control_loss': ['one-Commit-lost-control-terminal-is-Unknown-with-independent-current-observations-no-replay'],
    'commit_live_successor': ['actual-C2-RESERVED-lock-save-allows-mounted-live-successor-and-retains-control-during-signal'],
    'commit_authority': ['Commit-grant-target-service-authority-and-expiry-are-independent'],
    'commit_refusals': ['readonly-Commit-partial-input-and-budget-refusals-preserve-source'],
    'commit_denied': ['known-service-Commit-denial-retains-full-typed-failure-with-explicit-native-cleanup-refusal'],
}
driver.NOT_RUN = ['upstream native Unknown Commit failure (control-terminal loss is a separate outer delivery case)',
                  'namespace/new-inode operations', 'prepared npm workload', 'R6',
                  'hard RSS/cgroup/performance qualification', 'failed-submission discard/recovery']

TARGET = {'workspace': b'read', 'incarnation': b'\x71' * 32}
COMMIT_MS = 10_000
DISK_BYTES = 64 * 1024 * 1024


def begin_commit(client, identity, **options):
    driver.begin(client, identity, opcode=14, budget=options.pop('budget', COMMIT_MS), **options)


def commit(client, identity, **options):
    begin_commit(client, identity, **options)
    return wire.receive(client, workspace=options.get('workspace', b'read'),
                        incarnation=options.get('incarnation', b'\x71' * 32))


def current(client, identity, writable=True):
    kind, result = driver.status.query(client, identity)
    assert kind == 6 and result['workspace'] == 'read' and result['incarnation'] == '71' * 32, result
    assert ('generation' in result) == writable, result
    return result


def refused(result, code):
    assert result == {'kind': 'failure', 'code': code, 'unknown': 0, 'cleanup': 0}, result


class Witness:
    """Independent public observation calls; no idle connection survives a schedule."""
    def __init__(self, port, private, public):
        self.authority = port, private, public
        self.client = None
        self.ids = itertools.count(1)

    def request(self, opcode, body, profile=1):
        assert self.client is None
        self.client = driver.status.controller(*self.authority)
        identity = next(self.ids)
        self.client.stdin.write(driver.route.begin(identity, opcode, body, profile=profile))
        self.client.stdin.write(driver.route.frame(4, identity, struct.pack('>Q', 0)))
        self.client.stdin.flush()
        data = bytearray(); end = time.monotonic() + 10
        while True:
            header = driver.route.read_exact(self.client.stdout.fileno(), 20, end)
            assert header[:4] == b'LFB1'
            kind, flags, reserved, response_id, length = struct.unpack('>BBHQI', header[4:])
            assert flags == reserved == 0 and response_id == identity and length <= 32768
            value = driver.route.read_exact(self.client.stdout.fileno(), length, end)
            if kind == 5:
                data.extend(value); assert len(data) <= 128 * 1024
            else:
                assert kind == 6, (kind, value)
                self.close()
                return value, bytes(data)

    def branch(self, branch):
        body, data = self.request(6, b'\x03' + bytes.fromhex(branch), 2)
        assert not data
        tag, result = driver.route.history(body); assert tag == 'BranchSnapshot'
        return wire.jsonable(result)

    def attributes(self, root, path):
        body, data = self.request(2, bytes.fromhex(root) + b'\x04' + driver.route.blob(path))
        assert not data
        r = driver.route.Reader(body); assert r.u8() == 9
        result = {'serial': r.u64(), 'kind': r.u8(), 'references': r.u64(),
                  'content': r.take(32).hex(), 'metadata': r.take(32).hex(), 'mode': int.from_bytes(r.take(4), 'big'),
                  'mtime': r.u64(), 'nanoseconds': int.from_bytes(r.take(4), 'big'), 'size': r.u64()}
        r.done(); return result

    def read(self, content, length):
        body, data = self.request(1, bytes.fromhex(content) + struct.pack('>QQ', 0, length))
        r = driver.route.Reader(body); assert r.u8() == 1 and r.u64() == length; r.done()
        assert len(data) == length
        return data

    def close(self):
        if self.client is not None:
            self.client.stdin.close(); assert self.client.wait(timeout=6) == 0, self.client.stderr.read()
            self.client = None


def small_expected(value):
    return bytes([value]) + bytes(i % 251 for i in range(1, 8192))


def large_prefix():
    return b''.join(hashlib.sha256(b'layerfs-control-commit-v1' + i.to_bytes(8, 'big')).digest()
                    for i in range(4096))


def mounted_edit(name, operation, value=0, resize=0):
    code = """import hashlib,json,os,sys
from pathlib import Path
p=Path('/layerfs/workspace/read'); op=sys.argv[1]; value=int(sys.argv[2]); size=int(sys.argv[3])
fd=os.open(p/'data.bin',os.O_RDWR)
try:
    if size: os.ftruncate(fd,size)
    if op=='byte': data=bytes([value])
    elif op=='live': data=b'LIVE'
    elif op=='large': data=b''.join(hashlib.sha256(b'layerfs-control-commit-v1'+i.to_bytes(8,'big')).digest() for i in range(131072))
    else: raise AssertionError(op)
    assert os.pwrite(fd,data,0)==len(data)
    assert os.pread(fd,min(len(data),131072),0)==data[:131072]
finally: os.close(fd)
assert (p/'alias').stat().st_ino==(p/'data.bin').stat().st_ino
print(json.dumps({'bytes_written':len(data),'input_sha256':hashlib.sha256(data).hexdigest(),
                  'size':(p/'data.bin').stat().st_size,'inode':(p/'data.bin').stat().st_ino}))
"""
    return json.loads(driver.mount.checked(['docker', 'exec', name, 'python3', '-c', code,
                                           operation, str(value), str(resize)], text=True).stdout)


def mounted_read(name, expected, size):
    code = """import hashlib,json,sys
from pathlib import Path
p=Path('/layerfs/workspace/read'); n=int(sys.argv[1]); expected_size=int(sys.argv[2])
data=(p/'data.bin').open('rb').read(n); alias=(p/'alias').open('rb').read(n)
assert data==alias and (p/'data.bin').stat().st_ino==(p/'alias').stat().st_ino
assert (p/'data.bin').stat().st_size==expected_size
print(json.dumps({'bytes_read':len(data),'sha256':hashlib.sha256(data).hexdigest(),
                  'inode':(p/'data.bin').stat().st_ino,'mode':(p/'data.bin').stat().st_mode&0o777}))
"""
    result = json.loads(driver.mount.checked(['docker', 'exec', name, 'python3', '-c', code,
                                              str(len(expected)), str(size)], text=True).stdout)
    assert result['sha256'] == hashlib.sha256(expected).hexdigest() and result['bytes_read'] == len(expected)
    return result


def held_live_files(name):
    code = """import hashlib,json,os,sys
fd=os.open('/layerfs/workspace/read/data.bin',os.O_RDWR)
alias=os.open('/layerfs/workspace/read/alias',os.O_RDONLY)
try:
    assert os.fstat(fd).st_ino==os.fstat(alias).st_ino
    print('OPENED',flush=True)
    assert sys.stdin.readline()=='WRITE\\n'
    assert os.pwrite(fd,b'LIVE',0)==4
    data=os.pread(fd,131072,0)
    assert data[:4]==b'LIVE' and data==os.pread(alias,131072,0)
    stat=os.fstat(fd); other=os.fstat(alias)
    assert stat.st_ino==other.st_ino and stat.st_size==other.st_size==4194304
    print(json.dumps({'bytes_written':4,'bytes_read':len(data),'sha256':hashlib.sha256(data).hexdigest(),
                      'inode':stat.st_ino,'size':stat.st_size,'route':'pwrite/pread/fstat on pre-opened data and alias FDs'}),flush=True)
    assert sys.stdin.readline()=='CLOSE\\n'
finally:
    os.close(alias); os.close(fd)
print('CLOSED',flush=True)
"""
    return subprocess.Popen(['docker', 'exec', '-i', name, 'python3', '-u', '-c', code],
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def helper_line(process):
    end = time.monotonic() + 3; line = bytearray()
    while len(line) < 4096:
        line.extend(driver.route.read_exact(process.stdout.fileno(), 1, end))
        if line[-1] == 10: return line.decode().strip()
    raise AssertionError('unbounded helper observation')


def verify_saved(witness, branch, result, expected, size, previous=None):
    assert result['kind'] == 'Completed' and result['outcome']['kind'] in ('Committed', 'UpToDate'), result
    outcome = result['outcome']; observed = witness.branch(branch)
    assert observed['effective_root'] == outcome['root'], (observed, outcome)
    if outcome['kind'] == 'Committed':
        assert observed['branch']['head_commit'] == outcome['commit']
        if previous is not None: assert outcome['parent'] == previous['branch']['head_commit']
    else: assert observed['branch']['head_commit'] == outcome['head']
    data = witness.attributes(outcome['root'], b'data.bin'); alias = witness.attributes(outcome['root'], b'alias')
    assert data == alias and data['size'] == size, (data, alias)
    assert witness.read(data['content'], len(expected)) == expected
    return {'branch': observed, 'attributes': data}


def completed(client, identity, before):
    result = commit(client, identity); assert result['kind'] == 'Completed', result
    after = current(client, identity + 1)
    assert result['generation'] == before['generation'] and after['generation'] == result['generation'] + 1
    assert after['revision'] == result['revision'] > before['revision']
    assert after['dirty_inodes'] == 0 and after['submission'] is None, after
    return result, after


def close_client(client, name, expected=0):
    driver.close(client, name, expected)


def execute(args, report):
    fixture = json.loads(args.fixture.read_text())
    assert fixture['status'] == 'PASS' and fixture['logical_bytes'] == 64 * 1024 * 1024
    service_dir = args.output / 'service'; service_dir.mkdir()
    master = args.fixture.parent / 'service/store.sqlite'; master_hash = driver.sha(master)
    assert master_hash == fixture['closed_master_sha256']['store.sqlite']
    shutil.copyfile(master, service_dir / 'store.sqlite'); assert driver.sha(service_dir / 'store.sqlite') == master_hash
    report['fixture_reuse'] = {'receipt': str(args.fixture), 'receipt_sha256': driver.sha(args.fixture),
        'closed_store_sha256': master_hash, 'clone_method': 'independent-byte-copy',
        'history': 'fresh live C5 producer; CREATE=1 and no copied/restarted catalog', 'cache_claim': None}
    report.update(commit_request_budget_ms=COMMIT_MS, commit_wire_max_ms=600000, caller_receive_seconds=11,
                  workspace_disk_budget_bytes=DISK_BYTES, fixture_logical_bytes=64 * 1024 * 1024)
    report['verification_coverage'] = ('Repeated edits verify the entire declared8192-byte file; live-successor '
        'verifies128KiB from captured and successor content, exact C5 roots/heads and alias identity. '
        'Mounted truncation to8192B or4MiB is an explicit workload operation; no full64MiB readback or performance claim.')
    peers = [(1, 'good', 63), (2, 'old', 31), (3, 'commit', 32), (4, 'none', 0), (5, 'expiry', 33)]
    keys = {name: os.urandom(32).hex() for name in ['service', 'writer', 'denied'] + [p[1] for p in peers]}
    public = {name: driver.route.public_key(key) for name, key in keys.items()}
    env = os.environ.copy()
    env.update(LAYERFS_PRIVATE_KEY=keys['service'],
        LAYERFS_PEERS=f'1,{public["writer"]},{int(time.time())+3600},255;2,{public["denied"]},{int(time.time())+3600},191',
        LAYERFS_STORE=str(service_dir / 'store.sqlite'), LAYERFS_LISTEN='0.0.0.0:0', LAYERFS_TELEMETRY='off',
        LAYERFS_HISTORY_CATALOG=str(service_dir / 'history.sqlite'), LAYERFS_HISTORY_CREATE='1',
        LAYERFS_HISTORY_BINDING='pair1-stage', LAYERFS_HISTORY_INCARNATION='1',
        LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(), LAYERFS_CONSTRUCTION_WORKERS='1')
    service = subprocess.Popen([driver.route.BIN / 'layerfs-service'], env=env, stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    name = 'layerfs-control-commit-' + uuid.uuid4().hex[:12]
    daemon = client = witness = proxy = live_files = None
    volume = container = paused = expected_retention = False
    service_ready = daemon_log = ''
    try:
        service_ready = driver.mount.line_until(service); assert 'ready' in service_ready
        port = int(service_ready.strip().rsplit(':', 1)[1])
        report['fixture'] = stage_route.bootstrap(service_dir, port, keys['writer'], public['service'],
                                                 bytes.fromhex(fixture['file_root']), False)
        branch = report['fixture']['branch']
        witness = Witness(port, keys['writer'], public['service']); previous = witness.branch(branch)
        report['initial_branch'] = previous
        driver.mount.checked(['docker', 'volume', 'create', name + '-root']); volume = True
        expiry = int(time.time()) + 10
        controls = ';'.join(f'{selector},{public[key]},{expiry if key=="expiry" else int(time.time())+3600},{grant}'
                            for selector, key, grant in peers)
        denied = args.case == 'commit_denied'; readonly = args.case == 'commit_refusals'
        selected = 'denied' if denied else 'writer'
        env = os.environ.copy(); env.update(LAYERFS_ENDPOINT=f'host.docker.internal:{port}',
            LAYERFS_SELECTOR='2' if denied else '1', LAYERFS_PRIVATE_KEY=keys[selected],
            LAYERFS_SERVER_KEY=public['service'], LAYERFS_TELEMETRY='off', LAYERFS_WORKSPACE_ROOT='/layerfs',
            LAYERFS_WORKSPACE_MAX_COUNT='2', LAYERFS_WORKSPACE_DISK_BUDGET_BYTES=str(DISK_BYTES),
            LAYERFS_CONTROL_LISTEN='0.0.0.0:23456', LAYERFS_CONTROL_PEERS=controls, LAYERFS_CONSTRUCTION_WORKERS='1')
        daemon = driver.mount.mount_process(args, env, 'branch:' + branch, name,
            mode='--mount-readonly' if readonly else '--mount-writable'); container = True
        report['runtime_nano_cpus'] = int(driver.mount.checked(
            ['docker', 'inspect', '--format', '{{.HostConfig.NanoCpus}}', name], text=True).stdout.strip())
        assert report['runtime_nano_cpus'] == 2_000_000_000
        first, second = driver.mount.line_until(daemon), driver.mount.line_until(daemon)
        daemon_log = first + second
        assert 'workspace control ready' in first and 'workspace ready' in second, daemon_log
        control_port = int(driver.mount.checked(['docker', 'port', name, '23456/tcp'], text=True).stdout.strip().rsplit(':', 1)[1])
        def controller(key='good', selector=1, endpoint=control_port):
            return driver.status.controller(endpoint, keys[key], public[selected], selector)
        client = controller(); report['initial_status'] = current(client, 1, not readonly)
        if args.case == 'commit_repeated':
            reports = []; saved = []; contents = []
            for index, byte in enumerate(b'ABA'):
                mounted_edit(name, 'byte', byte, 8192 if index == 0 else 0)
                before = current(client, 2 + index * 4)
                result, after = completed(client, 3 + index * 4, before)
                observed = verify_saved(witness, branch, result, small_expected(byte), 8192, previous)
                mounted_read(name, small_expected(byte), 8192)
                previous = observed['branch']; contents.append(observed['attributes']['content'])
                reports.append(result); saved.append({'status': after, **observed})
            assert contents[0] == contents[2] and contents[0] != contents[1]
            report.update(commits=reports, installed=saved, content_A_B_A=contents)
        elif args.case == 'commit_dirty_signal':
            mounted_edit(name, 'byte', ord('A'), 8192); before = current(client, 2)
            assert before['dirty_inodes'] == 1
            driver.mount.stop_mount(name)
            retained = driver.mount.line_until(daemon, timeout=12); daemon_log += retained
            assert 'workspace shutdown retained:' in retained and 'Busy' in retained, retained
            assert daemon.poll() is None
            after_signal = current(client, 3)
            assert not after_signal['mounted'] and after_signal['dirty_inodes'] == 1
            assert after_signal['revision'] == before['revision'] and not after_signal['stopping']
            result, after = completed(client, 4, after_signal)
            report['saved'] = verify_saved(witness, branch, result, small_expected(ord('A')), 8192, previous)
            assert driver.remount(client, 6)['outcome'] == 'Mounted'
            report.update(signal_retained=retained.strip(), control_after_signal=after_signal,
                          commit=result, installed=after, mounted=mounted_read(name, small_expected(ord('A')), 8192))
        elif args.case == 'commit_control_loss':
            mounted_edit(name, 'byte', ord('A'), 8192); before = current(client, 2)
            close_client(client, name); client = None
            proxy = driver.lost_result_proxy(control_port)
            client = controller(endpoint=proxy[0]); result = commit(client, 1)
            assert result == {'kind': 'failure', 'code': 12, 'unknown': 1, 'cleanup': 0}, result
            client.stdin.close(); assert client.wait(timeout=6) == 1; client = None
            proxy[1].join(6)
            for worker in proxy[2]: worker.join(6)
            assert not proxy[1].is_alive() and 'error' not in proxy[3] and proxy[3].get('withheld_ciphertext_bytes', 0) > 0
            driver.status.control_threads(name, 1); client = controller(); observed = current(client, 1)
            assert observed['generation'] == before['generation'] + 1 and observed['dirty_inodes'] == 0 and observed['submission'] is None
            head = witness.branch(branch); attr = witness.attributes(head['effective_root'], b'data.bin')
            assert head['branch']['head_commit'] != previous['branch']['head_commit']
            assert witness.read(attr['content'], 8192) == small_expected(ord('A'))
            report.update(unknown=result, native_current_status=observed, independent_branch=head,
                          mounted=mounted_read(name, small_expected(ord('A')), 8192), proxy=proxy[3],
                          qualification='Outer control terminal loss only; no reconstructed Commit receipt and no native Unknown claim; exactly one Commit submitted')
        elif args.case == 'commit_live_successor':
            assert args.lock_observer is not None
            observer_path = args.lock_observer.resolve(); driver.isolation.namespace().assert_owned(observer_path, 'C2 lock observer')
            library = ctypes.CDLL(str(observer_path)); observer = library.layerfs_reserved_lock_owner
            observer.argtypes = [ctypes.c_char_p]; observer.restype = ctypes.c_int
            lock_path = os.fsencode(service_dir / 'store.sqlite'); assert observer(lock_path) == 0
            report['input'] = mounted_edit(name, 'large', resize=4 * 1024 * 1024)
            live_files = held_live_files(name); assert helper_line(live_files) == 'OPENED'
            before = current(client, 2); begin_commit(client, 3)
            started = time.monotonic(); owner = 0
            while owner == 0 and time.monotonic() - started < 3:
                owner = observer(lock_path); assert owner >= 0 and service.poll() is None
            assert owner == service.pid, f'actual C2 RESERVED owner missed: {owner}'
            service.send_signal(signal.SIGSTOP); paused = True
            pid, stopped = os.waitpid(service.pid, os.WUNTRACED); assert pid == service.pid and os.WIFSTOPPED(stopped)
            assert observer(lock_path) == service.pid, 'C2 save ended before the owned service stopped'
            report['C2_boundary'] = {'service_pid': service.pid, 'reserved_owner': owner,
                'observer_sha256': driver.sha(observer_path), 'wait_seconds': time.monotonic() - started,
                'method': 'read-only F_GETLK on SQLite RESERVED byte, confirmed again after SIGSTOP'}
            driver.mount.stop_mount(name)
            retained = driver.mount.line_until(daemon, timeout=3); daemon_log += retained
            assert 'workspace shutdown retained:' in retained and 'Busy' in retained
            live_files.stdin.write(b'WRITE\n'); live_files.stdin.flush()
            report['live_while_service_stopped'] = json.loads(helper_line(live_files))
            expected_live = b'LIVE' + large_prefix()[4:]
            assert report['live_while_service_stopped']['sha256'] == hashlib.sha256(expected_live).hexdigest()
            assert report['live_while_service_stopped']['bytes_read'] == len(expected_live)
            assert observer(lock_path) == service.pid
            service.send_signal(signal.SIGCONT); paused = False
            result = wire.receive(client); assert result['kind'] == 'Completed', result
            captured = verify_saved(witness, branch, result, large_prefix(), 4 * 1024 * 1024, previous)
            live = current(client, 4)
            assert live['dirty_inodes'] == 1 and live['generation'] == result['generation'] + 1
            assert live['revision'] == result['revision']
            next_result, installed = completed(client, 5, live)
            successor = verify_saved(witness, branch, next_result, b'LIVE' + large_prefix()[4:], 4 * 1024 * 1024, captured['branch'])
            output, errors = live_files.communicate(b'CLOSE\n', timeout=6)
            assert live_files.returncode == 0 and output == b'CLOSED\n', (output, errors)
            live_files = None
            report.update(captured_commit=result, captured=captured, live_status=live,
                          successor_commit=next_result, successor=successor, installed=installed,
                          signal_retained=retained.strip(),
                          concurrent_open_qualification='Only pre-opened descriptors are proved during occupied remote admission; new path/FD open returned EBUSY in retained attempt01 and remains unqualified')
        elif args.case == 'commit_authority':
            mounted_edit(name, 'byte', ord('A'), 8192); before = current(client, 2)
            close_client(client, name); client = None
            for label, key, selector, options in [('old-mask31', 'old', 2, {}), ('no-grant', 'none', 4, {}),
                    ('wrong-target', 'good', 1, {'workspace': b'other'}),
                    ('wrong-incarnation', 'good', 1, {'incarnation': b'\x72' * 32})]:
                client = controller(key, selector); result = commit(client, 1, **options); refused(result, 3)
                close_client(client, name, 1); client = controller(); after = current(client, 1)
                assert (after['generation'], after['revision'], after['dirty_inodes']) == (before['generation'], before['revision'], 1)
                close_client(client, name); client = None
                report.setdefault('permission_refusals', []).append({'case': label, 'result': result})
            client = driver.status.controller(port, keys['writer'], public['service'])
            result = commit(client, 1); refused(result, 2)
            client.stdin.close(); assert client.wait(timeout=6) == 1; client = None
            report['service_refusal'] = result
            client = controller('commit', 3); assert driver.status.query(client, 1) == (7, {'code': 3, 'unknown': 0, 'cleanup': 0})
            close_client(client, name, 1); client = controller('expiry', 5)
            identity = 1
            while time.time() < expiry - 1:
                current(client, identity); identity += 1; time.sleep(.1)
            while time.time() < expiry: time.sleep(min(.05, max(0, expiry-time.time())))
            result = commit(client, identity); refused(result, 3)
            close_client(client, name, 1); client = controller('commit', 3)
            result = commit(client, 1); assert result['kind'] == 'Completed', result
            close_client(client, name); client = controller(); after = current(client, 1)
            assert after['revision'] == result['revision'] and after['dirty_inodes'] == 0
            report.update(commit_only_result=result, installed=after,
                          saved=verify_saved(witness, branch, result, small_expected(ord('A')), 8192, previous))
        elif args.case == 'commit_refusals':
            result = commit(client, 2); refused(result, 3)
            close_client(client, name, 1); client = controller(); current(client, 1, False)
            result = commit(client, 2, budget=50); refused(result, 11)
            close_client(client, name, 1); client = controller(); current(client, 1, False)
            begin_commit(client, 2, budget=500, end=False)
            driver.status.control_threads(name, 2); client.stdin.close()
            partial = wire.receive(client); assert partial['kind'] == 'failure'
            assert client.wait(timeout=6) == 1; client = None
            driver.status.control_threads(name, 1); client = controller()
            report.update(partial_result=partial, after=current(client, 1, False))
            assert witness.branch(branch) == previous
            report['readonly_source'] = mounted_read(name, bytes(i % 251 for i in range(8192)), 64 * 1024 * 1024)
        elif args.case == 'commit_denied':
            mounted_edit(name, 'byte', ord('A'), 8192); before = current(client, 2)
            result = commit(client, 3)
            assert result['kind'] == 'Failed' and result['phase'] == 'CompositeCommit' and result['disposition'] == 'KnownBeforeCommit', result
            assert result['generation'] == before['generation'] and result['cause']['code'] == 3 and result['cause']['unknown'] == 0
            assert result['known_stage'] is None and result['known_outcome'] is None and result['installed_revision'] is None
            retained = current(client, 4); submission = retained['submission']; assert submission is not None
            assert submission['commit']['failure'] == 'KnownBeforeCommit' and submission['commit']['phase'] == 'CompositeCommit'
            assert witness.branch(branch) == previous
            assert driver.unmount(client, 5)['outcome'] == 'Unmounted'
            close = driver.close_clean(client, 6); assert close['outcome'] == 'Retained' and close['code'] == 13
            report.update(typed_failure=result, retained_status=retained, close_refusal=close,
                          qualification='Known service failure with retained submission; native clean close refused, external teardown only')
            expected_retention = True
        else: raise AssertionError(args.case)
        close_client(client, name); client = None
        witness.close(); witness = None
        if expected_retention:
            assert daemon.poll() is None
            report['native_cleanup'] = 'REFUSED: retained failed submission; no clean-close/reuse qualification'
        else:
            driver.mount.stop_mount(name); assert daemon.wait(timeout=12) == 0
            errors = daemon.stderr.read(); daemon_log += errors.decode(errors='replace')
            assert 'workspace closed' in daemon_log
            driver.mount.checked(['docker', 'exec', name, 'test', '!', '-e', '/layerfs/workspace/read'])
            report['native_cleanup'] = 'PASS: current writable/readonly owner unmounted and cleanly closed'
        report['checks'] = [{'id': key, 'status': 'PASS'} for key in driver.CASES[args.case]]
    finally:
        original_failure = sys.exc_info()[0] is not None
        cleanup_failures = []
        def cleanup_attempt(owner, operation):
            try: return operation()
            except BaseException as error:
                cleanup_failures.append({'owner': owner, 'failure': repr(error)})
                return None
        def kill_wait(process):
            process.kill(); return process.wait(timeout=6)
        if paused:
            cleanup_attempt('resume-owned-service', lambda: service.send_signal(signal.SIGCONT))
        if live_files is not None and live_files.poll() is None:
            report['forced_live_helper_cleanup'] = True
            cleanup_attempt('live-files-helper', lambda: kill_wait(live_files))
        if client is not None and client.poll() is None:
            report['forced_client_cleanup'] = True
            cleanup_attempt('control-client', lambda: kill_wait(client))
        if witness is not None and witness.client is not None and witness.client.poll() is None:
            report['forced_witness_cleanup'] = True
            cleanup_attempt('witness-client', lambda: kill_wait(witness.client))
        if proxy:
            for index, worker in enumerate([proxy[1], *proxy[2]]):
                cleanup_attempt(f'proxy-worker-{index}', lambda worker=worker: worker.join(6))
                if worker.is_alive(): cleanup_failures.append({'owner': f'proxy-worker-{index}', 'failure': 'still alive after bounded join'})
        report['runtime_identity'] = {'container': name if container else None,
                                      'volume': name + '-root' if volume else None}
        if container:
            report['retained_container'] = name
            if daemon is not None and daemon.poll() is None: report['forced_daemon_cleanup'] = True
            removed = cleanup_attempt('remove-container', lambda: driver.mount.checked(['docker', 'rm', '-f', name]))
            if removed is not None: report.pop('retained_container')
        if daemon is not None:
            cleanup_attempt('wait-daemon-client', lambda: daemon.wait(timeout=10))
            if daemon.poll() is None:
                report['forced_daemon_client_cleanup'] = True
                cleanup_attempt('stop-daemon-client', lambda: kill_wait(daemon))
            if daemon.poll() is not None:
                cleanup_attempt('save-daemon-diagnostics', lambda: (args.output / 'daemon.stderr').write_bytes(daemon_log.encode() + daemon.stderr.read()))
        if service.poll() is None:
            cleanup_attempt('close-service-input', service.stdin.close)
            cleanup_attempt('wait-service', lambda: service.wait(timeout=10))
            if service.poll() is None:
                report['forced_service_cleanup'] = True
                cleanup_attempt('stop-service', lambda: kill_wait(service))
        if service.poll() is not None:
            cleanup_attempt('save-service-diagnostics', lambda: (args.output / 'service.stderr').write_bytes(service_ready.encode() + service.stderr.read()))
        else:
            report['retained_service_pid'] = service.pid
        if volume:
            report['retained_volume'] = name + '-root'
            removed = cleanup_attempt('remove-volume', lambda: driver.mount.checked(['docker', 'volume', 'rm', name + '-root']))
            if removed is not None: report.pop('retained_volume')
        if cleanup_failures:
            report['cleanup_failures'] = cleanup_failures
            if not original_failure: raise RuntimeError(f'independent cleanup failed: {cleanup_failures}')
    assert service.returncode == 0 and driver.sha(master) == master_hash
    report['master_unchanged'] = True
    report['cleanup'] = ('EXTERNAL_ONLY: declared retained failure; owned runtime forcibly removed, no native clean-close claim'
                         if expected_retention else 'PASS: native clean close and owned runtime/volume removal')


driver.execute = execute

if __name__ == '__main__':
    driver.main()
