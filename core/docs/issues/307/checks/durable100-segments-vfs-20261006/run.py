import pathlib,sys,os,fcntl,json,time,re,subprocess,hashlib

root=pathlib.Path('/Users/yifanxu/.codex/worktrees/init-entry-performance/layerfs')
primary=pathlib.Path('/Users/yifanxu/Ephemeral-AI-Lab/layerfs')
sys.path.insert(0,str(root/'core/benchmark/fs-bench-pro'))
import runner
from families.phase7_sqlite import invoke,archive
os.chdir(root)
(root/'benchmark_agent_report.md').read_text()
identity=runner.identities()
assert not identity['source_dirty']
assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()=='fb7f3f47734eb8cd4d68b8e965a24929c8332785'
out=root/'benchmark-results/fs-bench-pro/durable100-segments-vfs-count-20261006'
assert not out.exists();out.mkdir()
refpath=root/'benchmark-results/fs-bench-pro/durable100-segments-20261006-phase7-sqlite-init-100-acquisition-payload-segments-v1-candidate/receipt.json'
prior=json.loads(refpath.read_text());fixture=prior['fixture'];assert runner.digest(fixture['manifest'])==fixture['manifest_sha256']
record={'kind':'Durable100 payload-segment per-port VFS and native I/O cause diagnostic','status':'INCOMPLETE','identity':identity,'product_seal':prior['product_seal'],'admission_eligible':False,'cache':'uncontrolled/instrumented; no numerical speed/RSS/rate admission','case':fixture['case'],'fixture':fixture,'source_receipt':str(refpath),'source_receipt_sha256':runner.digest(refpath),'sample_count':0,'performance_samples':0,'budgets_s':{'build':30,'diagnostic':15,'proof':9.5},'orchestrator_sha256':runner.digest(__file__),'competing_work':runner.competing_work()}
lock=(runner.RESULTS/'phase7-sqlite.lock').open('a+b')
try:
    fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    env={**os.environ,'LAYERFS_CONSTRUCTION_WORKERS':'1','CARGO_TARGET_DIR':str(root/'core/target')}
    build=invoke(['cargo','+1.85.1','build','--manifest-path','core/Cargo.toml','--release','--locked','-p','layerfs-project','--example','durable_init_costs','--example','verify_namespace'],out,'build',30000000000,env,root)
    record['build']=build
    if build['exit_code'] or build['timed_out']:raise RuntimeError('needed release build failed')
    cb=invoke(['clang','-O2','-Wall','-Wextra','-Werror','-dynamiclib','core/benchmark/fs-bench-pro/diagnostics/sqlite_init_vfs_observer.c','-lsqlite3','-o','core/target/durable100-init-vfs.dylib'],out,'observer-build',30000000000,os.environ.copy(),root)
    record['observer_build']=cb
    if cb['exit_code'] or cb['timed_out']:raise RuntimeError('observer build failed')
    driver=archive(root/'core/target/release/examples/durable_init_costs',root,runner)
    verifier=archive(root/'core/target/release/examples/verify_namespace',root,runner)
    library=archive(root/'core/target/durable100-init-vfs.dylib',root,runner)
    record.update(driver=driver,verifier=verifier,library=library)
    pre={x['path']:runner.digest(x['path']) for x in [driver,verifier,library]}
    db=out/'store.sqlite'
    diagnostic_env={**env,'DYLD_INSERT_LIBRARIES':library['path'],'LAYERFS_CAUSE_VFS_LOG':str(out/'vfs.json'),'LAYERFS_INIT_VFS_EVENTS':str(out/'events.json')}
    record['diagnostic_environment']={k:diagnostic_env[k] for k in ['LAYERFS_CONSTRUCTION_WORKERS','CARGO_TARGET_DIR','DYLD_INSERT_LIBRARIES','LAYERFS_CAUSE_VFS_LOG','LAYERFS_INIT_VFS_EVENTS']}
    record['run']=invoke([driver['path'],fixture['source'],str(db),'durable','--vfs','payload-segments'],out,'driver',15000000000,diagnostic_env,root)
    record['sample_count']=1
    if record['run']['exit_code'] or record['run']['timed_out']:raise RuntimeError('diagnostic failed; no retry')
    stderr=(out/'driver.stderr').read_text();roots=re.findall(r'\broot=.*?([0-9a-f]{64})',stderr)
    assert roots and roots[-1]==prior['performance']['child']['root'],roots
    vfs=json.loads((out/'vfs.json').read_text());events=json.loads((out/'events.json').read_text())
    assert vfs['live_files']==vfs['close_errors']==vfs['unknown_sync_flags']==0
    assert events['omitted']==0 and events['native_stmt_status_reset'] is False
    assert all(x['result'] in [100,101] for x in events['events'])
    units=[json.loads(s.removeprefix('VFS_UNIT ')) for s in (out/'driver.stdout').read_text().splitlines() if s.startswith('VFS_UNIT ')]
    ids={x['unit_id'] for x in units};assert all(x['unit_id'] in ids or x['unit_id']==0 for x in events['events'])
    for file in vfs['files']:
        rows=[x for x in vfs['call_scopes'] if x['class']==file['class']]
        for key in ['writes','write_submitted_bytes','write_ns','syncs','sync_ns']:assert sum(x[key] for x in rows)==file[key],key
    assert not any(x['errors'] for x in vfs['call_scopes'])
    pubs=[x for x in units if x['unit']=='storage.publish'];assert pubs
    assert sum(x['scope']==1 for x in events['events'] if x['unit']=='storage.publish')==len(pubs)
    for unit in units:
        inside=[x for x in events['events'] if x['unit_id']==unit['unit_id']]
        for k in range(4):
            for j in range(8):assert sum(x['classes'][k][j] for x in inside)<=unit['classes'][k][j]
    # Preserve the closed producer, including writable SHM, before proof open.
    import shutil
    producer=out/'producer-before-proof';producer.mkdir()
    for file in [db,pathlib.Path(str(db)+'-wal'),pathlib.Path(str(db)+'-shm')]:
        if file.exists():shutil.copy2(file,producer/file.name)
    shutil.copytree(pathlib.Path(str(db)+'.payload'),producer/'store.sqlite.payload')
    record['producer_before_proof']={str(p.relative_to(producer)):runner.digest(p) for p in producer.rglob('*') if p.is_file()}
    cmd=[verifier['path'],str(db),str(db),prior['performance']['child']['root'],'41'*16,fixture['manifest'],fixture['manifest_sha256'],'durable']
    record['proof']=invoke(cmd,out,'verifier',9500000000,{**os.environ,'LAYERFS_CONSTRUCTION_WORKERS':'1','LAYERFS_HISTORY_CURSOR_KEY':'28'*32},root)
    assert record['proof']['exit_code']==0 and not record['proof']['timed_out'] and record['proof']['child']['status']=='PASS'
    assert all(runner.digest(s)==sha for s,sha in pre.items())
    record['direct_units']=[json.loads(line) for line in (out/'driver.stdout').read_text().splitlines() if line.startswith('{')]
    record.update(status='DIAGNOSTIC_COMPLETE',root_match=True,observer_status='PASS',verification_status='PASS',units=units,vfs=vfs,events=events,binary_pre_post=pre,cleanup='PASS' if not list(out.glob('.layerfs-allocation-*')) else 'FAIL',source_dirty_after=runner.identities()['source_dirty'])
    print(json.dumps({'status':record['status'],'unit_count':len(units),'events':events['events_attempted'],'publications':len(pubs),'files':vfs['files'],'proof':record['proof']['child']}))
except Exception as error:
    record.update(status='FAIL',reason=str(error));raise
finally:
    runner.write_json(out/'receipt.json',record);runner.manifest_run(out);lock.close()
