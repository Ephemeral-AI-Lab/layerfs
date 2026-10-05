#!/usr/bin/env python3
"""Actual failed CLI Attach retains its owner before control startup until explicit SIGTERM."""
import json
import os
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import threading
import time
import uuid

import control_unmount as driver

driver.MODE = 'functional-daemon-attach-startup-failure'
driver.ENTRY_SOURCE = Path(__file__)
driver.CASES = {'attach_startup_failure': [
    'failed-CLI-Attach-retains-owner-without-ready-control-until-explicit-signal-preserving-original-error']}
driver.NOT_RUN = ['writable startup', 'startup failure with acquired mount/backing/arena resources',
                  'namespace/npm/R6', 'hard RSS/cgroup/performance qualification']


def deadline_result_proxy(port):
    """Partial authentic third frame makes progress without completing its terminal."""
    listener = socket.socket(); listener.bind(('0.0.0.0', 0)); listener.listen(1); listener.settimeout(5)
    address = listener.getsockname()[1]; result = {}; uploads = []
    def exact(stream, count):
        data = bytearray()
        while len(data) < count:
            part = stream.recv(count - len(data))
            if not part: raise EOFError('native frame ended early')
            data.extend(part)
        return bytes(data)
    def relay():
        try:
            with listener.accept()[0] as client, socket.create_connection(('127.0.0.1', port), timeout=5) as service:
                client.settimeout(12)
                closed = threading.Event()
                def upload():
                    try:
                        while chunk := client.recv(16384): service.sendall(chunk)
                        result['client_eof'] = True
                    except OSError as error:
                        result['upload_error'] = repr(error)
                    finally: closed.set()
                worker = threading.Thread(target=upload, daemon=True); worker.start(); uploads.append(worker)
                lengths = []
                for index in range(3):
                    header = exact(service, 4); length = struct.unpack('>I', header)[0]; assert length <= 32804
                    encrypted = exact(service, length); lengths.append(length)
                    if index < 2: client.sendall(header + encrypted)
                    else: terminal_header, terminal = header, encrypted
                result['server_frame_lengths'] = lengths
                started = time.monotonic()
                result['terminal_started_monotonic_ns'] = time.monotonic_ns()
                result['terminal_ciphertext_bytes'] = len(terminal)
                result['ciphertext_chunks'] = []
                client.sendall(terminal_header)
                result['terminal_header_forwarded'] = True
                offset = 0
                chunk_bytes = min(16, (len(terminal) - 1) // 4)
                assert chunk_bytes > 0
                for _ in range(4):
                    if closed.wait(2): break
                    client.sendall(terminal[offset:offset + chunk_bytes])
                    result['ciphertext_chunks'].append({'offset': offset, 'bytes': chunk_bytes,
                        'monotonic_ns': time.monotonic_ns(), 'seconds_after_terminal': time.monotonic() - started})
                    offset += chunk_bytes
                result['terminal_prefix_bytes_forwarded'] = offset
                result['withheld_ciphertext_bytes'] = len(terminal) - offset
                assert result['withheld_ciphertext_bytes'] > 0
                assert closed.wait(max(0, 12 - (time.monotonic() - started))), 'client did not close at its original operation deadline'
                assert result.get('client_eof') and 'upload_error' not in result, result
                result['terminal_hold_until_client_eof_seconds'] = time.monotonic() - started
                worker.join(3); assert not worker.is_alive()
                result['upload_worker_joined'] = True
        except BaseException as error: result['error'] = repr(error)
        finally: listener.close()
    controller = threading.Thread(target=relay, daemon=True); controller.start()
    return address, controller, uploads, result


def execute(args, report):
    fixture = json.loads(args.fixture.read_text())
    assert fixture['status'] == 'PASS' and fixture['mode'] == 'functional-mounted-proof'
    service_dir = args.output / 'service'; service_dir.mkdir()
    seals = {}
    for filename in ('store.sqlite', 'history.sqlite'):
        source = args.fixture.parent / 'service' / filename
        target = service_dir / filename
        seals[filename] = driver.sha(source); shutil.copyfile(source, target)
        assert driver.sha(target) == seals[filename]
    report['fixture_reuse'] = {'receipt': str(args.fixture), 'receipt_sha256': driver.sha(args.fixture),
        'clone_method': 'independent-byte-copy', 'closed_master_sha256': seals,
        'history': 'read-only reopened fixture; no write authority', 'cache_claim': None}
    service_key, daemon_key, control_key = (os.urandom(32).hex() for _ in range(3))
    service_public, daemon_public, control_public = (
        driver.route.public_key(key) for key in (service_key, daemon_key, control_key))
    environment = os.environ.copy()
    environment.update(LAYERFS_PRIVATE_KEY=service_key,
        LAYERFS_PEERS=f'1,{daemon_public},{int(time.time()) + 3600},255',
        LAYERFS_STORE=str(service_dir / 'store.sqlite'), LAYERFS_LISTEN='0.0.0.0:0',
        LAYERFS_TELEMETRY='off', LAYERFS_HISTORY_CATALOG=str(service_dir / 'history.sqlite'),
        LAYERFS_HISTORY_BINDING='pair1-mounted-read', LAYERFS_HISTORY_CREATE='0',
        LAYERFS_HISTORY_CURSOR_KEY=os.urandom(32).hex(), LAYERFS_CONSTRUCTION_WORKERS='1')
    service = subprocess.Popen([driver.route.BIN / 'layerfs-server'], env=environment,
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    name = 'layerfs-attach-startup-' + uuid.uuid4().hex[:12]
    daemon = None
    volume = container = False
    proxy = None
    service_ready = diagnostics = ''
    try:
        service_ready = driver.mount.line_until(service); assert 'ready' in service_ready, service_ready
        port = int(service_ready.strip().rsplit(':', 1)[1])
        proxy = deadline_result_proxy(port)
        driver.mount.checked(['docker', 'volume', 'create', name + '-root']); volume = True
        environment = os.environ.copy()
        environment.update(LAYERFS_ENDPOINT=f'host.docker.internal:{proxy[0]}', LAYERFS_SELECTOR='1',
            LAYERFS_PRIVATE_KEY=daemon_key, LAYERFS_SERVER_KEY=service_public, LAYERFS_TELEMETRY='off',
            LAYERFS_WORKSPACE_ROOT='/layerfs', LAYERFS_WORKSPACE_MAX_COUNT='2',
            LAYERFS_CONTROL_LISTEN='0.0.0.0:23456',
            LAYERFS_CONTROL_PEERS=f'1,{control_public},{int(time.time()) + 3600},31',
            LAYERFS_CONSTRUCTION_WORKERS='1')
        assert service.poll() is None
        report['service_boundary'] = {'pid': service.pid, 'selected_endpoint': environment['LAYERFS_ENDPOINT'],
            'upstream_endpoint': f'127.0.0.1:{port}',
            'scope': 'successful native handshake and HELLO; actual third frame header and bounded ciphertext prefixes progress every2s while its suffix remains withheld until caller EOF at unchanged10-second absolute deadline'}
        daemon = driver.mount.mount_process(args, environment, fixture['root'], name); container = True
        report['runtime_nano_cpus'] = int(driver.mount.checked(
            ['docker', 'inspect', '--format', '{{.HostConfig.NanoCpus}}', name], text=True).stdout.strip())
        assert report['runtime_nano_cpus'] == 2_000_000_000
        report['kernel'] = driver.mount.checked(['docker', 'exec', name, 'uname', '-srmo'], text=True).stdout.strip()
        original = driver.mount.line_until(daemon, timeout=12)
        (args.output / 'daemon.stderr').write_text(original)
        diagnostics += original
        retained = driver.mount.line_until(daemon, timeout=3)
        with (args.output / 'daemon.stderr').open('a') as log: log.write(retained)
        diagnostics += retained
        assert original.startswith('workspace attach startup retained: '), original
        cause = original.removeprefix('workspace attach startup retained: ').strip()
        assert retained.startswith('workspace startup cleanup retained: ') and 'Deadline' in retained, retained
        assert daemon.poll() is None
        proxy[1].join(3)
        for worker in proxy[2]: worker.join(3)
        assert not proxy[1].is_alive() and all(not worker.is_alive() for worker in proxy[2])
        assert 'error' not in proxy[3] and proxy[3].get('client_eof') and proxy[3].get('upload_worker_joined'), proxy[3]
        report['native_result_delay'] = proxy[3]
        driver.mount.checked(['docker', 'exec', name, 'python3', '-c',
            "from pathlib import Path; "
            "rows=(Path('/proc/net/tcp').read_text()+Path('/proc/net/tcp6').read_text()).splitlines(); "
            "assert not any(len(r.split())>3 and r.split()[3]=='0A' and r.split()[1].endswith(':5BA0') for r in rows); "
            "assert not Path('/layerfs/workspace/read').exists()"])
        report['retained_startup'] = {'original_failure': cause, 'cleanup_diagnostic': retained.strip(),
            'control_listener_closed': True, 'daemon_retained_before_signal': True, 'original_attach_seconds': 10}
        driver.mount.stop_mount(name)
        assert daemon.wait(timeout=12) == 1
        errors = daemon.stderr.read(); diagnostics += errors.decode(errors='replace')
        with (args.output / 'daemon.stderr').open('ab') as log: log.write(errors)
        assert 'workspace failed startup cleaned\n' in diagnostics, diagnostics
        assert 'Error: ' + cause in diagnostics, diagnostics
        assert 'workspace ready ' not in diagnostics and 'workspace control ready ' not in diagnostics, diagnostics
        driver.mount.checked(['docker', 'exec', name, 'test', '!', '-e', '/layerfs/workspace/read'])
        report['explicit_cleanup'] = {'signal': 'SIGTERM', 'native_result_withheld_until_client_close': True,
            'exit_code': daemon.returncode, 'original_error_preserved': True}
        report['checks'] = [{'id': key, 'status': 'PASS'} for key in driver.CASES[args.case]]
    finally:
        if container:
            if daemon is not None and daemon.poll() is None: report['forced_daemon_cleanup'] = True
            driver.mount.checked(['docker', 'rm', '-f', name])
        if daemon is not None:
            daemon.wait(timeout=10)
            path = args.output / ('daemon.stderr' if (args.output / 'daemon.stderr').exists() else 'failed-daemon.stderr')
            with path.open('ab') as log: log.write(daemon.stderr.read())
        if service.poll() is None:
            service.stdin.close(); service.wait(timeout=10)
        (args.output / 'service.stderr').write_bytes(service_ready.encode() + service.stderr.read())
        if volume: driver.mount.checked(['docker', 'volume', 'rm', name + '-root'])
        if proxy:
            proxy[1].join(3)
            for worker in proxy[2]: worker.join(3)
            report['native_result_delay'] = proxy[3]
    assert service.returncode == 0
    for filename, expected in seals.items():
        assert driver.sha(args.fixture.parent / 'service' / filename) == expected
        assert driver.sha(service_dir / filename) == expected
    report['remote_files_unchanged'] = True
    report['cleanup'] = 'PASS: explicit signal cleaned retained failed Attach, daemon exited original error, owned runtime/volume removed'


driver.execute = execute

if __name__ == '__main__':
    driver.main()
