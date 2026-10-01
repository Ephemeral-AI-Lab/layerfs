"""Source-pinned standalone MinIO setup/performance/proof; one run per case."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import signal
import socket
import subprocess
import time
import urllib.request

from common import BLOCK, S3, body_hash, phase, rss, write_json

CASES = {
    'M-directory-10000-v1': (10000, 0, 'directory/'),
    'M-big-64m-v1': (1, 64 * 1048576, 'big/'),
    'M-tiny-1024-v1': (1024, 1024, 'tiny/'),
    'M-grouped-1024-v1': (4, 256 * 1024, 'grouped/'),
}


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def start(root):
    root = Path(root).resolve()
    root.mkdir(parents=True, exist_ok=True)
    config_path = root / 'private-config.json'
    if config_path.exists():
        raise RuntimeError('existing provider owner; no replacement/restart')
    port, console = free_port(), free_port()
    while console == port:
        console = free_port()
    config = {'port': port, 'console': console, 'access': 'probe-' + secrets.token_hex(8),
              'secret': secrets.token_hex(32), 'bucket': 'issue290-' + secrets.token_hex(8)}
    env = {**os.environ, 'MINIO_ROOT_USER': config['access'],
           'MINIO_ROOT_PASSWORD': config['secret'], 'MINIO_COMPRESSION_ENABLE': 'off',
           'MINIO_BROWSER': 'off', 'MINIO_UPDATE': 'off'}
    log_path = root / 'startup-private.log'
    fd = os.open(log_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, 'wb') as log:
        process = subprocess.Popen([str(root / 'minio'), 'server', str(root / 'data'),
                                    '--address', f'127.0.0.1:{port}',
                                    '--console-address', f'127.0.0.1:{console}'],
                                   env=env, stdout=log, stderr=subprocess.STDOUT,
                                   start_new_session=True)
    config['pid'] = process.pid
    fd = os.open(config_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, 'w') as out:
        json.dump(config, out)
    deadline = time.monotonic() + 15
    while True:
        if process.poll() is not None:
            raise RuntimeError('owned MinIO exited; inspect private startup log')
        try:
            with urllib.request.urlopen(f'http://127.0.0.1:{port}/minio/health/ready', timeout=1) as r:
                if r.status == 200:
                    break
        except OSError:
            if time.monotonic() > deadline:
                raise RuntimeError('provider readiness timeout; owner retained')
            time.sleep(.1)
    client = S3(config)
    client.call('PUT')
    # Separate, untimed transport qualification on a distinct key.
    digest = body_hash(128)
    client.call('PUT', 'qualification', size=128, payload_hash=digest)
    assert client.call('GET', 'qualification', consume=True, verify_hash=True) == (128, digest)
    client.call('DELETE', 'qualification')
    client.close()
    write_json(root / 'startup.json', {'pid': process.pid, 'port': port,
                                      'console_enabled': False, 'compression': 'off',
                                      'transport_qualification': 'PASS'})
    print('owned local MinIO ready; transport qualification PASS')


def execute(action, case, root, output):
    root = Path(root)
    output = Path(output) if output else root
    config = json.loads((root / 'private-config.json').read_text())
    count, size, prefix = CASES[case]
    client = S3(config)
    expected = [prefix + f'{i:08d}' for i in range(count)]
    block = BLOCK[:1024] * 64 if 'grouped' in case else BLOCK
    digest = body_hash(size, block)
    if action == 'setup':
        seal = root / 'directory-master.json'
        if seal.exists():
            raise RuntimeError('setup master already exists; do not regenerate')
        for key in expected:
            client.call('PUT', key)
        keys, pages = client.listing(prefix)
        assert keys == expected
        write_json(seal, {'count': count, 'expected_keys_sha256':
                         hashlib.sha256('\n'.join(expected).encode()).hexdigest(),
                         'pages': pages, 'qualification': 'PASS'})
        print('directory master created once; exact list qualification PASS')
    elif action == 'perf':
        output.mkdir(parents=True, exist_ok=False)
        record = {'case': case, 'cache_verdict': 'INELIGIBLE', 'performance_claim': False,
                  'cache_contract': 'server OS/page residency UNKNOWN; GET follows PUT',
                  'input': 'seeded synthetic memory, streamed in64KiB blocks',
                  'objects': count, 'object_bytes': size,
                  'logical_files': 1024 if 'grouped' in case else count,
                  'minio_rss_before': rss(config['pid'])}
        if size:
            phase(record, 'put', count, count * size,
                  lambda: [client.call('PUT', k, size=size, payload_hash=digest, block=block) for k in expected])
            received = phase(record, 'get', count, count * size,
                             lambda: sum(client.call('GET', k, consume=True)[0] for k in expected))
            assert received == count * size
        keys, pages = phase(record, 'list', count, 0, lambda: client.listing(prefix))
        assert keys == expected
        record['list_pages'] = pages
        record['requests'] = client.calls
        record['sent_bytes'] = client.sent
        record['received_bytes'] = client.received
        record['minio_rss_after'] = rss(config['pid'])
        record['command_status'] = 'COMPLETE'
        write_json(output / 'performance.json', record)
        print(case, 'performance complete; numerical cache INELIGIBLE')
    elif action == 'verify':
        start_time = time.monotonic_ns()
        keys, pages = client.listing(prefix)
        assert keys == expected
        for key in expected if size else expected[:1]:
            assert client.call('GET', key, consume=True, verify_hash=True) == (size, digest)
        proof_ns = time.monotonic_ns() - start_time
        cleanup_start = time.monotonic_ns()
        if size:
            for key in expected:
                client.call('DELETE', key)
            assert client.listing(prefix)[0] == []
        write_json(output / 'verification.json', {'case': case, 'proof': 'PASS',
                   'body_sha256': digest, 'proof_ns': proof_ns,
                   'cleanup_ns': time.monotonic_ns() - cleanup_start,
                   'cleanup': 'PASS' if size else 'MASTER_RETAINED_FOR_OWNED_FINAL_CLEANUP',
                   'requests': client.calls})
        print(case, 'independent body/list proof PASS')
    client.close()


def stop(root):
    root = Path(root)
    config = json.loads((root / 'private-config.json').read_text())
    client = S3(config)
    keys, _ = client.listing('directory/')
    for key in keys:
        client.call('DELETE', key)
    assert client.listing('directory/')[0] == []
    client.close()
    pid = config['pid']
    command = subprocess.check_output(['ps', '-o', 'command=', '-p', str(pid)], text=True).strip()
    if not command.startswith(str(root.resolve() / 'minio') + ' server '):
        raise RuntimeError('recorded PID no longer matches the owned binary; no termination')
    os.kill(pid, signal.SIGTERM)
    deadline = time.monotonic() + 5
    exited = False
    while time.monotonic() < deadline:
        state = subprocess.run(['ps', '-o', 'state=', '-p', str(pid)],
                               capture_output=True, text=True).stdout.strip()
        if not state or state.startswith('Z'):
            exited = True
            break
        time.sleep(.1)
    write_json(root / 'shutdown.json', {'owned_pid': pid, 'master_cleanup': 'PASS',
                                     'termination_requested': True, 'process_exited': exited})
    if not exited:
        raise RuntimeError('owned MinIO exit incomplete after 5s; no escalation/retry')
    print('owned master removed; owned MinIO exited')


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('action', choices=['start', 'setup', 'perf', 'verify', 'stop'])
    p.add_argument('--root', required=True)
    p.add_argument('--case', choices=CASES)
    p.add_argument('--output')
    a = p.parse_args()
    if a.action == 'start':
        start(a.root)
    elif a.action == 'stop':
        stop(a.root)
    else:
        execute(a.action, a.case, a.root, a.output)
