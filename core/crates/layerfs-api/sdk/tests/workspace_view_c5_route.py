#!/usr/bin/env python3
"""External SDK C1-success/local-C5-fault route; append-only local receipt.

The relay forwards authenticated service records verbatim. For an armed Commit
only, it tests the read-only history head *after* receiving a host ciphertext
record and gates that record until an external per-container fault is applied.
No service or daemon keys are intercepted, and no canonical reply is forged.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import sqlite3
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parents[5]
SOURCE = Path(__file__).resolve()
WRAPPER = SOURCE.with_name('docker_c5_fault.py')
BRANCH = bytes([0x11]) + bytes([37]) * 16


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def exact(sock, count):
    result = bytearray()
    while len(result) < count:
        part = sock.recv(count - len(result))
        if not part:
            raise EOFError('short encrypted frame')
        result.extend(part)
    return bytes(result)


class Relay:
    def __init__(self, host, output):
        self.host = host
        self.output = output
        self.service = socket.socket()
        self.service.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.control = socket.socket()
        self.control.bind(('127.0.0.1', 0))
        self.control.listen(8)
        self.control.settimeout(.25)
        self.ready = threading.Event()
        self.go = threading.Event()
        self.stop_requested = threading.Event()
        self.armed = False
        self.lock = threading.Lock()
        self.gates = 0
        self.head_hex = None
        self.errors = []
        self.connections = 0
        self.history = None
        self.journal = output / 'relay.jsonl'
        self.journal.touch(exist_ok=False)

    def event(self, event, **facts):
        with self.lock:
            with self.journal.open('a') as file:
                file.write(json.dumps({'event': event, 'at_ns': time.monotonic_ns(), **facts}) + '\n')

    def head(self):
        if self.history is None or not self.history.exists():
            return None
        with sqlite3.connect(f'file:{self.history}?mode=ro', uri=True, timeout=.25) as db:
            row = db.execute('SELECT head_commit_id FROM branches WHERE branch_id=?',
                             (BRANCH,)).fetchone()
            return row[0].hex() if row and row[0] else None

    def controls(self):
        while not self.stop_requested.is_set():
            try:
                client, _ = self.control.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            with client:
                client.settimeout(12)
                try:
                    line = client.makefile('rb').readline(512).decode().strip()
                    if line.startswith('BIND '):
                        port = int(line[5:])
                        assert port > 0 and not self.port
                        self.port = port
                        self.service.bind((self.host, port))
                        self.event('bound', port=port, host=self.host)
                        thread = threading.Thread(target=self.accept, daemon=True)
                        thread.start()
                        self.workers.append(thread)
                        client.sendall(b'BOUND\n')
                    elif line.startswith('ARM '):
                        history = Path(line[4:]).resolve()
                        assert history.name == 'history.sqlite'
                        assert history.parent.name.startswith('layerfs-view-c5-')
                        self.history = history
                        assert self.head() is None, 'head already advanced before gate'
                        self.armed = True
                        self.event('armed', history=str(history))
                        client.sendall(b'ARMED\n')
                    elif line == 'WAIT':
                        assert self.armed
                        if not self.ready.wait(10):
                            raise TimeoutError('canonical head and host reply gate not reached')
                        client.sendall(f'GATED {self.head_hex}\n'.encode())
                    elif line == 'GO':
                        assert self.ready.is_set()
                        self.go.set()
                        self.event('released', gates=self.gates)
                        client.sendall(b'RELEASED\n')
                    else:
                        raise ValueError('unknown control operation')
                except BaseException as error:
                    self.errors.append(repr(error))
                    client.sendall(b'ERROR\n')

    def accept(self):
        assert self.port > 0
        self.service.listen(8)
        self.service.settimeout(.25)
        while not self.stop_requested.is_set():
            try:
                client, _ = self.service.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            self.connections += 1
            worker = threading.Thread(target=self.exchange, args=(self.connections, client), daemon=True)
            worker.start()
            self.workers.append(worker)

    def exchange(self, number, client):
        try:
            with client, socket.create_connection(('127.0.0.1', self.port), timeout=3) as server:
                server.settimeout(None)
                def upstream():
                    try:
                        while data := client.recv(65536):
                            server.sendall(data)
                    except OSError:
                        pass
                    finally:
                        try:
                            server.shutdown(socket.SHUT_WR)
                        except OSError:
                            pass
                worker = threading.Thread(target=upstream, daemon=True)
                worker.start()
                while not self.stop_requested.is_set():
                    try:
                        header = exact(server, 4)
                        length = int.from_bytes(header, 'big')
                        assert 0 < length <= 1 << 20, length
                        body = exact(server, length)
                    except EOFError:
                        break
                    if self.armed and not self.ready.is_set():
                        head = self.head()
                        if head is not None:
                            self.gates += 1
                            self.head_hex = head
                            self.event('canonical_head_reply_gated', connection=number,
                                       head=head, ciphertext_sha256=hashlib.sha256(header + body).hexdigest())
                            self.ready.set()
                            assert self.go.wait(10), 'fault was not applied in time'
                    client.sendall(header + body)
                worker.join(timeout=1)
        except BaseException as error:
            self.errors.append(f'{number}: {error!r}')

    def start(self, port):
        self.port = port
        self.workers = []
        for callback in (self.controls, self.accept):
            thread = threading.Thread(target=callback, daemon=True)
            thread.start()
            self.workers.append(thread)

    def stop(self):
        self.stop_requested.set()
        self.go.set()
        self.control.close()
        self.service.close()
        for worker in self.workers:
            worker.join(timeout=1)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--image', required=True)
    parser.add_argument('--host-ip', required=True)
    parser.add_argument('--limit-helper', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    assert str(output).startswith(str((ROOT / 'core/target').resolve()) + os.sep)
    real_docker = shutil.which('docker')
    assert real_docker
    assert subprocess.check_output([real_docker, 'image', 'inspect', args.image,
                                    '--format', '{{.Id}}'], text=True).strip() == args.image
    # No external socket is opened until the SDK server owns its exact port.
    relay = Relay(args.host_ip, output)
    bin_dir = output / 'bin'
    bin_dir.mkdir()
    wrapper = bin_dir / 'docker'
    shutil.copyfile(WRAPPER, wrapper)
    wrapper.chmod(0o555)
    helper = args.limit_helper.resolve()
    assert helper.is_file()
    env = os.environ.copy()
    env.update(PATH=str(bin_dir) + os.pathsep + env['PATH'], LAYERFS_REAL_DOCKER=real_docker,
               LAYERFS_C5_CONTROL=f'127.0.0.1:{relay.control.getsockname()[1]}',
               LAYERFS_C5_HOST_IP=args.host_ip, LAYERFS_C5_HELPER=str(helper),
               LAYERFS_TEST_IMAGE=args.image, LAYERFS_CONSTRUCTION_WORKERS='1')
    # Bind service host to the same port as the real loopback acceptor after
    # Server::listen: the Rust test tells this relay its bound port via BIND.
    command = ['cargo', '+1.85.1', 'test', '--manifest-path', 'core/Cargo.toml', '--locked',
               '-p', 'layerfs-sdk', '--test', 'workspace_view', '--', '--exact',
               'view_lease_known_c1_local_c5_failure', '--nocapture', '--test-threads=1']
    dirty = subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).splitlines()
    mode = 'diagnostic' if '-diagnostic-' in output.name else 'functional'
    if mode == 'functional':
        assert not dirty, 'clean source required'
    result = {'status': 'FAIL', 'mode': mode, 'source': subprocess.check_output(
        ['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(), 'dirty': dirty,
        'driver_sha256': sha(SOURCE), 'wrapper_sha256': sha(WRAPPER),
        'test_sha256': sha(SOURCE.with_name('workspace_view.rs')),
        'helper_sha256': sha(helper), 'image_id': args.image, 'command': command,
        'performance_claim': False, 'cache_claim': None}
    relay.port = 0
    relay.workers = []
    thread = threading.Thread(target=relay.controls, daemon=True)
    thread.start()
    relay.workers.append(thread)
    began = time.monotonic()
    try:
        try:
            run = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, timeout=85)
            (output / 'test.stdout').write_bytes(run.stdout)
            (output / 'test.stderr').write_bytes(run.stderr)
            result['test_exit'] = run.returncode
        except subprocess.TimeoutExpired as error:
            (output / 'test.stdout').write_bytes(error.stdout or b'')
            (output / 'test.stderr').write_bytes(error.stderr or b'')
            result['timeout'] = True
        passed = result.get('test_exit') == 0 and relay.gates == 1 and not relay.errors and (
            b'VIEW_LEASE_C5' in (output / 'test.stdout').read_bytes())
        result['status'] = ('DIAGNOSTIC_PASS' if passed and mode == 'diagnostic'
                            else 'PASS' if passed else 'FAIL')
    finally:
        relay.stop()
        result.update(wall_seconds=time.monotonic() - began,
                      canonical_reply_gates=relay.gates, canonical_head=relay.head_hex,
                      connections=relay.connections,
                      relay_errors=relay.errors)
        (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        (output / 'SHA256SUMS').write_text(''.join(
            sha(p) + '  ' + str(p.relative_to(output)) + '\n'
            for p in sorted(output.rglob('*')) if p.is_file() and p.name != 'SHA256SUMS'))
        print(json.dumps({'status': result['status'], 'gates': relay.gates, 'output': str(output)}))


if __name__ == '__main__':
    main()
