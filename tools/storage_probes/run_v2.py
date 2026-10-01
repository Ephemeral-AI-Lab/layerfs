"""Twenty-four source-pinned selections; V1 arms are never re-executed."""
import argparse
import fcntl
import hashlib
import json
from pathlib import Path
import sys

from common import write_json
from run import command, inventory
from pack_probe import CASES as PACKS
from catalog_probe import CASES as CATALOG
from concurrent_probe import CASES as CONCURRENT

GROUPS = {'pack': list(PACKS),
          'publication': [k for k in CATALOG if k.startswith('D-')],
          'concurrency': list(CONCURRENT),
          'scaling': [k for k in CATALOG if k.startswith('S-')]}


def run(args):
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    source = inventory()
    if source['tracked_dirty']:
        raise RuntimeError('freeze tracked source before collection')
    repo = Path(__file__).resolve().parents[2]
    binary = repo / 'core/target/release/examples/minio_pack_probe'
    source['pack_example_sha256'] = hashlib.sha256(
        (repo / 'core/crates/layerfs-storage/examples/minio_pack_probe.rs').read_bytes()).hexdigest()
    source['pack_binary_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
    source['cargo_lock_sha256'] = hashlib.sha256((repo / 'core/Cargo.lock').read_bytes()).hexdigest()
    source['cargo_config_sha256'] = hashlib.sha256((repo / '.cargo/config.toml').read_bytes()).hexdigest()
    identity = output / 'source.json'
    if identity.exists():
        assert json.loads(identity.read_text()) == source, 'source changed; retain existing output'
    else:
        write_json(identity, source)
    if args.provider:
        provider = Path(args.provider).resolve()
        seal = json.loads((provider / 'provider.json').read_text())
        target = output / 'provider.json'
        if target.exists(): assert json.loads(target.read_text()) == seal
        else: write_json(target, seal)
    cases = GROUPS[args.group]
    if args.case:
        assert args.case in cases
        cases = [args.case]
    lockpath = repo / 'benchmark-results/storage-probes/.measurement.lock'
    with lockpath.open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        for case in cases:
            destination = output / case
            logfile = output / (case + '-perf.log')
            if destination.exists() or logfile.exists():
                raise RuntimeError('case already attempted; no unchanged resampling: ' + case)
            script = ('pack_probe.py' if case.startswith('P-') else
                      'concurrent_probe.py' if case.startswith('C-') else 'catalog_probe.py')
            base = [sys.executable, str(Path(__file__).parent / script)]
            options = ['--case', case, '--output', str(destination)]
            if case.startswith('P-'):
                options += ['--fixtures', str(Path(args.pack_fixtures).resolve()), '--provider', str(provider)]
            elif case.startswith('C-upload'):
                options += ['--provider', str(provider)]
            elif case.startswith(('D-', 'S-')):
                options += ['--fixtures', str(Path(args.catalog_fixtures).resolve())]
            perf = command(base + ['perf'] + options, logfile, 15)
            destination.mkdir(exist_ok=True)
            write_json(destination / 'performance-command.json', perf)
            if perf['status'] == 'COMPLETE':
                proof = command(base + ['verify'] + options, output / (case + '-verify.log'), 10)
                write_json(destination / 'verification-command.json', proof)
            else:
                write_json(destination / 'verification-command.json',
                           {'status': 'NOT_RUN', 'reason': 'performance failed or timed out'})
            print(case, perf['status'], f"wall={perf['wall_ns']/1e9:.3f}s", flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--list', action='store_true')
    parser.add_argument('--group', choices=GROUPS)
    parser.add_argument('--case')
    parser.add_argument('--output')
    parser.add_argument('--provider')
    parser.add_argument('--pack-fixtures', default='benchmark-results/storage-probes/v2-pack-fixtures')
    parser.add_argument('--catalog-fixtures', default='benchmark-results/storage-probes/v2-catalog-fixtures')
    args = parser.parse_args()
    if args.list:
        for group, cases in GROUPS.items():
            for case in cases: print(group, case)
    elif not args.group or not args.output:
        parser.error('--group and --output required')
    else:
        run(args)
