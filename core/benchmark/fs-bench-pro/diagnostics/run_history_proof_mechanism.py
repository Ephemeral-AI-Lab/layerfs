#!/usr/bin/env python3
"""Native proof count diagnostic on retained stores; no speed/proof admission."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import sys
import subprocess
import time
ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT/'core/benchmark/fs-bench-pro'))
import runner
from families.phase7_history import build
from families.phase7_sqlite import invoke
from shared import cold_native, history_observer, phase7_history_proof as proof


BLOB_FIELDS = ['matched_pack_open_calls','matched_pack_open_ns','matched_pack_open_failures',
               'all_blob_read_calls','all_blob_requested_bytes','all_blob_returned_bytes',
               'all_blob_read_ns','all_blob_read_failures','all_blob_close_calls',
               'all_blob_close_ns','all_blob_close_failures']


def read_phases(lines):
    state = None
    rows = []
    for line in lines:
        if line.startswith('VERIFY_STATE_BEGIN state='):
            state = int(line.removeprefix('VERIFY_STATE_BEGIN state='))
        if line.startswith('VERIFY_PHASE_WORK '):
            if state is None:
                raise ValueError('phase outside declared state')
            row = json.loads(line.removeprefix('VERIFY_PHASE_WORK '))
            if row['stage'] not in ('walk','file-roots','remaining-digest') or len(row['sql_values']) != 19 or len(row['blob_values']) != len(BLOB_FIELDS):
                raise ValueError('phase counter schema mismatch')
            row['state'] = state
            rows.append(row)
    return rows


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--out', required=True)
    parser.add_argument('--states', type=int, choices=(17,53,157), default=17)
    parser.add_argument('--arm', choices=('baseline','candidate','both'), default='both')
    parser.add_argument('--baseline-run', default='issue302-history17-pooled-baseline1')
    parser.add_argument('--candidate-run', default='issue302-history17-pooled-candidate1')
    args = parser.parse_args()
    row = {17:'history-stride10',53:'history-stride3',157:'history-stride1'}[args.states]
    selected_arms = ['baseline','candidate'] if args.arm == 'both' else [args.arm]
    identity = runner.identities()
    if identity['source_dirty']:
        raise ValueError('freeze diagnostic source/harness')
    out = runner.owned(args.out)
    out.mkdir(parents=True)
    reference = ROOT/'target/phase7-baseline/layerfs'
    if subprocess.check_output(['git','rev-parse','HEAD'],cwd=reference,text=True).strip() != proof.BASE or subprocess.check_output(['git','status','--porcelain'],cwd=reference,text=True):
        raise ValueError('pinned clean reference checkout required')
    locks = []
    try:
        for path in (runner.RESULTS/'phase7-sqlite.lock', reference/'target/phase7-sqlite.lock'):
            handle = path.open('a+b')
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
            locks.append(handle)
        declaration = {'kind': f'history{args.states}-native-proof-count-v3', 'identity': identity,
                       'admission': 'NOT_APPLICABLE', 'native_budget_ns': 9_500_000_000,
                       'complete_per_arm_budget_ns': 60_000_000_000,
                       'arms': selected_arms, 'children_per_arm': 1,
                       'blob_counter_fields': BLOB_FIELDS, 'phase_scope': 'disjoint child phases nested inside outer per-state verification; never add child counters to parent',
                       'workload_row': row, 'baseline_run': args.baseline_run, 'candidate_run': args.candidate_run,
                       'reuse': 'original closed retained stores/producer roots/census/reference metadata; no speed rerun',
                       'cache': 'whole source corpus and retained database content invalidated and mincore checked before each native child',
                       'scope': 'native oracle/custody verification only; census/export/preservation of ordinary combined proof not included in native wall; no admission or proof promotion'}
        runner.write_json(out/'declaration.json', declaration)
        for arm, root in (('baseline', reference), ('candidate', ROOT)):
            if arm not in selected_arms: continue
            folder = out/arm
            folder.mkdir()
            original = runner.RESULTS/(args.baseline_run if arm == 'baseline' else args.candidate_run)
            runner.verify_run_manifest(original)
            receipt = json.loads((original/'receipt.json').read_text())
            db = original/'store.sqlite'
            owners = [db] if arm == 'candidate' else [db, Path(str(db)+'.history.sqlite')]
            if any(not p.is_file() or any(Path(str(p)+s).exists() for s in ('-wal', '-shm', '-journal')) for p in owners):
                raise ValueError('closed stores required')
            hashes = {str(p): runner.digest(p) for p in owners}
            if receipt['workload_row'] != row or receipt['performance']['child']['states'] != args.states:
                raise ValueError('retained workload selection mismatch')
            pins_path = runner.RESULTS/args.baseline_run/'root-pins.json'
            if arm == 'candidate':
                pins = json.loads(pins_path.read_text())
                proof.validate_pins(pins, pins['identity'])
                if receipt['performance']['child']['roots'] != pins['roots']:
                    raise ValueError('original root vector mismatch')
            vehicle = {}
            compiled = build(root, arm, folder, runner, vehicle)
            if compiled['status'] != 'PASS':
                raise ValueError('release build failed')
            observer = history_observer.build(ROOT, folder, invoke, runner)
            helper = cold_native.build(ROOT, folder, invoke)
            request = json.loads((original/'proof-request.json').read_text())
            corpus = Path(request['corpus'])
            binary = compiled['binaries']['verify_history' if arm == 'candidate' else 'history_reference_verify']['path']
            command = [binary, str(corpus), str(db), str(original/'producer-proof-input.json'), row, 'complete']
            command += ['disposable', str(pins_path)] if arm == 'candidate' else ['reference', 'independent-reference', str(original/'reference-metadata.tsv')]
            env = {**os.environ, 'LAYERFS_CONSTRUCTION_WORKERS': '1',
                   'LAYERFS_HISTORY_CURSOR_KEY': '28'*32, 'LAYERFS_HISTORY_VERIFY_PROGRESS': '1',
                   'DYLD_INSERT_LIBRARIES': observer['path'],
                   'LAYERFS_SQLITE_WORK_OUTPUT': str(folder/'sql-work.json'),
                   'LAYERFS_CAUSE_VFS_LOG': str(folder/'vfs.json'),
                   'LAYERFS_CLOSE_OBSERVER_OUTPUT': str(folder/'close.json')}
            start = time.monotonic_ns()
            if arm == 'baseline':
                metadata_output = folder/'reference-metadata.tsv'
                census = proof.collect(db, arm, receipt['performance']['child'], row, metadata_output)
                runner.write_json(folder/'diagnostic-census.json', census)
                command[-1] = str(metadata_output)
            cold = cold_native.attest_paths([corpus/'checkpoint-manifest.json', corpus/'inputs', corpus/'oracles', *owners], helper, folder, 60_000_000_000, invoke, ROOT)
            if cold['status'] != 'PASS':
                raise ValueError('diagnostic cache ineligible')
            print('START', arm, flush=True)
            result = invoke(command, folder, 'native', 9_500_000_000, env, root)
            preserved = all(runner.digest(p) == hashes[str(p)] and not any(Path(str(p)+s).exists() for s in ('-wal', '-shm', '-journal')) for p in owners)
            lines = (folder/'native.stderr').read_text().splitlines()
            states = [line for line in lines if line.startswith('VERIFY_STATE_WORK ')]
            record = {'kind': declaration['kind'], 'admission': 'NOT_APPLICABLE', 'status': 'DIAGNOSTIC_TIMEOUT' if result['timed_out'] else 'DIAGNOSTIC',
                      'identity': identity, 'original_receipt_sha256': runner.digest(original/'receipt.json'),
                      'build': compiled, 'vehicle': vehicle, 'observer': observer, 'cache': cold,
                      'owner_hashes_before': hashes, 'owners_preserved': preserved, 'native': result,
                      'complete_command_wall_ns': time.monotonic_ns()-start,
                      'completed_states': len(states), 'progress': states, 'phases': read_phases(lines),
                      'engine': [json.loads(line.removeprefix('HISTORY_ENGINE_WORK ')) for line in lines if line.startswith('HISTORY_ENGINE_WORK ')]}
            if any(not phase['available'] for phase in record['phases']) or not preserved or record['complete_command_wall_ns'] > 60_000_000_000 or (result['exit_code'] and not result['timed_out']):
                record['status'] = 'FAIL'
            runner.write_json(folder/'receipt.json', record)
            runner.manifest_run(folder)
            print('END', arm, record['status'], 'completed_states', len(states), flush=True)
    finally:
        runner.manifest_run(out)
        for handle in reversed(locks):
            handle.close()


if __name__ == '__main__':
    main()
