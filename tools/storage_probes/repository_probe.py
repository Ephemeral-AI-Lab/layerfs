"""Full closed repository backend import; one construction and transfer attempt."""
import argparse
from concurrent.futures import ThreadPoolExecutor, wait, FIRST_COMPLETED
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import threading
import time

from common import S3, write_json
from minio_probe import start, stop
from run import inventory
from wal_probe import connect, provider_identity, settings

GIB = 1024 ** 3
PACK_LIMIT = 256 * 1024


def sha256_file(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as f:
        for chunk in iter(lambda: f.read(65536), b''):
            digest.update(chunk)
    return digest.hexdigest()


def deadline_command(argv, log, seconds, env=None):
    started = time.monotonic_ns()
    with Path(log).open('xb') as stream:
        try:
            result = subprocess.run(argv, stdout=stream, stderr=subprocess.STDOUT,
                                    timeout=seconds, env=env)
            status, code = ('COMPLETE' if result.returncode == 0 else 'FAIL'), result.returncode
        except subprocess.TimeoutExpired:
            status, code = 'TIMEOUT', None
    return {'command': argv, 'status': status, 'exit': code,
            'wall_ns': time.monotonic_ns() - started, 'limit_seconds': seconds}


def transfer(args, download=False):
    output, stage = Path(args.output), Path(args.stage)
    config = json.loads((Path(args.minio) / 'private-config.json').read_text())
    # Each of four workers exclusively owns one HTTP connection. No retries.
    local = threading.local()
    clients, clients_lock = [], threading.Lock()
    def client():
        if not hasattr(local, 'client'):
            local.client = S3(config)
            with clients_lock: clients.append(local.client)
        return local.client
    target = output / 'downloaded-packs'
    if download: target.mkdir(exist_ok=False)
    db = sqlite3.connect(stage / 'catalog.sqlite', isolation_level=None, timeout=0)
    db.execute('PRAGMA cache_size=-512'); db.execute('PRAGMA mmap_size=0')
    query = db.execute('SELECT id,size,blake3 FROM packs ORDER BY id')
    lock = threading.Lock()
    active = maximum_active = 0
    def operation(row):
        nonlocal active, maximum_active
        number, size, _ = row
        assert 0 < size <= PACK_LIMIT
        name = f'pack-{number:08d}.bin'
        before = time.monotonic_ns()
        # Hashing real prepared body is paid by this transfer phase.
        original = (stage / 'packs' / name).read_bytes()
        assert len(original) == size
        digest = hashlib.sha256(original).hexdigest()
        def body_started():
            nonlocal active, maximum_active
            with lock:
                active += 1; maximum_active = max(maximum_active, active)
        if download:
            received = client().call('GET', 'packs/' + name, response_limit=PACK_LIMIT)
            assert received == original
            (target / name).write_bytes(received)
        else:
            client().call('PUT', 'packs/' + name, size=size, payload_hash=digest,
                          data=original, body_gate=body_started)
            with lock: active -= 1
        return {'id': number, 'bytes': size, 'sha256': digest,
                'elapsed_ns': time.monotonic_ns() - before, 'ack': True}
    started = time.monotonic_ns()
    count = total = 0
    try:
        with ThreadPoolExecutor(max_workers=4) as executor, (output / ('download-acks.jsonl' if download else 'upload-acks.jsonl')).open('x') as ledger:
            pending = set()
            exhausted = False
            while pending or not exhausted:
                while not exhausted and len(pending) < 8:
                    row = query.fetchone()
                    if row is None: exhausted = True; break
                    pending.add(executor.submit(operation, row))
                if not pending: break
                done, pending = wait(pending, return_when=FIRST_COMPLETED)
                for future in done:
                    row = future.result()
                    count += 1; total += row['bytes']
                    assert total <= 4 * GIB
                    ledger.write(json.dumps(row) + '\n'); ledger.flush()
                if count % 128 < len(done):
                    write_json(output / ('download-progress.json' if download else 'upload-progress.json'),
                               {'packs_acknowledged': count, 'bytes': total, 'complete': False})
        transfer_ns = time.monotonic_ns() - started
        expected = db.execute('SELECT count(*),sum(size) FROM packs').fetchone()
        assert (count, total) == expected
    finally:
        db.close()
        for instance in clients: instance.close()
    return {'packs': count, 'bytes': total, 'transfer_ns': transfer_ns,
            'connections': len(clients), 'max_outstanding': 8,
            'max_body_started_before_ack': maximum_active,
            'overlap_scope': 'client request bodies sent before prior ACK; no provider CPU overlap claim'}


def perf(args):
    output = Path(args.output)
    started = time.monotonic_ns()
    provider = provider_identity(args.sqlite_provider)
    result = transfer(args)
    publication_started = time.monotonic_ns()
    db = connect(Path(args.stage) / 'catalog.sqlite', 'wal-fullfsync')
    profile = settings(db)
    assert db.execute('SELECT ready FROM publication').fetchone() == (0,)
    db.execute('BEGIN IMMEDIATE')
    assert db.execute('UPDATE publication SET ready=1 WHERE ready=0').rowcount == 1
    db.execute('COMMIT')
    checkpoint = db.execute('PRAGMA wal_checkpoint(TRUNCATE)').fetchone()
    assert checkpoint[0] == 0
    db.close()
    result.update({'publication_ns': time.monotonic_ns() - publication_started,
                   'wall_ns': time.monotonic_ns() - started, 'checkpoint': checkpoint,
                   'publication': 'READY', 'provider': provider, 'settings': profile,
                   'cache_verdict': 'INELIGIBLE', 'performance_claim': False,
                   'cache_contract': 'closed independent fixture; source/pack/server OS residency unobserved',
                   'profile': 'standalone prepared backend upload; not SDK Init/Commit'})
    write_json(output / 'performance.json', result)


def verify(args):
    provider_identity(args.sqlite_provider)
    result = transfer(args, download=True)
    write_json(Path(args.output) / 'download.json', result)
    # Replace this process: the runner's single 10s watchdog covers download+decode.
    os.execv(args.binary, [args.binary, 'verify', args.master, args.stage,
                          str(Path(args.output) / 'downloaded-packs')])


def run(args):
    output = Path(args.output).resolve()
    assert not output.exists(), 'fresh output required; no unchanged resampling'
    output.mkdir(parents=True)
    source = inventory(); assert not source['tracked_dirty'], 'commit before collection'
    write_json(output / 'source.json', source)
    master = Path(args.master).resolve()
    assert sha256_file(master / 'manifest.sqlite') == '541f55db9814837fd60386e42e56781e80fbdbd6d555d4bb0e3a099e215139af'
    seal = json.loads((master / 'seal.json').read_text())
    write_json(output / 'corpus.json', seal)
    assert shutil.disk_usage(output).free >= 16 * GIB, 'temporary disk admission unavailable'
    binary = Path(args.binary).resolve()
    write_json(output / 'build.json', {'binary_sha256': sha256_file(binary),
               'profile': 'release locked Cargo1.85.1; repo-root .cargo/config.toml',
               'cargo_config_sha256': sha256_file('.cargo/config.toml')})
    stage = output / 'stage'
    provider = output / 'minio-provider'
    with (output.parent / '.measurement.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        prep = deadline_command([str(binary), 'prepare', str(master), str(stage)],
                                output / 'preparation.log', 180)
        write_json(output / 'preparation-command.json', prep)
        print('preparation', prep['status'], f"{prep['wall_ns']/1e9:.3f}s", flush=True)
        if prep['status'] != 'COMPLETE': return
        catalog_hash = sha256_file(stage / 'catalog.sqlite')
        write_json(output / 'prepared-seal.json', {'catalog_sha256': catalog_hash,
                   'packs_seal': 'per-pack BLAKE3 in private catalog',
                   'catalog_bytes': (stage / 'catalog.sqlite').stat().st_size})
        provider.mkdir()
        shutil.copy2(Path(args.minio_binary), provider / 'minio')
        expected = 'b107901fd1afe7b36165c6aa66bb027f96e2ec2b690eebe8c9376746d9a9b0df'
        assert sha256_file(provider / 'minio') == expected
        write_json(output / 'minio.json', {'binary_sha256': expected,
                   'deployment': 'native isolated macOS loopback; compression off'})
        start(provider)
        script = str(Path(__file__).resolve())
        common = ['--master', str(master), '--stage', str(stage), '--output', str(output),
                  '--minio', str(provider), '--binary', str(binary),
                  '--sqlite-provider', str(Path(args.sqlite_provider).resolve())]
        env = {**os.environ, 'DYLD_LIBRARY_PATH': str(Path(args.sqlite_provider).resolve())}
        try:
            measured = deadline_command([sys.executable, script, 'perf'] + common,
                                        output / 'performance.log', 25, env)
            write_json(output / 'performance-command.json', measured)
            print('upload', measured['status'], f"{measured['wall_ns']/1e9:.3f}s", flush=True)
            if measured['status'] == 'COMPLETE':
                proof = deadline_command([sys.executable, script, 'verify'] + common,
                                         output / 'verification.log', 10, env)
                write_json(output / 'verification-command.json', proof)
                print('proof', proof['status'], f"{proof['wall_ns']/1e9:.3f}s", flush=True)
            else:
                write_json(output / 'verification-command.json', {'status': 'NOT_RUN', 'reason': 'upload incomplete'})
        finally:
            # No corpus objects use directory/; helper stops server without deleting packs/.
            cleanup_started = time.monotonic_ns()
            stop(provider)
            write_json(output / 'cleanup.json', {'status': 'PASS', 'retained_imported_store': True,
                       'wall_ns': time.monotonic_ns() - cleanup_started,
                       'shutdown': json.loads((provider / 'shutdown.json').read_text())})


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=['run', 'perf', 'verify'])
    parser.add_argument('--output', required=True)
    parser.add_argument('--master', default='benchmark-results/storage-probes/deepseek-full-master-v2')
    parser.add_argument('--stage')
    parser.add_argument('--minio')
    parser.add_argument('--sqlite-provider', default='benchmark-results/storage-probes/sqlite-3.51.3-provider')
    parser.add_argument('--minio-binary', default='benchmark-results/storage-probes/provider-complete-v1/minio')
    parser.add_argument('--binary', default='core/target/release/examples/minio_repository_probe')
    args = parser.parse_args()
    {'run': run, 'perf': perf, 'verify': verify}[args.action](args)
