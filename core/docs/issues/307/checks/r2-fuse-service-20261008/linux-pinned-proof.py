"""Final compiled identities for the unchanged dispatcher/attribute families."""
from pathlib import Path
import hashlib
import json
import os
import signal
import subprocess
import time

out = Path('core/docs/issues/307/checks/r2-fuse-service-20261008')
source = json.loads((out / '52-source-identity.json').read_text())['sha256']
assert all(hashlib.sha256(Path(p).read_bytes()).hexdigest() == digest for p, digest in source.items())
selection = json.loads((out / '62-binary-pins.json').read_text())['linux'][:2]
records = []
for number, selected in enumerate(selection):
    Path('benchmark_agent_report.md').read_text()
    binary = Path(selected['path'])
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == selected['sha256']
    command = [str(binary), '--test-threads=1']
    started = time.monotonic_ns()
    expired = False
    with (out / f'linux-pinned-{number}.txt').open('x') as log:
        process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = process.wait(timeout=100)
        except subprocess.TimeoutExpired:
            expired = True
            os.killpg(process.pid, signal.SIGKILL)
            code = process.wait()
    record = {**selected, 'command': command, 'exit': code, 'timeout': expired,
              'wall_bound_seconds': 100, 'elapsed_ns': time.monotonic_ns()-started}
    records.append(record)
    print(json.dumps(record), flush=True)
    print((out / f'linux-pinned-{number}.txt').read_text(), flush=True)
    (out / '63-linux-pinned-results.json').write_text(json.dumps(records, indent=2) + '\n')
    if code:
        raise SystemExit(code)
