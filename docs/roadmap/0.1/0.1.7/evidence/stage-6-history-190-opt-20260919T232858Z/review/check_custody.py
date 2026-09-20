#!/usr/bin/env python3
"""Independent read-only final artifact/source/LOC custody check under locks."""
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

campaign = Path(__file__).resolve().parents[1]
repo = campaign.parents[5]
harness = repo / 'core/benchmark/fs-bench-pro-storage-content'
sys.path.insert(0, str(harness / 'shared'))
import receipt

def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()

with contextlib.ExitStack() as stack:
    for path in dict.fromkeys([(Path(os.environ.get('TMPDIR','/tmp'))/'layerfs-infra-measurement.lock').resolve(), Path('/tmp/layerfs-infra-measurement.lock').resolve()]):
        handle = stack.enter_context(path.open('a'))
        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
    stack.enter_context(receipt.measurement_lock(harness / '.measurement.lock'))
    identities = {arm:json.loads((campaign / f'{arm}-identity.json').read_text()) for arm in ('baseline','candidate')}
    baseline, candidate = identities.values()
    assert baseline['harness_files'] == candidate['harness_files']
    assert baseline['harness_lock_sha256'] == candidate['harness_lock_sha256']
    assert baseline['core_lock_sha256'] == candidate['core_lock_sha256']
    for arm, identity in identities.items():
        assert digest(Path(identity['binary'])) == identity['binary_sha256']
    for key in ('harness_files','product_files'):
        for path, expected in candidate[key].items():
            assert digest((harness if key == 'harness_files' else repo)/path) == expected, path
    changed = [name for name in baseline['product_files'] if baseline['product_files'][name] != candidate['product_files'][name]]
    assert changed == ['core/crates/layerfs-content/src/filesystem/update.rs']
    assert digest(repo/'core/Cargo.lock') == candidate['core_lock_sha256']
    assert digest(harness/'Cargo.lock') == candidate['harness_lock_sha256']
    assert digest(repo/'tools/production_loc.py') == candidate['counter_sha256']
    loc = json.loads(subprocess.check_output([sys.executable,'tools/production_loc.py','--json'],cwd=repo,text=True))
    assert loc == candidate['production_loc']
    stores, verification, nonpassing = {}, {}, {}
    for run in sorted((campaign/'runs').iterdir()):
        raw = run/'raw'
        perf = json.loads((run/'perf-receipt.json').read_text())
        verify = json.loads((run/'verify-receipt.json').read_text())
        for name, expected in perf['artifacts'].items():
            path = run/'trace-perf.jsonl' if name == 'trace.jsonl' else raw/name
            assert digest(path) == expected, str(path)
        for name, expected in verify['artifacts'].items():
            assert digest(raw/name) == expected, name
        store = digest(raw/'sample.sqlite')
        stores[run.name] = {'sha256':store,'apparent_bytes':(raw/'sample.sqlite').stat().st_size,'allocated_bytes':(raw/'sample.sqlite').stat().st_blocks*512}
        assert store == perf['artifacts']['sample.sqlite'] == verify['artifacts']['sample.sqlite']
        phases = json.loads((raw/'phases-verify.json').read_text())
        limit = 10_000_000_000 if run.name.endswith('10') else 20_000_000_000
        verification[run.name] = {'work_ns':phases['verification_ns'],'command_ns':verify['wall_ns'],'target_ns':limit,'target_status':'PASS' if phases['verification_ns']<=limit else 'TARGET_MISS','hard_budget_status':'PASS' if verify['wall_ns']<=60_000_000_000 else 'FAIL','exit_code':verify['exit_code']}
        trace=[json.loads(line) for line in (raw/'trace.jsonl').read_text().splitlines()]
        nonpassing[run.name]=[row for row in trace if row['kind']=='gate' and not str(row['value']).startswith('PASS|')]
    store_equality={case:stores['baseline-'+case]['sha256']==stores['candidate-'+case]['sha256'] for case in ('history-stride10','history-stride3')}
    assert all(store_equality.values())
    print(json.dumps({'status':'PASS','changed_product_files':changed,'harness_matches':True,'product_current_matches_candidate':True,'binary_hashes':{arm:value['binary_sha256'] for arm,value in identities.items()},'stores':stores,'store_equality':store_equality,'verification':verification,'production_loc':loc,'nonpassing':nonpassing},indent=2))
