import os,sys,json,fcntl,hashlib
from pathlib import Path
primary=Path('/Users/yifanxu/Ephemeral-AI-Lab/layerfs');root=Path('/Users/yifanxu/.codex/worktrees/init-entry-performance/layerfs');ref=Path('/Users/yifanxu/.codex/worktrees/cluster-one-scaling-reference/layerfs/benchmark-results/fs-bench-pro/cluster-one-reference-attribution-corrected-20261006/receipt.json')
sys.path.insert(0,str(root/'core/benchmark/fs-bench-pro'))
from families.phase7_sqlite import invoke
(root/'benchmark_agent_report.md').read_text();out=root/'benchmark-results/fs-bench-pro/space-scaling-v2-physical-io-20261006';out.mkdir()
lock=(out.parent/'phase7-sqlite.lock').open('a+b');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
p=root/'benchmark-results/fs-bench-pro/space-scaling-20261006-phase7-sqlite-init-100000-space-scaling-v2-candidate/receipt.json';prior=json.loads(p.read_text());baseline=json.loads(ref.read_text());library=baseline['binaries']['vfs_library'];assert hashlib.sha256(Path(library['path']).read_bytes()).hexdigest()==library['sha256'];driver=prior['build']['binaries']['benchmark_init'];fixture=prior['fixture'];env={**os.environ,'LAYERFS_CONSTRUCTION_WORKERS':'1','LAYERFS_HISTORY_CURSOR_KEY':'28'*32,'DYLD_INSERT_LIBRARIES':library['path'],'LAYERFS_CAUSE_VFS_LOG':str(out/'vfs.json')}
r={'kind':'physical-I/O cause diagnostic of exact qualified v2 public Init binary','admission_eligible':False,'cache':'uncontrolled/instrumented; no timing claim','source_receipt':str(p),'library':library,'sample_count':0}
try:
 r['run']=invoke([driver['path'],fixture['source'],str(out/'store.sqlite'),'phase7-sqlite-init-100000-space-scaling-v2','durable','30'],out,'driver',30_000_000_000,env,root);r['sample_count']=1
 assert r['run']['exit_code']==0 and not r['run']['timed_out'];assert r['run']['child']['root']==prior['performance']['child']['root']
 r['proof']=invoke([prior['build']['binaries']['verify_namespace']['path'],str(out/'store.sqlite'),str(out/'store.sqlite'),prior['performance']['child']['root'],'41'*16,fixture['manifest'],fixture['manifest_sha256'],'durable'],out,'verifier',19_000_000_000,{k:v for k,v in env.items() if k not in ['DYLD_INSERT_LIBRARIES','LAYERFS_CAUSE_VFS_LOG']},root)
 assert r['proof']['exit_code']==0 and not r['proof']['timed_out'];r['status']='DIAGNOSTIC_COMPLETE';print(json.dumps(json.loads((out/'vfs.json').read_text())['files'],indent=2))
except Exception as e:r['status']='FAIL';r['reason']=str(e);raise
finally:(out/'receipt.json').write_text(json.dumps(r,indent=2)+'\n');lock.close();print('RECEIPT',out/'receipt.json')
