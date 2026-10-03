"""Sealed macOS cold-helper build and measured invocation; no fallback."""
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time

SOURCE = Path(__file__).with_suffix('.c')
FLAGS = ['-O2', '-std=c11', '-Wall', '-Wextra', '-Werror']


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def build(root, out, invoke):
    if platform.system() != 'Darwin':
        raise ValueError('required native cold profile is macOS-only')
    start = time.monotonic_ns()
    compiler = subprocess.run(['/usr/bin/clang', '--version'], check=True,
                              capture_output=True, text=True, timeout=2).stdout
    inputs = {'source_sha256': digest(SOURCE), 'compiler': compiler,
              'flags': FLAGS, 'platform': platform.platform(), 'architecture': platform.machine()}
    seal = hashlib.sha256(json.dumps(inputs, sort_keys=True).encode()).hexdigest()
    folder = Path(root) / 'benchmark-results/fs-bench-pro/cold-helper-archive' / seal
    binary = folder / 'phase7-cold'
    metadata = folder / 'identity.json'
    if metadata.exists():
        prior = json.loads(metadata.read_text())
        if prior['inputs'] != inputs or prior['binary_sha256'] != digest(binary):
            raise ValueError('immutable cold helper identity mismatch')
        return {**prior, 'mode': 'sealed-binary-reuse', 'build_wall_ns': time.monotonic_ns() - start}
    folder.mkdir(parents=True, exist_ok=False)
    command = ['/usr/bin/clang', *FLAGS, str(SOURCE), '-o', str(binary)]
    result = invoke(command, out, 'cold-build', 30_000_000_000, os.environ.copy(), root)
    if result['exit_code'] != 0 or result['timed_out']:
        raise ValueError('native cold helper build failed; retained cold-build output')
    binary.chmod(0o555)
    record = {'inputs': inputs, 'seal': seal, 'binary': str(binary),
              'binary_sha256': digest(binary), 'mode': 'compiled', 'build': result,
              'build_wall_ns': time.monotonic_ns() - start}
    metadata.write_text(json.dumps(record, sort_keys=True, indent=2) + '\n')
    return record


def attest(root, helper, out, budget_ns, invoke, cwd):
    binary = Path(helper['binary'])
    if helper['inputs']['source_sha256'] != digest(SOURCE) or helper['binary_sha256'] != digest(binary):
        raise ValueError('cold helper source/binary seal mismatch')
    result = invoke([str(binary), str(root)], out, 'cold', budget_ns, os.environ.copy(), cwd)
    row = result['child']
    if result['exit_code'] != 0 or result['timed_out'] or not isinstance(row, dict):
        raise ValueError('native cold attestation failed; retained cold stdout/stderr')
    if row['files'] <= 0 or row['page_size_bytes'] <= 0 or row['resident_after'] < 0:
        raise ValueError('invalid native cold attestation counters')
    return {**row, 'status': 'PASS' if row['resident_after'] == 0 else 'INELIGIBLE',
            'method': 'native two-pass msync-invalidate and whole-input mincore; no payload reads',
            'scope': 'regular-file content pages; filesystem metadata residency not directly observed',
            'invocation': result, 'helper_seal': helper['seal'], 'helper_binary_sha256': helper['binary_sha256']}
