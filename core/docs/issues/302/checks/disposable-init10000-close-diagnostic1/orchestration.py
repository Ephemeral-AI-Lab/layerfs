import sys,json,os,fcntl,time,hashlib,shutil
from pathlib import Path
root=Path.cwd();sys.path.insert(0,str(root/'core/benchmark/fs-bench-pro'))
import runner
from families.phase7_sqlite import invoke,archive
from shared import cold_native
out=root/'benchmark-results/fs-bench-pro/issue302-disposable-init10000-close-diagnostic1';out.mkdir()
identity=runner.identities();assert not identity['source_dirty']
prior=root/'benchmark-results/fs-bench-pro/issue302-disposable-init-campaign1'
records={a:json.loads((prior/f'phase7-sqlite-disposable-init-10000-v1-{a}/receipt.json').read_text()) for a in ('baseline','candidate')}
observer=archive(root/'target/phase7-agent/sqlite-close-observer.dylib',root,runner)
source_seals={str(p.relative_to(root)):runner.digest(p) for p in [root/'core/benchmark/fs-bench-pro/diagnostics/sqlite_close_observer.c',root/'core/benchmark/fs-bench-pro/diagnostics/sqlite_vfs_observer.c']}
runner.write_json(out/'declaration.json',{'kind':'count-driven-close-mechanism-diagnostic-v1','identity':identity,'source_seals':source_seals,'observer':observer,'samples_per_arm':1,'case':'Init10000','gate':'NOT_APPLICABLE; historical gate FAIL preserved','purpose':'attribute VFS write/sync/close, SQLite close and OS-close term; no retry/admission speed number','performance_budget_ns':15000000000,'verification':'SKIPPED; same archived vehicles, original matching roots retained; diagnostic root cross-check only'})
locks=[]
for p in [runner.RESULTS/'phase7-sqlite.lock',root/'target/phase7-baseline/layerfs/target/phase7-sqlite.lock']:
 p.parent.mkdir(parents=True,exist_ok=True);h=p.open('a+b');fcntl.flock(h,fcntl.LOCK_EX|fcntl.LOCK_NB);locks.append(h)
try:
 for arm in ('baseline','candidate'):
  folder=out/arm;folder.mkdir();prior_row=records[arm];fixture=prior_row['fixture']
  scratch=folder/'scratch';scratch.mkdir();db=folder/'store.sqlite';history=folder/'history.sqlite'
  binary=prior_row['build']['binaries']['benchmark_init' if arm=='candidate' else 'sqlite_reference_init']
  assert runner.digest(Path(binary['path']))==binary['sha256']
  env={**os.environ,'LAYERFS_CONSTRUCTION_WORKERS':'1','LAYERFS_HISTORY_CURSOR_KEY':'28'*32,'TMPDIR':str(scratch),'DYLD_INSERT_LIBRARIES':observer['path'],'LAYERFS_CAUSE_VFS_LOG':str(folder/'vfs.json'),'LAYERFS_CLOSE_OBSERVER_OUTPUT':str(folder/'close.json')}
  helper=cold_native.build(root,folder,invoke)
  start=time.monotonic_ns();cold=cold_native.attest(fixture['source'],helper,folder,15000000000,invoke,root)
  record={'kind':'count-driven-close-mechanism-diagnostic-v1','arm':arm,'status':'DIAGNOSTIC','admission':'NOT_APPLICABLE','identity':identity,'observer':observer,'fixture':fixture,'binary':binary,'cold':cold,'verification':'SKIPPED','sample_count':0}
  if cold['status']!='PASS':record['status']='INELIGIBLE'
  else:
   command=[binary['path'],fixture['source'],str(db),str(scratch if arm=='candidate' else history),'phase7-sqlite-disposable-init-10000-v1']+(['disposable'] if arm=='candidate' else [])
   print('START',arm,flush=True)
   result=invoke(command,folder,'driver',15000000000-(time.monotonic_ns()-start),env,root if arm=='candidate' else root/'target/phase7-baseline/layerfs')
   record['run']=result;record['sample_count']=1;record['command_wall_ns']=time.monotonic_ns()-start
   child=result['child'];record['matching_original_root']=isinstance(child,dict) and child.get('root')==prior_row['performance']['child']['root']
   if result['exit_code']!=0 or result['timed_out'] or not record['matching_original_root']:record['status']='FAIL'
   else:
    vfs=json.loads((folder/'vfs.json').read_text());close=json.loads((folder/'close.json').read_text());record.update(vfs=vfs,close=close)
    if vfs['live_files'] or vfs['close_errors'] or vfs['unknown_sync_flags']:record['status']='INCOMPLETE'
    print('END',arm,json.dumps({'status':record['status'],'close':close,'vfs':vfs}),flush=True)
  runner.write_json(folder/'receipt.json',record);runner.manifest_run(folder)
finally:
 for h in reversed(locks):h.close()
 runner.manifest_run(out)
