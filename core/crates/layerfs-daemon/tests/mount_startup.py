#!/usr/bin/env python3
"""Actual daemon startup deadline/retained-owner proof, with external seccomp only.

The host reuses a closed read-only mounted fixture and immutable executables.
A Linux supervisor delays the real FUSE INIT writev, then lets that syscall run.
No product hooks, emulated successful replies, build, or performance claim.
"""
import argparse
import array
import ctypes
import errno
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import select
import shutil
import signal
import socket
import stat
import struct
import subprocess
import sys
import time
import uuid


MOUNT_PATH = '/layerfs/workspace/read'
HOLD_SECONDS = 10.25


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def supervise(command):
    """Runs only inside Linux; parent stays unfiltered, child execs the daemon."""
    class Filter(ctypes.Structure):
        _fields_ = [('code', ctypes.c_ushort), ('jt', ctypes.c_ubyte),
                    ('jf', ctypes.c_ubyte), ('k', ctypes.c_uint)]

    class Program(ctypes.Structure):
        _fields_ = [('length', ctypes.c_ushort), ('filters', ctypes.POINTER(Filter))]

    class Data(ctypes.Structure):
        _fields_ = [('nr', ctypes.c_int), ('arch', ctypes.c_uint),
                    ('ip', ctypes.c_ulonglong), ('args', ctypes.c_ulonglong * 6)]

    class Notification(ctypes.Structure):
        _fields_ = [('id', ctypes.c_ulonglong), ('pid', ctypes.c_uint),
                    ('flags', ctypes.c_uint), ('data', Data)]

    class Response(ctypes.Structure):
        _fields_ = [('id', ctypes.c_ulonglong), ('value', ctypes.c_longlong),
                    ('error', ctypes.c_int), ('flags', ctypes.c_uint)]

    class Sizes(ctypes.Structure):
        _fields_ = [('notification', ctypes.c_ushort), ('response', ctypes.c_ushort),
                    ('data', ctypes.c_ushort)]

    class Iovec(ctypes.Structure):
        _fields_ = [('base', ctypes.c_void_p), ('length', ctypes.c_size_t)]

    machine = platform.machine()
    if machine == 'aarch64':
        arch, seccomp_nr, writev_nr = 0xC00000B7, 277, 66
    elif machine == 'x86_64':
        arch, seccomp_nr, writev_nr = 0xC000003E, 317, 20
    else:
        raise RuntimeError(f'NOT_RUN: unsupported seccomp test architecture {machine}')
    assert sys.byteorder == 'little' and ctypes.sizeof(Iovec) == 16
    libc = ctypes.CDLL(None, use_errno=True)
    libc.syscall.restype = ctypes.c_long
    libc.process_vm_readv.restype = ctypes.c_ssize_t
    libc.process_vm_readv.argtypes = [ctypes.c_int, ctypes.POINTER(Iovec), ctypes.c_ulong,
                                    ctypes.POINTER(Iovec), ctypes.c_ulong, ctypes.c_ulong]
    libc.ioctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_void_p]

    def checked(value, operation):
        if value < 0:
            number = ctypes.get_errno()
            raise OSError(number, f'{operation}: {os.strerror(number)}')
        return value

    sizes = Sizes()
    checked(libc.syscall(seccomp_nr, 3, 0, ctypes.byref(sizes)), 'SECCOMP_GET_NOTIF_SIZES')
    assert sizes.notification == ctypes.sizeof(Notification) == 80
    assert sizes.response == ctypes.sizeof(Response) == 24 and sizes.data == ctypes.sizeof(Data) == 64
    # _IOWR('!', 0/1, struct); _IOW('!', 2, uint64_t), Linux generic ABI.
    recv_ioctl = (3 << 30) | (80 << 16) | (ord('!') << 8)
    send_ioctl = (3 << 30) | (24 << 16) | (ord('!') << 8) | 1
    valid_ioctl = (1 << 30) | (8 << 16) | (ord('!') << 8) | 2
    filters = (Filter * 7)(
        Filter(0x20, 0, 0, 4), Filter(0x15, 1, 0, arch), Filter(0x06, 0, 0, 0x80000000),
        Filter(0x20, 0, 0, 0), Filter(0x15, 0, 1, writev_nr),
        Filter(0x06, 0, 0, 0x7FC00000), Filter(0x06, 0, 0, 0x7FFF0000))
    program = Program(len(filters), filters)
    parent, child = socket.socketpair(socket.AF_UNIX, socket.SOCK_SEQPACKET)
    parent.settimeout(5)
    stderr_path, stdout_path = Path('/tmp/mount-startup.stderr'), Path('/tmp/mount-startup.stdout')
    errors = stderr_path.open('xb')
    output = stdout_path.open('xb')
    pid = os.fork()
    if pid == 0:
        try:
            parent.close()
            os.dup2(errors.fileno(), 2)
            os.dup2(output.fileno(), 1)
            checked(libc.prctl(38, 1, 0, 0, 0), 'PR_SET_NO_NEW_PRIVS')
            listener = checked(libc.syscall(seccomp_nr, 1, 8, ctypes.byref(program)),
                               'SECCOMP_FILTER_FLAG_NEW_LISTENER')
            child.sendmsg([b'L'], [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array('i', [listener]))])
            os.close(listener)
            child.close()
            os.execv(command[0], command)
        except BaseException as error:
            os.write(2, (f'external wrapper failed: {error!r}\n').encode())
            os._exit(125)
    child.close()
    errors.close()
    output.close()
    report = {'status': 'FAIL', 'daemon_pid': pid, 'kernel': platform.uname()._asdict(),
              'filter': 'native-arch writev USER_NOTIF; inherited across exec and clone; no TSYNC',
              'original_mount_budget_seconds': 10, 'hold_seconds': HOLD_SECONDS,
              'continued_notifications': 0, 'explicit_cleanup_signals': 0}
    listener = None
    reaped = False
    try:
        data, ancillary, flags, _ = parent.recvmsg(1, socket.CMSG_SPACE(array.array('i').itemsize))
        assert data == b'L' and flags == 0, (data, flags)
        descriptors = []
        for level, kind, payload in ancillary:
            assert (level, kind) == (socket.SOL_SOCKET, socket.SCM_RIGHTS)
            received = array.array('i'); received.frombytes(payload)
            descriptors.extend(received)
        assert len(descriptors) == 1, descriptors
        listener = descriptors[0]
        parent.close()

        def log():
            assert stderr_path.stat().st_size <= 1_048_576, 'unbounded daemon diagnostics'
            value = stderr_path.read_text(errors='replace')
            assert 'workspace ready ' not in value, value
            return value

        def mounted():
            rows = []
            for line in Path('/proc/self/mountinfo').read_text().splitlines():
                left, right = line.split(' - ', 1)
                if left.split()[4] == MOUNT_PATH:
                    assert right.split()[0] in ('fuse', 'fuse.layerfs')
                    assert right.split()[1] == 'layerfs', line
                    rows.append(line)
            assert len(rows) <= 1
            return rows

        def resume(notification):
            identity = ctypes.c_ulonglong(notification.id)
            checked(libc.ioctl(listener, valid_ioctl, ctypes.byref(identity)), 'NOTIF_ID_VALID')
            response = Response(notification.id, 0, 0, 1)  # CONTINUE the actual syscall.
            checked(libc.ioctl(listener, send_ioctl, ctypes.byref(response)), 'NOTIF_SEND CONTINUE')
            report['continued_notifications'] += 1

        def notification(timeout):
            if not select.select([listener], [], [], max(0, timeout))[0]:
                return None
            value = Notification()
            received = libc.ioctl(listener, recv_ioctl, ctypes.byref(value))
            if received < 0 and ctypes.get_errno() == errno.ENOENT:
                return None  # Last filtered task can exit between poll and receive.
            checked(received, 'NOTIF_RECV')
            assert value.data.arch == arch and value.data.nr == writev_nr
            return value

        def memory(tid, address, count):
            assert 0 < count <= 1024
            storage = ctypes.create_string_buffer(count)
            local = Iovec(ctypes.addressof(storage), count)
            remote = Iovec(address, count)
            received = checked(libc.process_vm_readv(tid, ctypes.byref(local), 1,
                                                      ctypes.byref(remote), 1, 0), 'process_vm_readv')
            assert received == count, (received, count)
            return storage.raw

        end = time.monotonic() + 12
        held = None
        while time.monotonic() < end:
            value = notification(min(.05, end - time.monotonic()))
            if value is None:
                continue
            fd = int(value.data.args[0])
            observed = os.stat(f'/proc/{value.pid}/fd/{fd}')
            if not stat.S_ISCHR(observed.st_mode) or observed.st_rdev != os.stat('/dev/fuse').st_rdev:
                resume(value)
                continue
            assert value.pid == pid, 'INIT must be the daemon main thread'
            assert 1 <= value.data.args[2] <= 8
            vectors = memory(value.pid, value.data.args[1], value.data.args[2] * ctypes.sizeof(Iovec))
            prefix = bytearray()
            total = 0
            for offset in range(0, len(vectors), ctypes.sizeof(Iovec)):
                vector = Iovec.from_buffer_copy(vectors[offset:offset + ctypes.sizeof(Iovec)])
                total += vector.length
                count = min(vector.length, max(0, 24 - len(prefix)))
                if count:
                    prefix.extend(memory(value.pid, vector.base, count))
            assert len(prefix) == 24
            length, error, unique, major, minor = struct.unpack('=IiQII', prefix)
            assert length == total and 24 <= length <= 128 and error == 0 and unique > 0 and major == 7
            rows = mounted(); assert rows
            report['init'] = {'notification_id': value.id, 'tid': value.pid, 'fd': fd,
                              'iovec_count': value.data.args[2], 'length': length,
                              'unique': unique, 'major': major, 'minor': minor, 'mountinfo': rows}
            held = value
            break
        assert held is not None, 'missing actual FUSE INIT writev'
        held_at = time.monotonic()
        while time.monotonic() - held_at < HOLD_SECONDS:
            value = notification(min(.05, HOLD_SECONDS - (time.monotonic() - held_at)))
            if value is not None:
                resume(value)
            log()
        report['actual_init_hold_seconds'] = time.monotonic() - held_at
        assert report['actual_init_hold_seconds'] > 10
        resume(held)
        end = time.monotonic() + 4
        while time.monotonic() < end:
            value = notification(.02)
            if value is not None:
                resume(value)
            if 'workspace startup cleanup retained: Deadline' in log():
                break
        before = log()
        marker = 'workspace mount startup failed: '
        originals = [line.removeprefix(marker) for line in before.splitlines() if line.startswith(marker)]
        assert originals == ['mount Deadline: Deadline'], before
        assert 'workspace startup cleanup retained: Deadline' in before, before
        assert 'workspace failed startup cleaned' not in before, before
        done, status = os.waitpid(pid, os.WNOHANG)
        if done:
            reaped = True
            report['daemon_exit'] = os.waitstatus_to_exitcode(status)
        assert not done, 'startup owner exited before explicit cleanup'
        report['retained_mountinfo'] = mounted(); assert report['retained_mountinfo']
        listeners = []
        for table in ('/proc/net/tcp', '/proc/net/tcp6'):
            for line in Path(table).read_text().splitlines()[1:]:
                fields = line.split()
                if fields[3] == '0A' and int(fields[1].rsplit(':', 1)[1], 16) == 23456:
                    listeners.append(line)
        assert listeners and before.count('workspace control ready ') == 1, before
        report['control_listener_after_failure'] = 'present'
        # Control starts before native mount construction. Check its acceptor
        # without claiming an authenticated operation from this raw TCP probe.
        with socket.create_connection(('127.0.0.1', 23456), timeout=3):
            until = time.monotonic() + 3
            while time.monotonic() < until:
                tasks = Path(f'/proc/{pid}/task')
                count = sum(path.read_text().strip().startswith('layerfs-control')
                            for path in tasks.glob('*/comm'))
                if count == 2: break
                value = notification(.02)
                if value is not None: resume(value)
            else: raise AssertionError('control acceptor did not own the pending handshake')
        report['control_availability'] = 'TCP accepted and control session observed; no authenticated-operation claim'
        report['startup_error'] = originals[0]
        # Observe retained ownership across an interval with no cleanup signal.
        until = time.monotonic() + .2
        while time.monotonic() < until:
            value = notification(.02)
            if value is not None:
                resume(value)
        assert mounted() and 'workspace failed startup cleaned' not in log()
        os.kill(pid, signal.SIGTERM)
        report['explicit_cleanup_signals'] = 1
        end = time.monotonic() + 12
        while time.monotonic() < end:
            done, status = os.waitpid(pid, os.WNOHANG)
            if done:
                reaped = True
                report['daemon_exit'] = os.waitstatus_to_exitcode(status)
                break
            value = notification(.02)
            if value is not None:
                resume(value)
        assert reaped and report['daemon_exit'] == 1, report
        final = log()
        assert final.count('workspace failed startup cleaned\n') == 1, final
        assert f'Error: {originals[0]}\n' in final, final
        assert not mounted() and not Path(MOUNT_PATH).exists()
        report['cleanup'] = 'PASS: one explicit signal detached and closed; original startup error exited 1'
        report['status'] = 'PASS'
    except BaseException as error:
        report['failure'] = repr(error)
    finally:
        parent.close()
        if not reaped:
            try:
                os.kill(pid, signal.SIGKILL)
                os.waitpid(pid, 0)
                report['forced_daemon_cleanup'] = True
            except ProcessLookupError:
                pass
        if listener is not None:
            os.close(listener)
        report['daemon_stderr'] = stderr_path.read_text(errors='replace')
        report['daemon_stdout'] = stdout_path.read_text(errors='replace')
    print(json.dumps(report), flush=True)
    return 0 if report['status'] == 'PASS' else 1


def execute(args, report, route, mount):
    fixture = json.loads(args.fixture.read_text())
    assert fixture['status'] == 'PASS' and fixture['mode'] == 'functional-mounted-proof'
    service_dir = args.output / 'service'; service_dir.mkdir()
    seals = {}
    for filename in ('store.sqlite', 'history.sqlite'):
        source, copied = args.fixture.parent / 'service' / filename, service_dir / filename
        seals[filename] = sha(source)
        shutil.copyfile(source, copied)
        assert sha(copied) == seals[filename]
    report['fixture_reuse'] = {'receipt': str(args.fixture), 'receipt_sha256': sha(args.fixture),
        'clone_method': 'independent-byte-copy', 'closed_master_sha256': seals,
        'history': 'read-only reopened fixture; no write authority', 'cache_claim': None}
    keys = {name: os.urandom(32).hex() for name in ('service', 'daemon', 'controller')}
    public = {name: route.public_key(value) for name, value in keys.items()}
    environment = os.environ.copy()
    environment.update(LAYERFS_PRIVATE_KEY=keys['service'],
        LAYERFS_PEERS=f'1,{public["daemon"]},{int(time.time())+3600},255',
        LAYERFS_STORE=str(service_dir / 'store.sqlite'), LAYERFS_LISTEN='0.0.0.0:0',
        LAYERFS_TELEMETRY='off', LAYERFS_HISTORY_CATALOG=str(service_dir / 'history.sqlite'),
        LAYERFS_HISTORY_BINDING='pair1-mounted-read', LAYERFS_HISTORY_CREATE='0',
        LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(), LAYERFS_CONSTRUCTION_WORKERS='1')
    service = subprocess.Popen([route.BIN / 'layerfs-server'], env=environment,
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    name = 'layerfs-startup-' + uuid.uuid4().hex[:12]
    made_volume = made_container = False
    supervisor = None
    try:
        ready = mount.line_until(service); assert 'ready' in ready, ready
        port = int(ready.strip().rsplit(':', 1)[1])
        mount.checked(['docker', 'volume', 'create', name + '-root']); made_volume = True
        daemon_values = dict(LAYERFS_ENDPOINT=f'host.docker.internal:{port}', LAYERFS_SELECTOR='1',
            LAYERFS_PRIVATE_KEY=keys['daemon'], LAYERFS_SERVER_KEY=public['service'],
            LAYERFS_WORKSPACE_ROOT='/layerfs', LAYERFS_WORKSPACE_MAX_COUNT='2',
            LAYERFS_CONTROL_LISTEN='0.0.0.0:23456',
            LAYERFS_CONTROL_PEERS=f'1,{public["controller"]},{int(time.time())+3600},7',
            LAYERFS_TELEMETRY='off', LAYERFS_CONSTRUCTION_WORKERS='1')
        environment = os.environ.copy()
        environment.update(daemon_values)
        command = ['docker', 'run', '-d', '--rm', '--name', name, '--cpus', '2', '--device', '/dev/fuse',
            '--cap-add', 'SYS_ADMIN', '--cap-add', 'SYS_PTRACE',
            '--security-opt', 'apparmor=unconfined', '--security-opt', 'seccomp=unconfined',
            '--add-host', 'host.docker.internal:host-gateway',
            '--mount', f'type=bind,src={args.linux_daemon},dst=/product/layerfs-daemon,readonly',
            '--mount', f'type=bind,src={Path(__file__).resolve()},dst=/proof.py,readonly',
            '--mount', f'type=volume,src={name}-root,dst=/layerfs']
        for key in daemon_values:
            command += ['-e', key]
        command += [report['runtime_image_id'], 'sleep', 'infinity']
        mount.checked(command, env=environment); made_container = True
        report['runtime_nano_cpus'] = int(mount.checked(
            ['docker', 'inspect', '--format', '{{.HostConfig.NanoCpus}}', name], text=True).stdout.strip())
        assert report['runtime_nano_cpus'] == 2_000_000_000
        supervisor = subprocess.Popen(['docker', 'exec', name, 'python3', '-u', '/proof.py',
            '--supervise', '/product/layerfs-daemon', '--mount-readonly', 'read', '71' * 32,
            '1', fixture['root'], '0', '0'], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        output, errors = supervisor.communicate(timeout=40)
        (args.output / 'supervisor.stdout').write_bytes(output)
        (args.output / 'supervisor.stderr').write_bytes(errors)
        observed = json.loads(output)
        (args.output / 'daemon.stderr').write_text(observed.pop('daemon_stderr', ''))
        (args.output / 'daemon.stdout').write_text(observed.pop('daemon_stdout', ''))
        report['observation'] = observed
        if observed['status'] == 'NOT_RUN':
            report['status'] = 'NOT_RUN'
            raise RuntimeError(f'NOT_RUN: {observed["failure"]}')
        assert supervisor.returncode == 0 and observed['status'] == 'PASS', observed
        report['checks'] = [{'id': name, 'status': 'PASS'} for name in (
            'real-FUSE-INIT-writev-held-past-original-startup-deadline',
            'late-constructor-retains-exact-owner-without-Workspace-ready',
            'control-listener-and-acceptor-remain-available-after-startup-mount-failure',
            'same-deadline-cleanup-refuses-without-detaching',
            'one-explicit-signal-cleans-and-preserves-original-startup-failure-exit')]
        report['cleanup'] = observed['cleanup']
    finally:
        if made_container:
            if supervisor is not None and supervisor.poll() is None:
                report['forced_supervisor_cleanup'] = True
            mount.checked(['docker', 'rm', '-f', name])
        if supervisor is not None and supervisor.poll() is None:
            output, errors = supervisor.communicate(timeout=8)
            (args.output / 'failed-supervisor.stdout').write_bytes(output)
            (args.output / 'failed-supervisor.stderr').write_bytes(errors)
        if service.poll() is None:
            service.stdin.close(); service.wait(timeout=8)
        (args.output / 'service.stderr').write_bytes(service.stderr.read())
        if made_volume:
            mount.checked(['docker', 'volume', 'rm', name + '-root'])


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--supervise':
        try:
            return supervise(sys.argv[2:])
        except BaseException as error:
            print(json.dumps({'status': 'NOT_RUN', 'failure': repr(error)}), flush=True)
            return 1
    import history_route as route
    import mounted_read as mount
    sys.path.insert(0, str(route.ROOT / 'core/benchmark/fs-bench-pro-storage-content/shared'))
    import isolation
    parser = argparse.ArgumentParser()
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--binaries', type=Path, required=True)
    parser.add_argument('--linux-daemon', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--image', default='rust:1.85.1-bookworm')
    args = parser.parse_args()
    for name in ('fixture', 'binaries', 'linux_daemon', 'output'):
        setattr(args, name, getattr(args, name).resolve())
    route.BIN = args.binaries
    args.output.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()

    def expired(_signal, _frame):
        raise TimeoutError('complete daemon startup functional selection exceeded 60 seconds')

    signal.signal(signal.SIGALRM, expired); signal.alarm(60)
    caller_dependencies = {}
    for module in tuple(sys.modules.values()):
        filename = getattr(module, '__file__', None)
        if filename:
            path = Path(filename).resolve()
            if path.is_relative_to(route.ROOT):
                caller_dependencies[str(path.relative_to(route.ROOT))] = sha(path)
    report = {'status': 'FAIL', 'mode': 'functional-daemon-retained-startup', 'case': 'held_init',
        'source': mount.checked(['git', 'rev-parse', 'HEAD'], text=True).stdout.strip(),
        'product_inputs_sha256': mount.product_inputs(), 'driver_sha256': sha(Path(__file__)),
        'mount_driver_sha256': sha(Path(mount.__file__)), 'route_driver_sha256': sha(Path(route.__file__)),
        'caller_dependencies_sha256': dict(sorted(caller_dependencies.items())),
        'linux_binary_sha256': sha(args.linux_daemon),
        'host_binary_sha256': {name: sha(route.BIN / name) for name in ('layerfs-server', 'examples/public_key')},
        'hard_budget_seconds': 60, 'performance_claim': False, 'cache_claim': None,
        'runtime_security_profile': 'SYS_ADMIN + SYS_PTRACE; AppArmor/seccomp unconfined; child installs declared writev USER_NOTIF filter',
        'not_run': ['constructor-error dependency cleanup proof', 'writable startup/control',
                    'hard syscall preemption', 'performance/RSS/cgroup qualification']}
    try:
        space = isolation.namespace()
        for path in (args.fixture, args.binaries, args.linux_daemon, args.output):
            space.assert_owned(path, 'startup proof input/output')
        work = isolation.concurrent_work()
        for item in work:
            item['command'] = re.sub(r'(?i)([a-z_]*(?:key|secret|token|password)[a-z_]*=)[^\s]+',
                                     r'\1<redacted>', item['command'])
        report['resource_isolation'] = space.as_fields() | {
            'artifact_root': str(args.output.parent), 'run_output': str(args.output),
            'build_target': str(route.ROOT / 'core/target'), 'linux_build_target': str(route.ROOT / 'core/target-linux'),
            'concurrent_work': work, 'observation': 'functional proof; no quiet-host/performance claim'}
        report['runtime_image_id'] = mount.checked(
            ['docker', 'image', 'inspect', '--format', '{{.Id}}', args.image], text=True).stdout.strip()
        with space.lock_path.open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            execute(args, report, route, mount)
        report['status'] = 'PASS'
    except BaseException as error:
        report['failure'] = repr(error)
        raise
    finally:
        signal.alarm(0)
        report['command_wall_seconds'] = time.monotonic() - started
        (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0


if __name__ == '__main__':
    sys.exit(main())
