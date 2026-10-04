#!/usr/bin/env python3
"""One candidate cause diagnostic, exact allocation descriptor attribution only."""
import argparse
import json
import os
from pathlib import Path
import re
import sys
import time
ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT/'core/benchmark/fs-bench-pro'))
import runner
from families.phase7_sqlite import invoke, archive
from shared import cold_native


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--out', required=True)
    parser.add_argument('--campaign', required=True)
    args = parser.parse_args()
    identity = runner.identities()
    if identity['source_dirty']: raise ValueError('freeze source/harness before cause diagnostic')
    out = runner.owned(args.out); out.mkdir(parents=True)
    prior = json.loads((Path(args.campaign)/'phase7-sqlite-disposable-init-10000-v1-candidate/receipt.json').read_text())
    fixture = prior['fixture']
    observer = archive(ROOT/'target/phase7-agent/sqlite-close-observer-identity.dylib', ROOT, runner)
    driver = archive(ROOT/'core/target/release/examples/benchmark_init', ROOT, runner)
    runner.write_json(out/'declaration.json', {
        'kind': 'allocation-descriptor-identity-cause-v1', 'identity': identity,
        'observer': observer, 'driver': driver, 'fixture': fixture,
        'orchestrator_sha256': runner.digest(Path(__file__)),
        'purpose': 'match real allocation fd/device/inode to actual close; separate transfer/scratch close',
        'speed_admission': 'NOT_APPLICABLE', 'samples': 1, 'budget_ns': 15_000_000_000,
        'verification': 'SKIPPED; original qualified namespace proof plus expected-root cross-check; cause only',
    })
    lock = (runner.RESULTS/'phase7-sqlite.lock').open('a+b')
    import fcntl
    fcntl.flock(lock, fcntl.LOCK_EX|fcntl.LOCK_NB)
    record = {'kind': 'allocation-descriptor-identity-cause-v1', 'status': 'INCOMPLETE',
              'admission': 'NOT_APPLICABLE', 'sample_count': 0, 'identity': identity,
              'observer': observer, 'driver': driver, 'verification': 'SKIPPED'}
    try:
        helper = cold_native.build(ROOT, out, invoke)
        scratch = out/'scratch'; scratch.mkdir(); db = out/'store.sqlite'
        start = time.monotonic_ns()
        record['cold'] = cold_native.attest(fixture['source'], helper, out, 15_000_000_000, invoke, ROOT)
        if record['cold']['status'] != 'PASS': record['status'] = 'INELIGIBLE'; return
        env = {**os.environ, 'LAYERFS_CONSTRUCTION_WORKERS': '1', 'LAYERFS_HISTORY_CURSOR_KEY': '28'*32,
               'TMPDIR': str(scratch), 'DYLD_INSERT_LIBRARIES': observer['path'],
               'LAYERFS_CAUSE_VFS_LOG': str(out/'vfs.json'), 'LAYERFS_CLOSE_OBSERVER_OUTPUT': str(out/'close.json')}
        command = [driver['path'], fixture['source'], str(db), str(scratch), 'phase7-sqlite-disposable-init-10000-v1', 'disposable']
        result = invoke(command, out, 'driver', 15_000_000_000-(time.monotonic_ns()-start), env, ROOT)
        record['run'] = result; record['sample_count'] = 1; record['command_wall_ns'] = time.monotonic_ns()-start
        if result['exit_code'] or result['timed_out']: record['status'] = 'FAIL'; return
        if result['child']['root'] != prior['performance']['child']['root']: raise ValueError('cause original root mismatch')
        stderr = (out/'driver.stderr').read_text()
        match = re.search(r'allocation_source: Some\(AllocationIdentity \{ descriptor: (\d+), device: (\d+), inode: (\d+), logical_bytes: (\d+) \}\)', stderr)
        if match is None: raise ValueError('actual allocation descriptor identity missing')
        source = dict(zip(('descriptor','device','inode','logical_bytes'), map(int, match.groups())))
        observed = json.loads((out/'close.json').read_text()); vfs = json.loads((out/'vfs.json').read_text())
        if observed['descriptor_identity_omitted'] or vfs['live_files'] or vfs['close_errors']: raise ValueError('cause close coverage incomplete')
        matches = [row for row in observed['descriptor_identities'] if all(row[k] == source[k] for k in ('descriptor','device','inode'))]
        if len(matches) != 1 or matches[0]['last_result'] != 0: raise ValueError('exact allocation descriptor close not observed/successful')
        record.update(status='DIAGNOSTIC', allocation_source=source, allocation_close=matches[0],
                      transfer_ns=int(re.search(r'allocation_transfer_ns: (\d+)',stderr).group(1)),
                      scratch_close_ns=int(re.search(r'allocation_scratch_close_ns: (\d+)',stderr).group(1)),
                      source_close_ns=int(re.search(r'allocation_source_close_ns: (\d+)',stderr).group(1)),
                      preallocation={key:int(re.search(key+r': (\d+)',stderr).group(1)) for key in ('preallocation_calls','preallocation_bytes','preallocation_ns','preallocation_close_ns')},
                      close=observed, vfs=vfs, original_root_match=True,
                      cleanup='PASS' if not list(scratch.iterdir()) and not list(out.glob('.layerfs-allocation-*')) else 'FAIL')
        print(json.dumps({k: record[k] for k in ('status','allocation_source','allocation_close','transfer_ns','scratch_close_ns','source_close_ns','preallocation')}))
    finally:
        runner.write_json(out/'receipt.json', record); runner.manifest_run(out); lock.close()


if __name__ == '__main__': main()
