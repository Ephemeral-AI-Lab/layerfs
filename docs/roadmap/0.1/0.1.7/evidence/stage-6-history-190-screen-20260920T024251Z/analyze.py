#!/usr/bin/env python3
"""Re-derive the one-second candidate decision; do not modify raw receipts."""
import collections
import csv
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent

def load(path):
    return json.loads(path.read_text())

def arm_result(arm):
    run = HERE / 'runs' / f'{arm}-history-stride10'
    timing = load(run / 'raw/timing.json')
    phases = load(run / 'raw/phases-perf.json')
    totals = collections.Counter()
    def walk(node):
        totals[node['name']] += node['elapsed_ns']
        for child in node['children']:
            walk(child)
    walk(timing)
    states = timing['children']
    assert sum(s['elapsed_ns'] for s in states) == phases['operation_ns']
    trace = [json.loads(line) for line in (run / 'trace-perf.jsonl').read_text().splitlines()]
    values = {r['key']: r['value'] for r in trace}
    provider = collections.Counter()
    for row in trace:
        if row.get('numeric') and '.provider.' in row['key']:
            provider[row['key'].split('.provider.', 1)[1]] += row['value']
    direct = collections.Counter()
    for state in states:
        for child in state['children']:
            direct[child['name']] += child['elapsed_ns']
    direct['state.uninstrumented'] = phases['operation_ns'] - sum(direct.values())
    assert sum(direct.values()) == phases['operation_ns']
    verify = load(run / 'raw/phases-verify.json')
    perf_receipt = load(run / 'perf-receipt.json')
    allocation = values['space.allocated_bytes']
    return {
        'operation_ns': phases['operation_ns'], 'span_totals_ns': dict(totals),
        'operation_partition_ns': dict(direct), 'provider': dict(provider),
        'complete_command_ns': perf_receipt['wall_ns'],
        'cpu_ns': phases['cpu_user_ns'] + phases['cpu_system_ns'],
        'lifetime_peak_rss_bytes': phases['process_peak_rss_bytes'],
        'heap_peak_incremental_bytes': values['heap.peak_incremental_bytes'],
        'allocated_bytes': allocation, 'apparent_bytes': values['space.apparent_bytes'],
        'allocated_limit_bytes': 49344512, 'allocated_over_limit_bytes': allocation - 49344512,
        'storage_target': 'TARGET_MISS' if allocation > 49344512 else 'PASS',
        'verify_ns': verify['verification_ns'], 'verify_limit_ns': 10000000000,
        'verify_target': 'PASS' if verify['verification_ns'] <= 10000000000 else 'TARGET_MISS',
        'verify_command_ns': load(run / 'verify-receipt.json')['wall_ns'],
        'store_sha256': perf_receipt['artifacts']['sample.sqlite'],
        'roots': {k:v for k,v in values.items() if k.startswith('history.state.') and k.endswith('.root')},
        'nonpassing_perf_gates': [r for r in trace if r['kind'] == 'gate' and not str(r['value']).startswith('PASS|')],
        'preflight_idle_percent': perf_receipt['pre_observation']['last_cpu_idle_percent'],
    }, states

if __name__ == '__main__':
    baseline, bs = arm_result('baseline')
    candidate, cs = arm_result('candidate')
    gain = baseline['operation_ns'] - candidate['operation_ns']
    phase_gain = baseline['span_totals_ns']['zero_count'] - candidate['span_totals_ns']['zero_count']
    result = {'baseline': baseline, 'candidate': candidate,
              'threshold_ns': 1000000000, 'operation_reduction_ns': gain,
              'zero_count_reduction_ns': phase_gain,
              'outside_zero_count_reduction_ns': gain-phase_gain,
              'shortfall_ns': 1000000000-gain, 'decision': 'REJECT_BELOW_ONE_SECOND',
              'causal_timing_saving_proven': False,
              'cache_admission': 'INELIGIBLE', 'stride3': 'NOT_RUN_REJECTED_CANDIDATE'}
    assert gain < result['threshold_ns']
    assert baseline['roots'] == candidate['roots'] and len(baseline['roots']) == 17
    assert baseline['store_sha256'] == candidate['store_sha256']
    with (HERE / 'results.json').open('x') as f:
        json.dump(result, f, indent=2); f.write('\n')
    with (HERE / 'states.csv').open('x', newline='') as f:
        out = csv.writer(f)
        out.writerow(['arm','state','ordinal','operation_ns','zero_count_ns','filesystem_ns','state_uninstrumented_ns'])
        for arm, states in [('baseline',bs),('candidate',cs)]:
            for i, state in enumerate(states):
                children = {x['name']:x for x in state['children']}
                fs = children['filesystem']
                zero = next((x['elapsed_ns'] for x in fs['children'] if x['name']=='zero_count'),0)
                out.writerow([arm,state['name'],min(1+i*10,157),state['elapsed_ns'],zero,fs['elapsed_ns'],state['elapsed_ns']-sum(x['elapsed_ns'] for x in state['children'])])
    print(json.dumps({k:v for k,v in result.items() if k not in ['baseline','candidate']},indent=2))
