#!/usr/bin/env python3
"""Declare and collect all four one-sample arms through the sole runner.py."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time
import runner
from families.phase7_sqlite import REQUIRED_BY_PROFILE
from shared.sqlite_contract import gate


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--out', required=True)
    args = parser.parse_args()
    identity = runner.identities()
    if identity['source_dirty']:
        raise ValueError('freeze committed source/harness before campaign')
    out = runner.owned(args.out); out.mkdir(parents=True)
    cases = list(REQUIRED_BY_PROFILE['disposable'][:4])
    runner.write_json(out/'declaration.json', {
        'schema': 'phase7-supported-disposable-init-campaign-v2', 'identity': identity,
        'cases': cases, 'order': '100/1000/10000/100000; baseline then candidate',
        'samples_per_case_arm': 1, 'profile': 'supported direct-open MEMORY/OFF; original Phase4.5 MEMORY/OFF',
        'command_budget_ns': 15_000_000_000, 'verification_budget_ns': 9_500_000_000,
        'margin': '10*candidate<=11*baseline', 'allocation': 'candidate-final-total<=matched-baseline-final-total',
        'cache': 'sealed native source invalidation and whole-input mincore zero; fresh databases',
        'workers': 'Init4; environment1', 'history_status': 'NOT_RUN; independent Init milestone first',
        'orchestrator_source_sha256': runner.digest(Path(__file__)),
    })
    rows = []
    for case in cases:
        records = {}
        for arm in ('baseline', 'candidate'):
            folder = out/f'{case}-{arm}'
            command = [sys.executable, str(runner.HERE/'runner.py'), 'run', '--case', case,
                       '--arm', arm, '--out', str(folder)]
            if arm == 'baseline': command += ['--baseline-root', str(runner.ROOT/'target/phase7-baseline/layerfs')]
            print('START', case, arm, flush=True); start = time.monotonic_ns()
            with (out/f'{case}-{arm}.stdout').open('x') as stdout, (out/f'{case}-{arm}.stderr').open('x') as stderr:
                result = subprocess.run(command, cwd=runner.ROOT, stdout=stdout, stderr=stderr)
            receipt = folder/'receipt.json'
            record = json.loads(receipt.read_text()) if receipt.exists() else {'status': 'INCOMPLETE', 'reason': 'missing receipt'}
            records[arm] = record
            print('END', case, arm, json.dumps({
                'exit': result.returncode, 'cli_wall_ns': time.monotonic_ns()-start,
                'status': record['status'], 'operation_ns': record.get('comparison_ns'),
                'command_ns': record.get('command_wall_ns'), 'proof': record.get('verification_status'),
                'cold': record.get('cache_status'), 'storage_bytes': record.get('storage_bytes'),
            }), flush=True)
        baseline, candidate = records['baseline'], records['candidate']
        status = gate(candidate, baseline, baseline.get('storage_bytes'))
        old = baseline.get('performance', {}).get('child') or {}
        new = candidate.get('performance', {}).get('child') or {}
        roots_equal = isinstance(new.get('root'), str) and new.get('root') == old.get('root')
        if not roots_equal and status == 'PASS': status = 'FAIL'
        row = {'case': case, 'status': status, 'roots_equal': roots_equal,
               'baseline_ns': baseline.get('comparison_ns'), 'candidate_ns': candidate.get('comparison_ns'),
               'baseline_storage': baseline.get('storage_bytes'), 'candidate_storage': candidate.get('storage_bytes')}
        if baseline.get('comparison_ns') and candidate.get('comparison_ns'):
            row['ratio'] = candidate['comparison_ns']/baseline['comparison_ns']
        rows.append(row); print('PAIR', json.dumps(row), flush=True)
    runner.write_json(out/'comparison.json', {'identity': identity, 'rows': rows,
                      'history': 'NOT_RUN', 'durable': 'NOT_RUN; follows all-seven Disposable qualification'})
    runner.manifest_run(out)


if __name__ == '__main__':
    main()
