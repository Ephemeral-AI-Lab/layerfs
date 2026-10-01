"""Single-attempt WAL profile collector with isolated fixed SQLite library."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import subprocess
import sys

from common import write_json
from run import inventory
from wal_probe import CASES


def run(args):
    provider = Path(args.provider).resolve()
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    source = inventory()
    assert not source['tracked_dirty'], 'freeze source before collection'
    identity = output / 'source.json'
    if identity.exists(): assert json.loads(identity.read_text()) == source
    else: write_json(identity, source)
    write_json(output / 'provider.json', json.loads((provider / 'qualification.json').read_text()))
    env = {**os.environ, 'DYLD_LIBRARY_PATH': str(provider)}
    cases = [args.case] if args.case else list(CASES)
    script = Path(__file__).parent / 'wal_probe.py'
    lockpath = Path(__file__).resolve().parents[2] / 'benchmark-results/storage-probes/.measurement.lock'
    with lockpath.open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        for case in cases:
            destination = output / case
            perf_log = output / (case + '-perf.log')
            assert not destination.exists() and not perf_log.exists(), 'attempt already retained'
            base = [sys.executable, str(script), '--case', case,
                    '--provider', str(provider), '--output', str(destination)]
            for action, limit in [('perf', 15), ('verify', 10)]:
                import time
                start = time.monotonic_ns()
                argv = base[:2] + [action] + base[2:]
                with (output / (case + '-' + action + '.log')).open('xb') as log:
                    try:
                        result = subprocess.run(argv, stdout=log, stderr=subprocess.STDOUT,
                                                env=env, timeout=limit)
                        code = result.returncode; status = 'COMPLETE' if code == 0 else 'FAIL'
                    except subprocess.TimeoutExpired: code = None; status = 'TIMEOUT'
                destination.mkdir(exist_ok=True)
                record = {'command': argv, 'status': status, 'exit': code,
                          'wall_ns': time.monotonic_ns() - start, 'limit_seconds': limit,
                          'library_override': str(provider)}
                write_json(destination / ('performance-command.json' if action == 'perf'
                                          else 'verification-command.json'), record)
                print(case, action, status, f"wall={record['wall_ns']/1e9:.3f}s", flush=True)
                if status != 'COMPLETE' and action == 'perf':
                    write_json(destination / 'verification-command.json',
                               {'status': 'NOT_RUN', 'reason': 'performance failed/timed out'})
                    break


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--list', action='store_true')
    parser.add_argument('--case', choices=CASES)
    parser.add_argument('--provider', default='benchmark-results/storage-probes/sqlite-3.51.3-provider')
    parser.add_argument('--output')
    args = parser.parse_args()
    if args.list:
        for name, shape in CASES.items(): print(name, shape)
    elif not args.output: parser.error('--output required')
    else: run(args)
