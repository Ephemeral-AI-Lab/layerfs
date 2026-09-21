#!/usr/bin/env python3
"""One registered functional Workspace stage selection, never a measurement.

Reuse a byte copy of the closed large-file Store. A fresh, live C5 producer
creates the small fixture namespace; no reopened-history mutation continuity.
The native-save cases observe SQLite's RESERVED byte without taking a lock.
"""
import argparse
import ctypes
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import select
import shutil
import signal
import socket
import threading
import struct
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'core/crates/layerfs-daemon/tests'))
import history_route as route
import mounted_read as mounted
import payload_route as payload
sys.path.insert(0, str(ROOT / 'core/benchmark/fs-bench-pro-storage-content/shared'))
import isolation

CASES = {
    'headroom': ['reserved-stage-progress-with-ordinary-disk-quota-occupied', 'exact-stage-and-retained-submission'],
    'metadata_only': ['metadata-only-stage-preserves-root-without-file-save', 'exact-stage-and-retained-submission'],
    'completion_failure': ['known-file-save-survives-native-completion-publication-failure'],
    'frontier': ['all-104-inode-completions-persist-and-stage', 'exact-stage-and-retained-submission'],
    'lowering': ['normalized-splice-lowering-and-streamed-exact-input', 'exact-stage-and-retained-submission'],
    'head_moved': ['stale-captured-head-preserves-exact-conflict-and-live-state'],
    'semantics': ['capture-live-successor-and-consumer-admission',
                  'frozen-bytes-attributes-aliases-and-one-tree-save',
                  'exact-stage-and-retained-submission'],
    'native_save': ['actual-service-save-with-live-successor-progress',
                    'exact-stage-and-retained-submission'],
    'unknown_save': ['unknown-save-retains-G-and-live-successor'],
    'metadata_denied': ['known-metadata-denial-retains-saved-file-G-and-D1'],
}
REQUIREMENTS = {
    'headroom': ['B-26', 'H-01'],
    'metadata_only': ['H-01', 'B-28'],
    'completion_failure': ['B-20', 'B-26', 'H-07', 'S-15'],
    'frontier': ['S-17', 'B-21', 'B-26', 'B-28'],
    'lowering': ['H-01', 'B-09', 'B-28'],
    'head_moved': ['H-04', 'H-07', 'S-15'],
    'semantics': ['S-02', 'S-03', 'S-10', 'S-12', 'S-13', 'S-15', 'H-01', 'H-02', 'B-28'],
    'native_save': ['S-03', 'S-11', 'H-01'],
    'unknown_save': ['S-10', 'S-15', 'H-07', 'H-09'],
    'metadata_denied': ['S-10', 'S-15', 'H-07'],
}


TEST_SOURCE = Path(__file__).with_name('stage.rs')
ENTRY_SOURCE = Path(__file__)
TEST_PREFIX = 'stage_'
TEST_MARKER = 'STAGE_CHECK'
MODE = 'functional-workspace-stage'
REQUIREMENT_SCOPE = 'stage-only subsets; no full Pair 1 completion'
LIMIT_CASES = ('completion_failure',)
DENIED_COMMIT_CASE = None
PROXY_CASE = None
DATA_MODES = {}
CALLER_USERS = {}
NOT_RUN = ['Workspace CommitStaged/reconciliation and repeated Commits', 'mounted writes',
           'namespace mutations and npm', 'R6', 'hard RSS/cgroup bound']

def lost_result_proxy(port):
    """Reuse daemon/tests/docker_faults.py's opaque native result-loss pattern."""
    listener = socket.socket(); listener.bind(('0.0.0.0', 0)); listener.listen(1); listener.settimeout(10)
    address = listener.getsockname()[1]; result = {}; uploads = []
    def exact(stream, count):
        data = bytearray()
        while len(data) < count:
            part = stream.recv(count-len(data))
            if not part: raise EOFError('native frame ended early')
            data.extend(part)
        return bytes(data)
    def relay():
        try:
            with listener.accept()[0] as client, socket.create_connection(('127.0.0.1', port), timeout=5) as service:
                client.settimeout(5)
                def upload():
                    try:
                        while chunk := client.recv(16384): service.sendall(chunk)
                    except OSError: pass
                thread = threading.Thread(target=upload, daemon=True); thread.start(); uploads.append(thread)
                for index in range(3):
                    header = exact(service, 4); length = struct.unpack('>I', header)[0]; assert length <= 32804
                    encrypted = exact(service, length)
                    if index < 2: client.sendall(header + encrypted)
                    else: result['withheld_ciphertext_bytes'] = length
                client.shutdown(socket.SHUT_RDWR); service.shutdown(socket.SHUT_RDWR)
        except BaseException as error: result['error'] = repr(error)
        finally: listener.close()
    controller = threading.Thread(target=relay, daemon=True); controller.start()
    return address, controller, uploads, result


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def bootstrap(service_dir, port, client_key, server_public, content, wide, data_mode=0o644):
    daemon, _ = route.start_daemon(service_dir, port, client_key, server_public, 1, None)
    try:
        manifest = (route.manifest_entry(0, b'', 2, 0o755, 1700000000, 0)
                    + route.manifest_entry(0, b'data.bin', 1, data_mode, 1700000001, 1, content)
                    + route.manifest_entry(0, b'other.bin', 1, 0o644, 1700000002, 2, content))
        if wide:
            for index in range(102):
                manifest += route.manifest_entry(0, f'f{index:03}'.encode(), 1, 0o644, 1700000003, index, content)
        command = b'\x01' + b'\xa1'*16 + route.blob(b'stage-fixture') + b'\xa2'*32 + struct.pack('>H', 105 if wide else 3) + manifest
        kind, body = route.exchange(daemon, 1, route.COMMAND_OPCODE, command, route.HISTORY_PROFILE)
        assert kind == 6, body
        tag, created = route.history(body); assert tag == 'StackCreated'
        command = b'\x02' + created['stack'] + b'\xa3'*16 + route.blob(b'stage') + b'\x01' + created['head_layer']
        kind, body = route.exchange(daemon, 2, route.COMMAND_OPCODE, command, route.HISTORY_PROFILE)
        assert kind == 6, body
        tag, snapshot = route.history(body); assert tag == 'BranchSnapshot'
        kind, body = route.exchange(daemon, 3, 2, created['root'] + b'\x01' + route.blob(b'data.bin'))
        assert kind == 6 and body[0] == 4, body
        serial = struct.unpack('>Q', body[1:9])[0]
        prepared = (b'\x05' + b'\xa4'*32 + snapshot['branch']['branch'] + route.optional(None) + created['head_layer']
                    + struct.pack('>Q', 1) + created['root'] + snapshot['scope']
                    + struct.pack('>QH', created['root_serial'], 1) + struct.pack('>QH', created['root_serial'], 1)
                    + route.blob(b'alias') + struct.pack('>QH', serial, 0))
        kind, body = route.exchange(daemon, 4, route.COMMAND_OPCODE, prepared, route.HISTORY_PROFILE)
        assert kind == 6, body
        tag, committed = route.history(body); assert tag == 'Committed'
        return {'branch': snapshot['branch']['branch'].hex(), 'root': committed['root'].hex(),
                'root_serial': created['root_serial'], 'file_serial': serial, 'data_mode': data_mode,
                'route': 'public InitLayerStack, Fork, explicit fixture alias Commit before Workspace attach'}
    finally:
        daemon.stdin.close()
        try: daemon.wait(timeout=6)
        except subprocess.TimeoutExpired: daemon.kill(); daemon.wait(timeout=6); raise
        (service_dir / 'fixture-daemon.stderr').write_bytes(daemon.stderr.read())
        assert daemon.returncode == 0


def execute(args, report, started):
    def remaining():
        value = 60 - (time.monotonic() - started)
        if value <= 0: raise TimeoutError('complete functional selection exceeded 60 seconds')
        return value
    def command(argv):
        return subprocess.run(argv, check=True, text=True, capture_output=True, timeout=remaining())
    fixture = json.loads(args.fixture.read_text())
    assert fixture['status'] == 'PASS' and fixture['logical_bytes'] == 64 * 1024 * 1024
    service_dir = args.output / 'service'; service_dir.mkdir()
    master = args.fixture.parent / 'service/store.sqlite'; master_hash = sha(master)
    shutil.copyfile(master, service_dir / 'store.sqlite'); assert sha(service_dir / 'store.sqlite') == master_hash
    report['fixture_reuse'] = {'receipt': str(args.fixture), 'receipt_sha256': sha(args.fixture),
                               'closed_store_sha256': master_hash, 'clone_method': 'independent-byte-copy',
                               'history': 'fresh live producer; no restarted write authority', 'cache_claim': None}
    server_key, client_key = os.urandom(32).hex(), os.urandom(32).hex()
    server_public, client_public = route.public_key(server_key), route.public_key(client_key)
    denied_key = os.urandom(32).hex() if args.case == DENIED_COMMIT_CASE else None
    grant = 127 if args.case == 'metadata_denied' else 255
    env = os.environ.copy()
    env.update(LAYERFS_PRIVATE_KEY=server_key, LAYERFS_PEERS=f'1,{client_public},{int(time.time())+3600},{grant}',
               LAYERFS_STORE=str(service_dir / 'store.sqlite'), LAYERFS_LISTEN='0.0.0.0:0', LAYERFS_TELEMETRY='off',
               LAYERFS_HISTORY_CATALOG=str(service_dir / 'history.sqlite'), LAYERFS_HISTORY_CREATE='1',
               LAYERFS_HISTORY_BINDING='pair1-stage', LAYERFS_HISTORY_INCARNATION='1',
               LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(), LAYERFS_CONSTRUCTION_WORKERS='1')
    if denied_key:
        env['LAYERFS_PEERS'] += f';2,{route.public_key(denied_key)},{int(time.time())+3600},63'
    service = subprocess.Popen([route.BIN / 'layerfs-service'], env=env, stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    name = 'layerfs-stage-' + uuid.uuid4().hex[:16]; volume = name + '-data'
    made_volume = created = stopped = killed = False; readiness = ''; child = None; proxy = None
    try:
        readiness = mounted.line_until(service, timeout=min(10, remaining())); assert 'ready' in readiness, readiness
        port = int(readiness.strip().rsplit(':', 1)[1])
        report['fixture'] = bootstrap(service_dir, port, client_key, server_public, bytes.fromhex(fixture['file_root']), args.case == 'frontier', DATA_MODES.get(args.case, 0o644))
        command(['docker', 'volume', 'create', volume]); made_volume = True
        command(['docker', 'run', '-d', '--privileged', '--name', name,
                 '--add-host', 'host.docker.internal:host-gateway',
                 '--mount', f'type=bind,src={args.test_binary.parent},dst=/runner,readonly',
                 '--mount', f'type=volume,src={volume},dst=/stage', args.image, 'sleep', 'infinity']); created = True
        command(['docker', 'exec', name, 'chmod', '700', '/stage'])
        caller = CALLER_USERS.get(args.case)
        if caller:
            command(['docker', 'exec', name, 'chown', caller, '/stage'])
        report['caller_identity'] = caller or '0:0'
        report['kernel'] = command(['docker', 'exec', name, 'uname', '-srmo']).stdout.strip()
        report['filesystem'] = command(['docker', 'exec', name, 'findmnt', '-n', '-o', 'FSTYPE', '-T', '/stage']).stdout.strip()
        assert report['filesystem'] == 'ext4'
        invocation = ['docker', 'exec', '-i', '-e', f'LAYERFS_ENDPOINT=host.docker.internal:{port}',
                      '-e', 'LAYERFS_PRIVATE_KEY', '-e', 'LAYERFS_SERVER_KEY',
                      '-e', f'LAYERFS_STAGE_BRANCH={report["fixture"]["branch"]}',
                      '-e', 'LAYERFS_STAGE_TEST_ROOT=/stage', '-e', 'LAYERFS_CONSTRUCTION_WORKERS=1', name,
                      '/runner/' + args.test_binary.name, '--ignored', '--nocapture', '--test-threads=1',
                      f'linux::{TEST_PREFIX}{args.case}', '--exact']
        if caller:
            invocation[3:3] = ['--user', caller]
        if args.case in LIMIT_CASES:
            binary_at = invocation.index(name) + 1
            invocation[binary_at:binary_at] = ['sh', '-c', 'trap "" XFSZ; exec "$@"', 'sh']
        report['test_selection'] = f'linux::{TEST_PREFIX}{args.case}'
        child_env = os.environ.copy(); child_env.update(LAYERFS_PRIVATE_KEY=client_key, LAYERFS_SERVER_KEY=server_public)
        if denied_key:
            child_env['LAYERFS_COMMIT_PRIVATE_KEY'] = denied_key
            invocation[3:3] = ['-e', 'LAYERFS_COMMIT_PRIVATE_KEY']
        if args.case == PROXY_CASE:
            proxy = lost_result_proxy(port)
            invocation[3:3] = ['-e', f'LAYERFS_COMMIT_ENDPOINT=host.docker.internal:{proxy[0]}']

        work = isolation.concurrent_work()
        for item in work:
            item['command'] = re.sub(r'(?i)([a-z_]*(?:key|secret|token|password)[a-z_]*=)[^\s]+', r'\1<redacted>', item['command'])
        report['resource_isolation']['concurrent_work'] = work
        if args.case in ('native_save', 'unknown_save'):
            assert args.lock_observer
            library = ctypes.CDLL(str(args.lock_observer))
            observer = library.layerfs_reserved_lock_owner; observer.argtypes = [ctypes.c_char_p]; observer.restype = ctypes.c_int
            lock_path = os.fsencode(service_dir / 'store.sqlite')
            assert observer(lock_path) == 0
        with (args.output / 'test.stderr').open('wb') as errors, (args.output / 'test.stdout').open('wb') as log:
            child = subprocess.Popen(invocation, env=child_env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors)
            pending = b''; output = b''; save_started = False
            while True:
                if not select.select([child.stdout], [], [], min(remaining(), 1))[0]:
                    continue
                chunk = os.read(child.stdout.fileno(), 65536)
                if not chunk: break
                log.write(chunk); log.flush(); output += chunk; pending += chunk
                while b'\n' in pending:
                    line, pending = pending.split(b'\n', 1)
                    if line.endswith(b'STAGE_SAVE_START'):
                        assert not save_started; save_started = True
                        observe_start = time.monotonic(); owner = 0
                        while owner == 0 and time.monotonic() - observe_start < 3:
                            owner = observer(lock_path); assert owner >= 0, owner
                            assert service.poll() is None
                        assert owner == service.pid, f'no service RESERVED lock observed: {owner}'
                        os.kill(service.pid, signal.SIGSTOP); stopped = True
                        pid, state = os.waitpid(service.pid, os.WUNTRACED)
                        assert pid == service.pid and os.WIFSTOPPED(state)
                        confirmed = observer(lock_path)
                        assert confirmed == service.pid, f'save boundary ended before stop: {confirmed}'
                        pause_start = time.monotonic()
                        report['native_save_observation'] = {
                            'boundary': 'EditFile call active; service owns SQLite RESERVED byte before and after SIGSTOP',
                            'observer': 'read-only F_GETLK at (1<<30)+1 length 1; no lock acquired',
                            'service_pid': service.pid, 'observed_pid': owner, 'stopped_pid': confirmed,
                            'lock_observer_sha256': sha(args.lock_observer), 'wait_seconds': pause_start-observe_start,
                            'not_a_claim': 'upload pause, terminal withholding, measured save latency or two-save overlap',
                        }
                        child.stdin.write(b'SAVE_PAUSED\n'); child.stdin.flush()
                    elif line == b'STAGE_LIVE_READY':
                        assert stopped and observer(lock_path) == service.pid
                        report['native_save_observation']['local_progress_while_stopped_seconds'] = time.monotonic() - pause_start
                        assert time.monotonic() - pause_start < 3
                        if args.case == 'unknown_save':
                            os.kill(service.pid, signal.SIGKILL); killed = True
                            report['expected_service_loss'] = 'SIGKILL at observed active C2 write transaction after local D1 progress'
                        else: os.kill(service.pid, signal.SIGCONT)
                        stopped = False
                        report['native_save_observation']['live_progress_before_resume_or_loss'] = True
                    elif line == b'STAGE_SAVE_DONE' and stopped:
                        raise AssertionError('save finished while service remained stopped')
            child.stdin.close(); child.wait(timeout=remaining()); report['test_exit'] = child.returncode
        text = output.decode(errors='replace')
        for check in report['checks']:
            if f'{TEST_MARKER} {check["id"]} PASS' in text: check['status'] = 'PASS'
        report['observations'] = []
        for line in text.splitlines():
            for marker in ('STAGE_RESOURCE ', 'STAGE_FAILURE ', 'STAGE_HEADROOM ', 'COMMIT_RESOURCE ', 'COMMIT_FAILURE ', 'COMMIT_HEADROOM ', 'COMMIT_CYCLES ', 'COMMIT_LATER_OBSERVATION '):
                if marker in line:
                    report['observations'].append(line[line.index(marker):]); break
        if proxy:
            proxy[1].join(timeout=min(5, remaining()))
            for thread in proxy[2]: thread.join(timeout=min(5, remaining()))
            report['native_result_loss'] = proxy[3] | {'scope': 'external encrypted result withheld; later queries are separate observations'}
            assert not proxy[1].is_alive() and all(not thread.is_alive() for thread in proxy[2])
            assert 'error' not in proxy[3] and proxy[3].get('withheld_ciphertext_bytes', 0) > 0
        assert child.returncode == 0 and all(check['status'] == 'PASS' for check in report['checks'])
        if save_started: assert report['native_save_observation']['live_progress_before_resume_or_loss']
        command(['docker', 'rm', '-f', name]); created = False
        command(['docker', 'volume', 'rm', volume]); made_volume = False
        report['cleanup'] = 'PASS: test process exited; owned Linux runtime/backing removed; no clean Workspace close claim'
    finally:
        if stopped: os.kill(service.pid, signal.SIGCONT)
        if child is not None and child.poll() is None:
            child.kill(); child.wait(timeout=6); report['test_forced_cleanup'] = True
        if service.poll() is None:
            if not killed: service.stdin.close()
            try: service.wait(timeout=min(6, max(0.01, 60 - (time.monotonic() - started))))
            except subprocess.TimeoutExpired:
                service.kill(); service.wait(timeout=6); report['service_forced_cleanup'] = True
        (args.output / 'service.stderr').write_bytes(readiness.encode() + service.stderr.read())
        report['service_exit'] = service.returncode
        if created: report['retained_container'] = name
        if made_volume: report['retained_volume'] = volume
    assert service.returncode == (-signal.SIGKILL if killed else 0) and not report.get('service_forced_cleanup')
    assert sha(master) == master_hash
    report['command_wall_seconds'] = time.monotonic() - started
    assert report['command_wall_seconds'] <= 60


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--binaries', type=Path, required=True)
    parser.add_argument('--test-binary', type=Path, required=True)
    parser.add_argument('--lock-observer', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--case', choices=CASES, required=True)
    parser.add_argument('--image', default='rust:1.85.1-bookworm')
    args = parser.parse_args()
    for name in ('fixture', 'binaries', 'test_binary', 'lock_observer', 'output'):
        if getattr(args, name): setattr(args, name, getattr(args, name).resolve())
    route.BIN = args.binaries; args.output.mkdir(parents=True, exist_ok=False)
    report = {'status': 'FAIL', 'mode': MODE, 'case': args.case,
              'checks': [{'id': key, 'status': 'NOT_RUN'} for key in CASES[args.case]],
              'packet_requirement_ids': REQUIREMENTS[args.case], 'requirement_scope': REQUIREMENT_SCOPE,
              'hard_budget_seconds': 60, 'stage_deadline_seconds': 25 if args.case == 'frontier' else 10,
              'commit_deadline_seconds': (25 if args.case == 'frontier' else 10) if MODE in ('functional-workspace-composite-commit', 'functional-workspace-resize') else (10 if MODE in ('functional-workspace-commit-staged', 'functional-workspace-maintenance') else None), 'performance_claim': False, 'cache_claim': None,
              'not_run': NOT_RUN}
    started = time.monotonic()
    def expired(_signal, _frame):
        raise TimeoutError('complete functional selection exceeded 60 seconds')
    signal.signal(signal.SIGALRM, expired); signal.alarm(60)
    try:
        report.update(source=payload.command(['git', 'rev-parse', 'HEAD'], cwd=ROOT).stdout.strip(),
                      product_inputs_sha256=payload.product_inputs(), driver_sha256=sha(Path(__file__)),
                      test_source_sha256=sha(TEST_SOURCE), entrypoint_sha256=sha(ENTRY_SOURCE),
                      helper_source_sha256=sha(Path(__file__).parent / 'support/native_workspace.rs'),
                      test_binary_sha256=sha(args.test_binary),
                      binaries={name: sha(route.BIN / name) for name in ('layerfs-service', 'layerfs-daemon', 'examples/public_key')},
                      image=payload.command(['docker', 'image', 'inspect', args.image, '--format', '{{.Id}}']).stdout.strip())
        space = isolation.namespace()
        for path in (args.output, args.fixture, args.test_binary, route.BIN): space.assert_owned(path, 'functional proof input/output')
        report['resource_isolation'] = space.as_fields() | {'artifact_root': str(args.output.parent),
            'run_output': str(args.output), 'build_target': str(ROOT / 'core/target'),
            'linux_build_target': str(ROOT / 'core/target-linux'),
            'shared_read_only': [str(args.fixture), str(args.test_binary)],
            'observation': 'process snapshot before public test; no quiet-host assertion'}
        with space.lock_path.open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            execute(args, report, started)
        report['status'] = 'PASS'
    except BaseException as error:
        report['failure'] = repr(error); raise
    finally:
        signal.alarm(0)
        report.setdefault('command_wall_seconds', time.monotonic() - started)
        (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__': main()
