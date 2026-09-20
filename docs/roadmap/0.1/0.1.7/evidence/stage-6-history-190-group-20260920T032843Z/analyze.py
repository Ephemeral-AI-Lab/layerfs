#!/usr/bin/env python3
"""Derive a fresh named report from retained raw history receipts."""
import collections
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent

def read(path):
    return json.loads(path.read_text())

def trace(path):
    return [json.loads(line) for line in path.read_text().splitlines()]

def arm(case, name):
    run = HERE / 'runs' / f'{name}-{case}'
    phase = read(run/'raw/phases-perf.json')
    tree = read(run/'raw/timing.json')
    receipt = read(run/'perf-receipt.json')
    rows = trace(run/'trace-perf.jsonl')
    values = {r['key']:r['value'] for r in rows}
    totals = collections.Counter()
    def walk(node):
        totals[node['name']] += node['elapsed_ns']
        for child in node['children']:
            walk(child)
    walk(tree)
    states = tree['children']
    assert sum(s['elapsed_ns'] for s in states) == phase['operation_ns']
    partition = collections.Counter()
    for state in states:
        for child in state['children']:
            partition[child['name']] += child['elapsed_ns']
    partition['state.uninstrumented'] = phase['operation_ns']-sum(partition.values())
    assert sum(partition.values()) == phase['operation_ns']
    counters = collections.Counter()
    for r in rows:
        if r.get('numeric') and r['key'].startswith('history.state.') and (r['kind']=='counter' or '.provider.' in r['key']) and not any(x in r['key'] for x in ['max_', 'maximum', 'peak', 'watermark']):
            counters[r['key'].split('.',3)[3]] += r['value']
    result = {'operation_ns':phase['operation_ns'], 'span_ns':dict(totals),
              'partition_ns':dict(partition), 'counters':dict(counters),
              'storage_ns':sum(totals[x] for x in ['storage.begin','storage.accept_loop','storage.finish']),
              'complete_command_ns':receipt['wall_ns'],
              'cpu_ns':phase['cpu_user_ns']+phase['cpu_system_ns'],
              'lifetime_rss_bytes':phase['process_peak_rss_bytes'],
              'heap_peak_incremental_bytes':values['heap.peak_incremental_bytes'],
              'allocated_bytes':values['space.allocated_bytes'],
              'apparent_bytes':values['space.apparent_bytes'],
              'roots':{k:v for k,v in values.items() if k.endswith('.root')},
              'nonpassing_gates':[r for r in rows if r['kind']=='gate' and not str(r['value']).startswith('PASS|')],
              'idle_percent':receipt['pre_observation']['last_cpu_idle_percent'],
              'state_operation_ns':{s['name']:s['elapsed_ns'] for s in states},
              'codec_cpu_ns':None}
    if (run/'verify-receipt.json').exists():
        result['verify_ns']=read(run/'raw/phases-verify.json')['verification_ns']
        result['verify_command_ns']=read(run/'verify-receipt.json')['wall_ns']
        result['verify_counters']={r['key']:r['value'] for r in trace(run/'raw/trace.jsonl') if r['key'].startswith('verify.') and r['kind']=='counter'}
    return result

result={}
for case in ['history-stride10','history-stride3']:
    if not all((HERE/'runs'/f'{a}-{case}'/'perf-receipt.json').exists() for a in ['baseline','candidate']):
        continue
    a,b=arm(case,'baseline'),arm(case,'candidate')
    assert a['roots']==b['roots']
    keys=['operation_ns','storage_ns','complete_command_ns','cpu_ns','lifetime_rss_bytes','heap_peak_incremental_bytes','allocated_bytes','apparent_bytes']
    result[case]={'baseline':a,'candidate':b,'baseline_minus_candidate':{k:a[k]-b[k] for k in keys},'canonical_roots_equal':True,'counter_differences':{k:[a['counters'].get(k),b['counters'].get(k)] for k in sorted(a['counters']|b['counters']) if a['counters'].get(k)!=b['counters'].get(k)}}
with (HERE/sys.argv[1]).open('x') as f:
    json.dump(result,f,indent=2);f.write('\n')
for case,data in result.items():
    print(case,json.dumps(data['baseline_minus_candidate']), 'counter differences',data['counter_differences'])
