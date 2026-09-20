#!/usr/bin/env python3
"""Independent arithmetic over completed raw receipts; no product execution."""
from collections import defaultdict
import hashlib
import json
from pathlib import Path
import re

campaign = Path(__file__).resolve().parents[1]
results = {}
for run in sorted((campaign / 'runs').iterdir()):
    receipt_path = run / 'perf-receipt.json'
    if not receipt_path.exists():
        continue
    receipt = json.loads(receipt_path.read_text())
    raw = run / 'raw'
    if not (raw / 'timing.json').exists():
        results[run.name] = {'execution_status': receipt['execution_status'], 'missing': 'timing.json'}
        continue
    timing = json.loads((raw / 'timing.json').read_text())
    phases = json.loads((raw / 'phases-perf.json').read_text())
    trace = [json.loads(line) for line in (run / 'trace-perf.jsonl').read_text().splitlines()]
    totals, fs_totals, work = defaultdict(int), defaultdict(int), defaultdict(int)
    states = []
    for state in timing['children']:
        named = {node['name']: node['elapsed_ns'] for node in state['children']}
        named['state.residual'] = state['elapsed_ns'] - sum(named.values())
        assert named['state.residual'] >= 0
        for name, elapsed in named.items():
            totals[name] += elapsed
        filesystem = next(node for node in state['children'] if node['name'] == 'filesystem')
        fs_named = {node['name']: node['elapsed_ns'] for node in filesystem['children']}
        fs_named['filesystem.residual'] = filesystem['elapsed_ns'] - sum(fs_named.values())
        assert fs_named['filesystem.residual'] >= 0
        for name, elapsed in fs_named.items():
            fs_totals[name] += elapsed
        states.append({'name': state['name'], 'elapsed_ns': state['elapsed_ns'], 'spans': named, 'filesystem_spans': fs_named})
    operation = sum(state['elapsed_ns'] for state in timing['children'])
    assert operation == phases['operation_ns'] == sum(totals.values())
    assert totals['filesystem'] == sum(fs_totals.values())
    roots = {}
    for row in trace:
        match = re.fullmatch(r'history\.state\.(\d+)\.(.+)', row['key'])
        if match:
            ordinal, name = match.groups()
            if name == 'root':
                roots[ordinal] = row['value']
            elif row.get('numeric'):
                work[name] += row['value']
    small_hashes = {}
    for name in ('timing.json', 'phases-perf.json'):
        digest = hashlib.sha256((raw / name).read_bytes()).hexdigest()
        assert receipt['artifacts'][name] == digest
        small_hashes[name] = digest
    digest = hashlib.sha256((run / 'trace-perf.jsonl').read_bytes()).hexdigest()
    assert receipt['artifacts']['trace.jsonl'] == digest
    small_hashes['trace-perf.jsonl'] = digest
    results[run.name] = dict(operation_ns=operation, root_ns=timing['elapsed_ns'], state_count=len(states),
        root_minus_children_ns=timing['elapsed_ns'] - operation, spans=dict(totals), filesystem_spans=dict(fs_totals),
        work=dict(work), roots=roots, states=states, phases=phases, raw_hashes=small_hashes,
        command_wall_ns=receipt['wall_ns'], execution_status=receipt['execution_status'],
        nonpassing=[row for row in trace if row['kind']=='gate' and not str(row['value']).startswith('PASS|')])
comparisons = {}
for case in ('history-stride10', 'history-stride3'):
    baseline, candidate = results.get('baseline-'+case), results.get('candidate-'+case)
    if baseline and candidate and 'roots' in baseline and 'roots' in candidate:
        assert baseline['roots'] == candidate['roots'], 'canonical state root mismatch'
        comparisons[case] = {'canonical_roots_equal': True, 'root_count': len(baseline['roots']),
            'operation_delta_ns': candidate['operation_ns'] - baseline['operation_ns'],
            'spans_delta_ns': {key: candidate['spans'][key]-value for key,value in baseline['spans'].items()},
            'work_delta': {key: candidate['work'][key]-value for key,value in baseline['work'].items()}}
print(json.dumps({'runs':results,'comparisons':comparisons}, indent=2))
