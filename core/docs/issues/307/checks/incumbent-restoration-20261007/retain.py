"""Copy compact receipts and derive the ledger from retained raw runs; no execution."""
import json,hashlib,shutil,sys
from pathlib import Path
primary=Path('/Users/yifanxu/Ephemeral-AI-Lab/layerfs');folder=Path(__file__).resolve().parent
sys.path.insert(0,str(primary/'core/benchmark/fs-bench-pro'))
from shared.cluster_one_control import VALUES
campaign='incumbent-restoration-20261007'
roots={'init':Path('/Users/yifanxu/.codex/worktrees/init-entry-performance/layerfs'),'history':Path('/Users/yifanxu/.codex/worktrees/save-vfs-amplification/layerfs')}
sel=json.loads((folder/'selection.json').read_text())
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
def keep(kind,case,arm,selection=''):
    run=roots[kind]/'benchmark-results/fs-bench-pro'/f'{campaign}{selection}-{case}-{arm}'
    wrapper=roots[kind]/'benchmark-results/fs-bench-pro'/f'{campaign}{selection}-wrapper'/f'{case}-{arm}.json'
    if not (run/'receipt.json').exists():return None,None
    dst=folder/('raw'+selection)/case/arm;dst.mkdir(parents=True,exist_ok=True)
    for name in ('receipt.json','manifest.json'):
        if (run/name).exists():shutil.copy2(run/name,dst/name)
    if wrapper.exists():shutil.copy2(wrapper,dst/'invocation.json')
    return json.loads((run/'receipt.json').read_text()),{'run':str(run),'receipt_sha256':sha(run/'receipt.json'),'compact':str((dst/'receipt.json').relative_to(primary))}
ledger={'campaign':campaign,'source':None,'init':[],'history':[]}
for c in sel['init_cases']:
    r,where=keep('init',c['id'],'candidate');n=int(c['fixture'].split('-')[1]);control_ns,control_b=VALUES[c['profile']][n]
    row={'case':c['id'],'profile':c['profile'],'files':n,'control_ns':control_ns,'control_bytes':control_b}
    if r is None:row['status']='NOT_RUN'
    else:
        ledger['source']=r['measured_source_commit'];t,b=r.get('comparison_ns'),r.get('storage_bytes');child=(r.get('performance') or {}).get('child') or {}
        row.update(status=r['status'],candidate_ns=t,candidate_bytes=b,speed_gate='PASS' if t is not None and 10*t<=11*control_ns else 'FAIL',allocation_gate='PASS' if b is not None and b<=control_b else 'FAIL',
            proof=r['verification_status'],cache=r['cache_status'],cleanup=r['cleanup']['status'],command_wall_ns=r.get('command_wall_ns'),verification_wall_ns=r.get('verification_wall_ns'),
            binary_sha256=r['build']['binaries']['benchmark_init']['sha256'],product_seal=r['product_seal'],phases={k:child.get(k) for k in ('bootstrap_ns','init_ns','checkpoint_ns','close_ns')},**where)
    ledger['init'].append(row)
def pair(c,selection):
    row={'arms':{}}
    for arm in ('baseline','candidate'):
        r,where=keep('history',c['id'],arm,selection)
        if r is None:row['arms'][arm]={'status':'NOT_RUN'};continue
        res=r.get('residency') or {}
        row['source']=r['measured_source_commit']
        row['arms'][arm]={'status':r['status'],'sample_count':r['sample_count'],'comparison_ns':r.get('comparison_ns'),'storage_bytes':r.get('storage_bytes'),'proof':r['verification_status'],'cache':r['cache_status'],'cleanup':r['cleanup']['status'],
            'command_wall_ns':r.get('command_wall_ns'),'command_budget_ns':r['command_budget_ns'],'verification_wall_ns':r.get('verification_wall_ns'),'verification_budget_ns':r['verification_budget_ns'],
            'source_residency':{k:res.get(k) for k in ('resident_first','resident_after','invalidated_files','status')},'reason':r.get('reason'),**where}
    a,b=row['arms']['baseline'],row['arms']['candidate']
    if a.get('comparison_ns') and b.get('comparison_ns'):
        row['speed_gate']='PASS' if 10*b['comparison_ns']<=11*a['comparison_ns'] else 'FAIL';row['time_delta_percent']=100*(b['comparison_ns']-a['comparison_ns'])/a['comparison_ns']
        row['allocation_gate']='PASS' if b['storage_bytes']<=c['allocation_ceiling'] else 'FAIL'
        ra=json.loads(Path(a['run']+'/receipt.json').read_text());rb=json.loads(Path(b['run']+'/receipt.json').read_text())
        row['roots_equal']=ra['performance']['child'].get('roots')==rb['performance']['child'].get('roots')
    return row
for c in sel['history_cases']:
    ledger['history'].append({'case':c['id'],'profile':c['profile'],'states':c['states'],'allocation_ceiling':c['allocation_ceiling'],'first_selection':pair(c,''),'second_selection':pair(c,'-r2')})
log=folder/'precondition.jsonl'
ledger['preconditioning']=[json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
(folder/'ledger.json').write_text(json.dumps(ledger,indent=2)+'\n');print('init',[r['status'] for r in ledger['init']]);print('history',[[(r[k]['arms']['baseline']['status'],r[k]['arms']['candidate']['status'],r[k].get('speed_gate')) for k in ('first_selection','second_selection')] for r in ledger['history']])
