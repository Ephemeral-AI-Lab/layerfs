"""Scoped Linux compilation and component verification; no mounted acceptance."""
from pathlib import Path
import hashlib
import json
import os
import signal
import subprocess
import sys
import time

out = Path('core/docs/issues/307/checks/r2-fuse-service-20261008')
records = []
sources = [*Path('core/crates/layerfs-fuse').rglob('*.rs'),
           Path('core/crates/layerfs-fuse/Cargo.toml'),
           Path('core/crates/layerfs-daemon/src/service/filesystem_port.rs'),
           Path('core/crates/layerfs-daemon/tests/filesystem_port.rs'),
           Path('core/Cargo.lock')]
identity = {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}

def run(name, command, limit=100):
    Path('benchmark_agent_report.md').read_text()
    started = time.monotonic_ns()
    expired = False
    with (out / name).open('x') as log:
        process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = process.wait(timeout=limit)
        except subprocess.TimeoutExpired:
            expired = True
            os.killpg(process.pid, signal.SIGKILL)
            code = process.wait()
    record = {'command': command, 'exit': code, 'timeout': expired,
              'wall_bound_seconds': limit, 'elapsed_ns': time.monotonic_ns()-started, 'output': name}
    records.append(record)
    (out / 'linux-callback-results.json').write_text(json.dumps({
        'sources': identity, 'records': records,
        'scope': 'functional component verification; natural caches; no mount/performance/residency claim',
        'profile': 'explicit global Disposable/WAL/OFF; Overlay MEMORY/OFF/EXCLUSIVE; Durable NOT_RUN disabled',
    }, indent=2) + '\n')
    print(json.dumps(record), flush=True)
    print((out / name).read_text()[-6000:], flush=True)
    if code:
        sys.exit(code)

run('linux-callback-fuser-integrity.txt', ['python3', '-B', 'core/tools/check_fuser_integrity.py'])
base = ['--manifest-path', 'core/Cargo.toml', '--locked']
run('linux-callback-fuse-build.txt', ['cargo', 'test', *base, '-p', 'layerfs-fuse', '--all-targets', '--no-run'])
run('linux-callback-port-build.txt', ['cargo', 'test', *base, '-p', 'layerfs-daemon', '--test', 'filesystem_port', '--no-run'])
run('linux-callback-dispatch-tests.txt', ['cargo', 'test', *base, '-p', 'layerfs-fuse', '--test', 'dispatch', '--', '--test-threads=1'])
run('linux-callback-attribute-tests.txt', ['cargo', 'test', *base, '-p', 'layerfs-fuse', '--test', 'attributes', '--', '--test-threads=1'])
run('linux-callback-port-tests.txt', ['cargo', 'test', *base, '-p', 'layerfs-daemon', '--test', 'filesystem_port', '--', '--test-threads=1'])
run('linux-callback-clippy.txt', ['cargo', 'clippy', *base, '-p', 'layerfs-fuse', '-p', 'layerfs-daemon', '--all-targets', '--', '-D', 'warnings'])
assert identity == {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}, 'source changed during checks'
