#!/usr/bin/env python3
"""Native proof count diagnostic on retained stores; no speed/proof admission."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import sys
import time
ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT/'core/benchmark/fs-bench-pro'))
import runner
from families.phase7_history import build
from families.phase7_sqlite import invoke
from shared import cold_native, history_observer, phase7_history_proof as proof


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--out', required=True)
    args = parser.parse_args()
    identity = runner.identities()
    if identity['source_dirty']:
        raise ValueError('freeze diagnostic source/harness')
    out = runner.owned(args.out)
    out.mkdir(parents=True)
    reference = ROOT/'target/phase7-baseline/layerfs'
    locks = []
    try:
        for path in (runner.RESULTS/'phase7-sqlite.lock', reference/'target/phase7-sqlite.lock'):
            handle = path.open('a+b')
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
            locks.append(handle)
        declaration = {'kind': 'history17-native-proof-count-v1', 'identity': identity,
                       'admission': 'NOT_APPLICABLE', 'native_budget_ns': 9_500_000_000,
                       'complete_per_arm_budget_ns': 60_000_000_000,
                       'arms': ['baseline', 'candidate'], 'children_per_arm': 1,
                       'reuse': 'original closed17v2 stores/producer roots/census/reference metadata; no speed rerun',
                       'cache': 'whole source corpus and retained database content invalidated and mincore checked before each native child',
                       'scope': 'native oracle/custody verification only; census/export/preservation of ordinary combined proof not included in native wall; no admission or proof promotion'}
        runner.write_json(out/'declaration.json', declaration)
        for arm, root in (('baseline', reference), ('candidate', ROOT)):
            folder = out/arm
            folder.mkdir()
            original = runner.RESULTS/f'issue302-history17-pooled-{arm}1'
            receipt = json.loads((original/'receipt.json').read_text())
            db = original/'store.sqlite'
            owners = [db] if arm == 'candidate' else [db, Path(str(db)+'.history.sqlite')]
            if any(not p.is_file() or any(Path(str(p)+s).exists() for s in ('-wal', '-shm', '-journal')) for p in owners):
                raise ValueError('closed stores required')
            hashes = {str(p): runner.digest(p) for p in owners}
            pins_path = runner.RESULTS/'issue302-history17-pooled-baseline1/root-pins.json'
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
            command = [binary, str(corpus), str(db), str(original/'producer-proof-input.json'), 'history-stride10', 'complete']
            command += ['disposable', str(pins_path)] if arm == 'candidate' else ['reference', 'independent-reference', str(original/'reference-metadata.tsv')]
            env = {**os.environ, 'LAYERFS_CONSTRUCTION_WORKERS': '1',
                   'LAYERFS_HISTORY_CURSOR_KEY': '28'*32, 'LAYERFS_HISTORY_VERIFY_PROGRESS': '1',
                   'DYLD_INSERT_LIBRARIES': observer['path'],
                   'LAYERFS_SQLITE_WORK_OUTPUT': str(folder/'sql-work.json'),
                   'LAYERFS_CAUSE_VFS_LOG': str(folder/'vfs.json'),
                   'LAYERFS_CLOSE_OBSERVER_OUTPUT': str(folder/'close.json')}
            start = time.monotonic_ns()
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
                      'completed_states': len(states), 'progress': states,
                      'engine': [json.loads(line.removeprefix('HISTORY_ENGINE_WORK ')) for line in lines if line.startswith('HISTORY_ENGINE_WORK ')]}
            if not preserved or record['complete_command_wall_ns'] > 60_000_000_000 or (result['exit_code'] and not result['timed_out']):
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
