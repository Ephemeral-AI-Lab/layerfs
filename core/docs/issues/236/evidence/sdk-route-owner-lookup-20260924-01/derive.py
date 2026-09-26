#!/usr/bin/env python3
"""Derive partial cause evidence from retained LFT1 after one dropped route event."""
import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parent
identity = json.loads((root / 'identities.json').read_text())
run = f"{identity['telemetry_run']:032x}"
def events(path, role):
    found = []
    for line in path.read_text(errors='replace').splitlines():
        if 'LFT1 ' in line:
            event = json.loads(line.partition('LFT1 ')[2])
            if event.get('run') == run and event.get('role') == role:
                found.append(event)
    return found
host = events(root / 'raw-host.log', 1)
sandbox = (root / 'primary-sandbox-id.txt').read_text().strip()
daemon = events(root / f'layerfs-{sandbox}.stderr', 2)
def one(source, key=None, label=None):
    found = [e for e in source if e.get('kind') == 'operation'
             and (key is None or e.get('key') == key)
             and (label is None or e.get('timing', {}).get('name') == label)]
    if len(found) != 1:
        raise ValueError(f'expected one {key} {label}, got {len(found)}')
    return found[0]
summary = [e for e in host if e.get('kind') == 'run-summary']
assert len(summary) == 1 and summary[0]['dropped'] == 1 and summary[0]['failed'] == 0
assert not [e for e in host if e.get('key') == 1000]
assert (root / 'exit-status.txt').read_text().strip() == '0'
assert 'test result: ok. 1 passed' in (root / 'raw-host.log').read_text()
phases = []
for name, key, daemon_name in [('mount',1002,'WorkspaceOpen'),('exec',1003,'WorkspaceExec'),
                               ('commit',1004,'WorkspaceCommit'),('unmount',1005,'WorkspaceUnmount')]:
    sdk = one(host, key=key)
    start = sdk['opened_ns']; stop = start + sdk['timing']['elapsed_ns']
    inside = [e for e in host if start <= e.get('opened_ns', -1) < stop]
    port = one(inside, key=2001, label='owner.docker_port')
    hello = one(inside, key=2002, label='owner.hello')
    control = one(daemon, label=daemon_name)
    assert sdk['success'] and port['success'] and hello['success'] and control['success']
    phases.append({'name':name,'sdk_wall_ns':sdk['timing']['elapsed_ns'],
                   'docker_port_ns':port['timing']['elapsed_ns'],
                   'hello_ns':hello['timing']['elapsed_ns'],
                   'daemon_control_wall_ns':control['timing']['elapsed_ns']})
report = {'kind':'sdk-owner-lookup-cause-diagnostic',
          'functional_status':'PASS','diagnostic_status':'INCOMPLETE_MISSING_ROUTE',
          'performance_admission':'INELIGIBLE','admission_eligible':False,
          'cache_contract':'uncontrolled OS/source cache; no cold or warm claim',
          'source_commit':identity['source_commit'],'telemetry_run':identity['telemetry_run'],
          'host_run_summary':summary[0], 'phases':phases,
          'four_call_totals_ns':{field:sum(p[field] for p in phases)
                                 for field in ('sdk_wall_ns','docker_port_ns','hello_ns','daemon_control_wall_ns')}}
(root / 'partial-report.json').open('x').write(json.dumps(report, indent=2, sort_keys=True)+'\n')
manifest = {p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(root.iterdir())
            if p.is_file() and p.name != 'manifest.json'}
(root / 'manifest.json').open('x').write(json.dumps(manifest, indent=2, sort_keys=True)+'\n')
print(json.dumps({'status':report['diagnostic_status'],'files':len(manifest),'totals':report['four_call_totals_ns']}))
