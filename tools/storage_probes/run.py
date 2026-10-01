"""Fixed-count standalone backend runner; no aggregate product/pre-push checks."""
import argparse
import fcntl
import hashlib
import json
from pathlib import Path
import platform
import sqlite3
import subprocess
import sys
import time

from common import write_json
from minio_probe import CASES as MINIO
from sqlite_probe import CASES as SQLITE


def inventory():
    root = Path(__file__).resolve().parents[2]
    tracked = subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'],
                                      cwd=root, text=True)
    return {'commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root,
                                              text=True).strip(),
            'tracked_dirty': bool(tracked), 'python': sys.version,
            'python_executable': sys.executable, 'sqlite': sqlite3.sqlite_version,
            'platform': platform.platform(),
            'hardware': subprocess.check_output(
                ['sysctl', 'machdep.cpu.brand_string', 'hw.memsize', 'hw.ncpu'], text=True).strip(),
            'tool_sha256': {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                           for p in Path(__file__).parent.glob('*.py')}}


def command(argv, logfile, limit):
    started = time.monotonic_ns()
    with logfile.open('xb') as log:
        try:
            result = subprocess.run(argv, stdout=log, stderr=subprocess.STDOUT, timeout=limit)
            exit_code, status = result.returncode, 'COMPLETE' if result.returncode == 0 else 'FAIL'
        except subprocess.TimeoutExpired:
            exit_code, status = None, 'TIMEOUT'
    return {'command': argv, 'exit': exit_code, 'status': status,
            'wall_ns': time.monotonic_ns() - started, 'limit_seconds': limit,
            'log': str(logfile.resolve())}


def run(args):
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    source = inventory()
    if source['tracked_dirty']:
        raise RuntimeError('tracked source dirty; freeze source before throughput collection')
    identity = output / 'source.json'
    if identity.exists():
        if json.loads(identity.read_text()) != source:
            raise RuntimeError('output source differs; preserve it and select fresh output')
    else:
        write_json(identity, source)
    cases = list(SQLITE if args.engine == 'sqlite' else MINIO)
    if args.case:
        if args.case not in cases:
            raise RuntimeError('case does not belong to selected engine')
        cases = [args.case]
    if args.engine == 'minio':
        provider = Path(args.provider).resolve()
        if not (provider / 'private-config.json').exists():
            raise RuntimeError('owned MinIO not started; run documented provider preparation')
        if 'M-directory-10000-v1' in cases and not (provider / 'directory-master.json').exists():
            raise RuntimeError('directory master missing; acquire once before performance')
        write_json(output / 'provider.json', json.loads((provider / 'provider.json').read_text()))
    # The per-worktree measurement lock prevents overlapping this runner's selections.
    lock_path = Path(__file__).resolve().parents[2] / 'benchmark-results/storage-probes/.measurement.lock'
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    with lock_path.open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        for case in cases:
            case_output = output / case
            if case_output.exists() or (output / (case + '-perf.log')).exists():
                raise RuntimeError('case already attempted; no unchanged resampling: ' + case)
            script = 'sqlite_probe.py' if args.engine == 'sqlite' else 'minio_probe.py'
            base = [sys.executable, str(Path(__file__).parent / script)]
            options = ['--case', case, '--output', str(case_output)]
            if args.engine == 'minio':
                options += ['--root', str(Path(args.provider).resolve())]
            perf = command(base + ['perf'] + options, output / (case + '-perf.log'), 15)
            case_output.mkdir(exist_ok=True)
            write_json(case_output / 'performance-command.json', perf)
            if perf['status'] == 'COMPLETE':
                proof = command(base + ['verify'] + options, output / (case + '-verify.log'), 10)
                write_json(case_output / 'verification-command.json', proof)
            else:
                write_json(case_output / 'verification-command.json',
                           {'status': 'NOT_RUN', 'reason': 'performance failed or timed out'})
            print(case, perf['status'], f"wall={perf['wall_ns']/1e9:.3f}s", flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--list', action='store_true')
    parser.add_argument('--engine', choices=['sqlite', 'minio'])
    parser.add_argument('--case')
    parser.add_argument('--output')
    parser.add_argument('--provider', default='benchmark-results/storage-probes/provider')
    args = parser.parse_args()
    if args.list:
        for engine, selection in [('sqlite', SQLITE), ('minio', MINIO)]:
            for case, shape in selection.items():
                print(engine, case, shape)
    elif not args.engine or not args.output:
        parser.error('--engine and --output are required')
    else:
        run(args)
