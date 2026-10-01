"""One source-pinned full JuiceFS upload attempt; private corpus and provider logs."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import sqlite3
import stat
import subprocess
import sys
import time
import xml.etree.ElementTree as ET

sys.path.insert(0, str(Path(__file__).parent / 'storage_probes'))
from common import S3, write_json
from minio_probe import start, stop
from run import inventory

MASTER = Path('/Users/yifanxu/.codex/worktrees/issue290-storage-probes/layerfs/benchmark-results/storage-probes/deepseek-full-master-v2')
MANIFEST = '541f55db9814837fd60386e42e56781e80fbdbd6d555d4bb0e3a099e215139af'
MINIO = Path('/Users/yifanxu/.codex/worktrees/issue290-storage-probes/layerfs/benchmark-results/storage-probes/provider-complete-v1/minio')
MINIO_SHA = 'b107901fd1afe7b36165c6aa66bb027f96e2ec2b690eebe8c9376746d9a9b0df'
JUICE_SHA = '27b40fe76522ae0b35887a7e7642289de4324a5f974274efa527c1b12312a226'
FLAGS = ['sync', '--threads', '1', '--list-threads', '1', '--dirs', '--perms', '--links']


def sha(path):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def execute(root, name, argv, limit, env=None):
    receipt = root / (name + '-command.json')
    if receipt.exists():
        raise RuntimeError('attempt already exists: ' + name)
    logfile = root / (name + '-private.log')
    fd = os.open(logfile, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    started = time.monotonic_ns()
    with os.fdopen(fd, 'wb') as log:
        process = subprocess.Popen(argv, stdout=log, stderr=subprocess.STDOUT,
                                   env=env, start_new_session=True)
        try:
            code = process.wait(timeout=limit)
            status = 'COMPLETE' if code == 0 else 'FAIL'
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
            status, code = 'TIMEOUT', None
    result = {'command': argv, 'status': status, 'exit': code,
              'wall_ns': time.monotonic_ns() - started, 'limit_seconds': limit,
              'private_log_sha256': sha(logfile), 'log_publication': 'private: may contain corpus paths'}
    write_json(receipt, result)
    print(name, status, f"wall={result['wall_ns']/1e9:.3f}s", flush=True)
    return result


def environment(root):
    return {**os.environ, 'juiceprobe': 'sqlite3://' + str(root / 'metadata.sqlite'),
            'juicequal': 'sqlite3://' + str(root / 'qualification.sqlite')}


def setup(root, binary):
    root.mkdir(parents=True, exist_ok=False)
    assert shutil.disk_usage(root).free >= 16 * 1024**3
    assert sha(MASTER / 'manifest.sqlite') == MANIFEST
    assert sha(binary) == JUICE_SHA and sha(MINIO) == MINIO_SHA
    provider = root / 'minio'; provider.mkdir()
    shutil.copyfile(MINIO, provider / 'minio'); (provider / 'minio').chmod(0o755)
    start(provider)
    config = json.loads((provider / 'private-config.json').read_text())
    env = {**environment(root), 'ACCESS_KEY': config['access'], 'SECRET_KEY': config['secret']}
    base = [str(binary), 'format', '--storage', 'minio', '--bucket',
            f"http://127.0.0.1:{config['port']}/{config['bucket']}",
            '--compress', 'zstd', '--block-size', '4M']
    try:
        for name, file, volume in [('format', 'metadata.sqlite', 'juice292full'),
                                  ('qualification-format', 'qualification.sqlite', 'juice292qual')]:
            result = execute(root, name, base + ['sqlite3://' + str(root / file), volume], 15, env)
            if result['status'] != 'COMPLETE':
                raise RuntimeError(name + ' incomplete; private log retained')
        source = root / 'qualification-source'; source.mkdir()
        (source / 'a').write_bytes(bytes(range(256)) * 8)
        (source / 'empty').write_bytes(b'')
        (source / 'dir').mkdir(); os.symlink('a', source / 'link')
        os.chmod(source / 'a', 0o640)
        target = root / 'qualification-readback'; target.mkdir()
        for name, paths in [('qualification-upload', [str(source) + '/', 'jfs://juicequal/']),
                            ('qualification-readback', ['jfs://juicequal/', str(target) + '/'])]:
            result = execute(root, name, [str(binary)] + FLAGS + paths, 10, environment(root))
            if result['status'] != 'COMPLETE':
                raise RuntimeError(name + ' incomplete; private log retained')
        assert (target / 'a').read_bytes() == (source / 'a').read_bytes()
        assert (target / 'empty').read_bytes() == b'' and (target / 'dir').is_dir()
        assert (target / 'link').is_symlink() and os.readlink(target / 'link') == 'a'
        assert (target / 'a').stat().st_mode & 0o777 == 0o640
        write_json(root / 'setup.json', {'qualification': 'PASS', 'scope': 'two files, empty directory, symlink and portable mode; no throughput sample',
                   'juicefs_sha256': JUICE_SHA, 'minio_sha256': MINIO_SHA,
                   'master_manifest_sha256': MANIFEST,
                   'corpus': json.loads((MASTER / 'seal.json').read_text())})
    except BaseException:
        stop(provider)
        raise


def inspect(root):
    started = time.monotonic_ns(); deadline = time.monotonic() + 10
    db = sqlite3.connect('file:' + str(root / 'metadata.sqlite') + '?mode=ro', uri=True)
    tables = [r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table'")]
    counts = db.execute('SELECT type,count(*),sum(length) FROM jfs_node GROUP BY type').fetchall()
    profile = {'journal_mode': db.execute('PRAGMA journal_mode').fetchone()[0],
               'native_synchronous_cache_sqlite_version': 'unavailable; external connection cannot establish native settings'}
    db.close()
    config = json.loads((root / 'minio/private-config.json').read_text()); client = S3(config)
    token = None; count = total = pages = 0
    try:
        while True:
            if time.monotonic() >= deadline:
                raise TimeoutError('read-only object inventory exceeded 10s')
            query = {'list-type': 2, 'prefix': 'juice292full/', 'max-keys': 1000}
            if token: query['continuation-token'] = token
            response = ET.fromstring(client.call('GET', query=query, response_limit=1024**2))
            ns = {'s': 'http://s3.amazonaws.com/doc/2006-03-01/'}
            for entry in response.findall('s:Contents', ns):
                count += 1; total += int(entry.findtext('s:Size', namespaces=ns))
            pages += 1
            if response.findtext('s:IsTruncated', namespaces=ns) != 'true': break
            token = response.findtext('s:NextContinuationToken', namespaces=ns)
            assert token
    finally:
        client.close()
    result = {'status': 'COMPLETE', 'wall_ns': time.monotonic_ns() - started,
              'metadata_counts_by_type': counts, 'tables': tables, 'profile': profile,
              'objects': count, 'object_bytes': total, 'list_pages': pages,
              'sqlite_file_bytes': {p.name: p.stat().st_size for p in root.glob('metadata.sqlite*')},
              'scope': 'read-only post-attempt inventory; metadata counts do not establish exact bytes or completed upload'}
    write_json(root / 'inventory.json', result)
    print('observed metadata/object inventory recorded', flush=True)


def verify(root, binary):
    deadline = time.monotonic() + 10
    target = root / 'readback'; target.mkdir(exist_ok=False)
    argv = [str(binary)] + FLAGS + ['jfs://juiceprobe/', str(target) + '/']
    # This child remains in the verifier's process group so the outer watchdog
    # terminates both processes together. Reconstruction and comparison share it.
    result = subprocess.run(argv, env=environment(root), timeout=max(.001, deadline-time.monotonic()))
    if result.returncode:
        raise RuntimeError('public reconstruction failed')
    db = sqlite3.connect('file:' + str(MASTER / 'manifest.sqlite') + '?mode=ro', uri=True)
    files = byte_count = entries = 0
    try:
        for path, kind, mode, size, link, digest in db.execute(
                'SELECT path,kind,mode,size,link,sha256 FROM entries ORDER BY path'):
            if time.monotonic() >= deadline: raise TimeoutError('full independent comparison deadline')
            selected = target / os.fsdecode(path)
            observed = selected.lstat()
            if kind == 'file':
                assert stat.S_ISREG(observed.st_mode) and observed.st_size == size
                assert stat.S_IMODE(observed.st_mode) == mode
                hasher = hashlib.sha256(); received = 0
                with selected.open('rb') as source:
                    while block := source.read(65536):
                        if time.monotonic() >= deadline: raise TimeoutError('byte comparison deadline')
                        hasher.update(block); received += len(block)
                assert received == size and hasher.hexdigest() == digest
                files += 1; byte_count += received
            elif kind == 'directory':
                assert stat.S_ISDIR(observed.st_mode)
            else:
                assert stat.S_ISLNK(observed.st_mode) and os.readlink(os.fsencode(selected)) == link
            entries += 1
            if files and files % 128 == 0:
                write_json(root / 'verification-progress.json', {'files': files, 'bytes': byte_count,
                           'entries': entries, 'complete': False})
        observed_entries = 1
        for parent, directories, filenames in os.walk(target, followlinks=False):
            if time.monotonic() >= deadline: raise TimeoutError('extra entry census deadline')
            observed_entries += len(directories) + len(filenames)
        assert observed_entries == entries and files == 103108 and byte_count == 3475776149
    finally:
        db.close()
    write_json(root / 'verification.json', {'status': 'PASS', 'files': files,
               'bytes': byte_count, 'entries': entries, 'scope': 'exact fixture bytes/EOF, names, links and regular modes; xattrs/flags/ownership unverified'})


def run(root, binary):
    assert (root / 'setup.json').exists() and not (root / 'performance-command.json').exists()
    source = inventory(); assert not source['tracked_dirty'], 'commit before sample'
    source['probe_sha256'] = sha(__file__); write_json(root / 'source.json', source)
    assert sha(MASTER / 'manifest.sqlite') == MANIFEST
    started = time.monotonic_ns()
    try:
        result = execute(root, 'performance', [str(binary)] + FLAGS +
                         [str(MASTER / 'tree') + '/', 'jfs://juiceprobe/'], 25, environment(root))
        write_json(root / 'performance.json', {**result, 'case': 'J-deepseek-full-upload-v1',
                   'cache_verdict': 'INELIGIBLE', 'performance_claim': False,
                   'cache_contract': 'reused closed master; native/OS residency unobserved',
                   'expected_regular_files': 103108, 'expected_symlinks': 10070,
                   'expected_regular_bytes': 3475776149, 'copy_threads': 1,
                   'profile': 'whole public sync command; upstream jfs defaults; no mount'})
        if result['status'] == 'COMPLETE':
            proof = execute(root, 'verification', [sys.executable, str(Path(__file__).resolve()),
                            'verify', '--output', str(root), '--binary', str(binary)], 10)
            if proof['status'] != 'COMPLETE':
                write_json(root / 'verification.json', {'status': 'INCOMPLETE',
                           'reconstruction_command': proof['status'],
                           'reason': 'exact proof did not complete within its bound'})
        else:
            write_json(root / 'verification.json', {'status': 'NOT_RUN', 'reason': 'upload incomplete'})
        execute(root, 'inventory', [sys.executable, str(Path(__file__).resolve()),
                'inspect', '--output', str(root)], 10)
    finally:
        cleanup = execute(root, 'cleanup', [sys.executable, str(Path(__file__).resolve()),
                          'stop', '--output', str(root)], 10)
        shutdown = root / 'minio/shutdown.json'
        write_json(root / 'cleanup.json', {'status': 'PASS' if cleanup['status']=='COMPLETE' else 'INCOMPLETE',
                   'wall_ns': cleanup['wall_ns'], 'retained_import': True,
                   'shutdown': json.loads(shutdown.read_text()) if shutdown.exists() else None})
        print('complete orchestration wall', (time.monotonic_ns()-started)/1e9, flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=['setup', 'run', 'inspect', 'verify', 'stop'])
    parser.add_argument('--output', required=True)
    parser.add_argument('--binary', default='benchmark-results/juicefs-deepseek/provider/juicefs')
    args = parser.parse_args(); root = Path(args.output).resolve(); binary = Path(args.binary).resolve()
    lock = Path('benchmark-results/juicefs-deepseek/.measurement.lock'); lock.parent.mkdir(exist_ok=True)
    if args.action in ('inspect', 'verify', 'stop'):
        {'inspect': lambda: inspect(root), 'verify': lambda: verify(root, binary),
         'stop': lambda: stop(root / 'minio')}[args.action]()
        sys.exit(0)
    with lock.open('a') as handle:
        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        {'setup': lambda: setup(root, binary), 'run': lambda: run(root, binary),
         'inspect': lambda: inspect(root)}[args.action]()
