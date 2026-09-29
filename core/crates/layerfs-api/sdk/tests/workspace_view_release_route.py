#!/usr/bin/env python3
"""One append-only live SDK uncertain-release proof; no product faults/hooks.

The external byte-transparent relay counts encrypted native records without
reading keys or plaintext. After the checked current-session Hello, the next
server record is fully received from the owned daemon but withheld from the
SDK client. The daemon's completion precedes this fault. The public SDK result
must be Unknown; a read-only status observes remote custody without retrying
release. Source and raw records are retained even on failure.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import threading
import time

SOURCE = Path(__file__).resolve()
WRAPPER = SOURCE.with_name('docker_release_relay.py')
ROOT = SOURCE.parents[5]
MAX_FRAME = 1 << 20


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def exact(sock, count):
    data = bytearray()
    while len(data) < count:
        next_bytes = sock.recv(count - len(data))
        if not next_bytes:
            raise EOFError('native relay stopped inside a framed record')
        data.extend(next_bytes)
    return bytes(data)


class Relay:
    def __init__(self, output):
        self.output = output
        self.listener = socket.socket()
        self.listener.bind(('127.0.0.1', 0))
        self.listener.listen(8)
        self.listener.settimeout(.4)
        self.control = socket.socket()
        self.control.bind(('127.0.0.1', 0))
        self.control.listen(8)
        self.control.settimeout(.4)
        self.lock = threading.Lock()
        self.stopped = threading.Event()
        self.next_id = 0
        self.connections = {}
        self.last = None
        self.target = None
        self.after_arm = 0
        self.actual = None
        self.container = None
        self.dropped = 0
        self.issues = []
        self.workers = []
        self.journal = output / 'relay.jsonl'
        self.journal.touch(exist_ok=False)

    def event(self, kind, **fields):
        with self.lock:
            with self.journal.open('a') as file:
                file.write(json.dumps({'event': kind, 'at_ns': time.monotonic_ns(), **fields}) + '\n')

    def controls(self):
        while not self.stopped.is_set():
            try:
                incoming, _ = self.control.accept()
            except socket.timeout:
                continue
            except OSError:
                if self.stopped.is_set():
                    break
                raise
            with incoming:
                try:
                    incoming.settimeout(6)
                    line = incoming.makefile('rb').readline(256).decode().strip()
                    if line.startswith('MAP '):
                        _, container, actual = line.split()
                        assert container.startswith('layerfs-')
                        assert actual.startswith('127.0.0.1:')
                        with self.lock:
                            assert self.container in (None, container), 'mapping moved'
                            assert self.actual in (None, actual), 'published port moved'
                            self.container, self.actual = container, actual
                        self.event('mapping', container=container, port=actual)
                        incoming.sendall(b'MAPPED\n')
                    elif line == 'ARM':
                        with self.lock:
                            assert self.target is None and self.last in self.connections
                            self.target = self.last
                            self.after_arm = 0
                            selected = self.target
                        self.event('armed', connection=selected)
                        incoming.sendall(b'ARMED\n')
                    else:
                        raise ValueError(f'unsupported control: {line!r}')
                except BaseException as error:
                    self.issues.append(repr(error))
                    incoming.sendall(b'ERROR\n')

    def accept(self):
        while not self.stopped.is_set():
            try:
                client, _ = self.listener.accept()
            except socket.timeout:
                continue
            except OSError:
                if self.stopped.is_set():
                    break
                raise
            with self.lock:
                self.next_id += 1
                number = self.next_id
                actual = self.actual
                self.connections[number] = client
            if actual is None:
                self.issues.append('connection without mapped owned daemon')
                client.close()
                continue
            self.event('connection', connection=number)
            worker = threading.Thread(target=self.exchange, args=(number, client, actual), daemon=True)
            self.workers.append(worker)
            worker.start()

    def exchange(self, number, client, actual):
        with client:
            try:
                with socket.create_connection(('127.0.0.1', int(actual.rsplit(':', 1)[1])), timeout=5) as service:
                    def upload():
                        try:
                            while data := client.recv(65536):
                                self.event('client_bytes', connection=number, bytes=len(data))
                                service.sendall(data)
                        except OSError:
                            pass
                        finally:
                            # Propagate EOF, not just bytes. The daemon admits
                            # one active control session; a stale half-open
                            # relay otherwise blocks the next checked mount.
                            try:
                                service.shutdown(socket.SHUT_WR)
                            except OSError:
                                pass
                    thread = threading.Thread(target=upload, daemon=True)
                    thread.start()
                    # Noise handshake and subsequent native records all use a
                    # 4-byte big-endian length. Never inspect decrypted frames.
                    while not self.stopped.is_set():
                        try:
                            header = exact(service, 4)
                            size = int.from_bytes(header, 'big')
                            assert 0 < size <= MAX_FRAME, size
                            ciphertext = exact(service, size)
                        except EOFError:
                            break
                        with self.lock:
                            self.last = number
                            if number == self.target:
                                self.after_arm += 1
                                ordinal = self.after_arm
                            else:
                                ordinal = 0
                        self.event('server_record', connection=number, ciphertext_bytes=size,
                                   after_arm_ordinal=ordinal)
                        if ordinal == 2:
                            digest = hashlib.sha256(header + ciphertext).hexdigest()
                            self.event('withheld_terminal_reply', connection=number,
                                       ciphertext_bytes=size, ciphertext_sha256=digest)
                            self.dropped += 1
                            # The server has emitted this result. End the client
                            # socket without delivering it; no fabricated reply.
                            break
                        client.sendall(header + ciphertext)
                    try:
                        client.shutdown(socket.SHUT_RDWR)
                    except OSError:
                        pass
                    try:
                        service.shutdown(socket.SHUT_RDWR)
                    except OSError:
                        pass
                    thread.join(timeout=1)
            except BaseException as error:
                self.issues.append(f'connection {number}: {error!r}')
            finally:
                with self.lock:
                    self.connections.pop(number, None)

    def start(self):
        for callback in (self.controls, self.accept):
            worker = threading.Thread(target=callback, daemon=True)
            worker.start()
            self.workers.append(worker)

    def stop(self):
        self.stopped.set()
        self.listener.close()
        self.control.close()
        for worker in self.workers:
            worker.join(timeout=1)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--image', required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(exist_ok=False, parents=True)
    assert ROOT.is_dir() and (ROOT / 'core/Cargo.toml').is_file()
    assert str(output).startswith(str((ROOT / 'core/target').resolve()) + os.sep)
    assert args.image.startswith('sha256:') and len(args.image) == 71
    sha_image = subprocess.check_output(['docker', 'image', 'inspect', args.image,
                                         '--format', '{{.Id}}'], text=True).strip()
    assert sha_image == args.image
    relay = Relay(output)
    relay.start()
    bin_dir = output / 'bin'
    bin_dir.mkdir()
    wrapper = bin_dir / 'docker'
    shutil.copyfile(WRAPPER, wrapper)
    wrapper.chmod(0o555)
    env = os.environ.copy()
    env.update(PATH=str(bin_dir) + os.pathsep + env['PATH'],
               LAYERFS_REAL_DOCKER=shutil.which('docker'),
               LAYERFS_RELAY_CONTROL_PORT=str(relay.control.getsockname()[1]),
               LAYERFS_RELAY_ENDPOINT='127.0.0.1:' + str(relay.listener.getsockname()[1]),
               LAYERFS_RELEASE_RELAY_CONTROL='127.0.0.1:' + str(relay.control.getsockname()[1]),
               LAYERFS_TEST_IMAGE=args.image, LAYERFS_CONSTRUCTION_WORKERS='1')
    cmd = ['cargo', '+1.85.1', 'test', '--manifest-path', 'core/Cargo.toml', '--locked',
           '-p', 'layerfs-sdk', '--test', 'workspace_view', '--', '--exact',
           'view_lease_uncertain_release_does_not_retry_or_claim_completion',
           '--nocapture', '--test-threads=1']
    dirty = subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT,
                                    text=True).splitlines()
    mode = 'diagnostic' if '-diagnostic-' in output.name else 'functional'
    if mode != 'diagnostic':
        assert not dirty, 'functional proof requires a clean source seal'
    record = {'status': 'FAIL', 'mode': mode, 'source_dirty': dirty,
              'source': subprocess.check_output(['git', 'rev-parse', 'HEAD'],
                   cwd=ROOT, text=True).strip(), 'image_id': args.image,
              'driver_sha256': sha(SOURCE), 'wrapper_sha256': sha(WRAPPER),
              'test_source_sha256': sha(SOURCE.with_name('workspace_view.rs')),
              'command': cmd, 'performance_claim': False, 'scope': 'one live public SDK release'}
    started = time.monotonic()
    try:
        try:
            child = subprocess.run(cmd, cwd=ROOT, env=env, capture_output=True, timeout=85)
            (output / 'test.stdout').write_bytes(child.stdout)
            (output / 'test.stderr').write_bytes(child.stderr)
            record['test_exit'] = child.returncode
        except subprocess.TimeoutExpired as error:
            (output / 'test.stdout').write_bytes(error.stdout or b'')
            (output / 'test.stderr').write_bytes(error.stderr or b'')
            record['timeout'] = True
        success = (record.get('test_exit') == 0 and relay.dropped == 1
                   and b'VIEW_LEASE_RELEASE_LOSS' in (output / 'test.stdout').read_bytes()
                   and not relay.issues)
        record['status'] = ('DIAGNOSTIC_PASS' if success and mode == 'diagnostic'
                            else 'PASS' if success else 'FAIL')
    finally:
        relay.stop()
        record.update(command_wall_seconds=time.monotonic() - started,
                      mapping={'container': relay.container, 'actual': relay.actual},
                      connection_count=relay.next_id, withheld_reply_count=relay.dropped,
                      relay_issues=relay.issues)
        (output / 'result.json').write_text(json.dumps(record, indent=2) + '\n')
        (output / 'SHA256SUMS').write_text(''.join(
            hashlib.sha256(p.read_bytes()).hexdigest() + '  ' + str(p.relative_to(output)) + '\n'
            for p in sorted(output.rglob('*')) if p.is_file() and p.name != 'SHA256SUMS'))
        print(json.dumps({'status': record['status'], 'source': record['source'],
                          'withheld_reply_count': relay.dropped, 'output': str(output)}))


if __name__ == '__main__':
    main()
