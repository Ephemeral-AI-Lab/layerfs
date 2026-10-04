"""One frozen cold diagnostic child per engine, never an admission/speed replay."""
from pathlib import Path
import sys,json,os,subprocess,hashlib,fcntl
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT/'core/benchmark/fs-bench-pro'))
import runner as common
from families import phase7_sqlite as sqlite,init_namespace as init
from shared import cold_native
from cause_reference_vehicle import generate,BASE

def write(path,value):path.write_text(json.dumps(value,indent=2,sort_keys=True)+'\n')
def run(arm,output,api_observer=False,vfs_observer=False,memory_off=False):
    if arm not in ('baseline','candidate'):raise ValueError('baseline or candidate required')
    out=common.owned(output);out.mkdir()
    base=ROOT/'target/phase7-baseline/layerfs';owner=base if arm=='baseline' else ROOT
    identity=common.identities()
    if identity['source_dirty']:raise ValueError('commit/freeze caller, observer and product before diagnostic')
    locks=[]
    for path in [common.RESULTS/'phase7-sqlite.lock',owner/'target/phase7-sqlite.lock']:
        path.parent.mkdir(parents=True,exist_ok=True);lock=path.open('a+b');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB);locks.append(lock)
    if arm=='baseline':
        if subprocess.check_output(['git','rev-parse','HEAD'],cwd=base,text=True).strip()!=BASE or subprocess.check_output(['git','status','--porcelain'],cwd=base,text=True).strip():raise ValueError('clean pinned baseline required')
    (out/'benchmark-report-contract-read.txt').write_text((ROOT/'benchmark_agent_report.md').read_text())
    source,seals=generate(ROOT)
    prospective={'kind':'paired-cold-namespace-small-step-cause-diagnostic','arm':arm,'admission':'NOT_RUN','product_speed_sample_count':0,'declared_diagnostic_children':1,'identity':identity,'shared_source_sha256':seals,'command_budget_ns':15000000000,'verification_budget_ns':9500000000,'scope':'shared namespace caller through public C1/C2/C5; Service envelope omitted; fixed observers and canonical census added; not an admission clock','native_vm_counter_qualified':False,'trace_vm_scope':'prepared statements exposing STMT/PROFILE events; status reset at PROFILE; opaque blob/VFS/internal operations not inferred','required_root_equivalence':'both frozen production Init1000 roots and eachother','required_canonical_equivalence':'2003ID role/length inventory and identical digest; independently sampled byte/tree proof'}
    prospective['api_observer_enabled']=api_observer
    prospective['vfs_observer_enabled']=vfs_observer
    prospective['memory_off_diagnostic']=memory_off
    if memory_off:
        prospective['profile_scope']='MEMORY/OFF before first mutation and at close; candidate retains original WAL/FULL validation and profile transition inside measured bootstrap; not a pure MEMORY/OFF connection-open lifecycle'
        prospective['proof_scope']='candidate independent byte-copy with WAL header for unchanged production verifier; original measured database hash checked unchanged; all copy/hash/header/verification wall below9.5s'
    write(out/'prospective.json',prospective)
    temporary=None
    if arm=='baseline':
        temporary=base/'core/crates/layerfs-api/sdk/examples/cause_reference_namespace.rs'
        if temporary.exists():raise ValueError('temporary example already exists')
        temporary.write_text(source);(out/'generated-reference.rs').write_text(source)
        build=['cargo','+1.85.1','build','--manifest-path','core/Cargo.toml','--release','--locked','-p','layerfs-sdk','--example','cause_reference_namespace','-p','layerfs-server','--example','verify_namespace']
    else:build=['cargo','+1.85.1','build','--manifest-path','core/Cargo.toml','--release','--locked','-p','layerfs-project','--example','cause_namespace','--example','verify_namespace']
    try:b=sqlite.invoke(build,out,'build',30000000000,{**os.environ,'CARGO_TARGET_DIR':str(owner/'core/target')},owner)
    finally:
        if temporary is not None:temporary.unlink()
    write(out/'build.json',b)
    if b['exit_code']!=0 or b['timed_out']:raise ValueError('required release/locked build failed; no diagnostic child')
    if arm=='baseline' and subprocess.check_output(['git','status','--porcelain'],cwd=base,text=True).strip():raise ValueError('baseline product not clean after build')
    driver=sqlite.archive(owner/'core/target/release/examples'/('cause_reference_namespace' if arm=='baseline' else 'cause_namespace'),ROOT,common)
    verifier=sqlite.archive(owner/'core/target/release/examples/verify_namespace',ROOT,common)
    api=None
    if api_observer:
        binary=out/'sqlite-api-observer.dylib'
        api_build=sqlite.invoke(['clang','-O2','-Wall','-Wextra','-Werror','-dynamiclib',str(ROOT/'core/benchmark/fs-bench-pro/diagnostics/sqlite_api_observer.c'),'-lsqlite3','-o',str(binary)],out,'api-build',30000000000,os.environ.copy(),ROOT)
        write(out/'api-build.json',api_build)
        if api_build['exit_code']!=0 or api_build['timed_out']:raise ValueError('required API observer build failed')
        api=sqlite.archive(binary,ROOT,common)
        identity['api_observer']=api
        identity['api_source_sha256']=common.digest(ROOT/'core/benchmark/fs-bench-pro/diagnostics/sqlite_api_observer.c')
    vfs=None
    if vfs_observer:
        binary=out/'sqlite-vfs-observer.dylib'
        vfs_build=sqlite.invoke(['clang','-O2','-Wall','-Wextra','-Werror','-dynamiclib',str(ROOT/'core/benchmark/fs-bench-pro/diagnostics/sqlite_vfs_observer.c'),'-lsqlite3','-o',str(binary)],out,'vfs-build',30000000000,os.environ.copy(),ROOT)
        write(out/'vfs-build.json',vfs_build)
        if vfs_build['exit_code']!=0 or vfs_build['timed_out']:raise ValueError('required VFS observer build failed')
        vfs=sqlite.archive(binary,ROOT,common)
        identity['vfs_observer']=vfs
        identity['vfs_source_sha256']=common.digest(ROOT/'core/benchmark/fs-bench-pro/diagnostics/sqlite_vfs_observer.c')
    memory=proof_adapter=None
    if memory_off:
        for source_name,binary_name,dynamic in [('sqlite_memory_profile.c','sqlite-memory-profile.dylib',True),('sqlite_proof_journal.c','sqlite-proof-journal',False)]:
            binary=out/binary_name
            command=['clang','-O2','-Wall','-Wextra','-Werror']+(['-dynamiclib'] if dynamic else [])+[str(ROOT/'core/benchmark/fs-bench-pro/diagnostics'/source_name),'-lsqlite3','-o',str(binary)]
            b=sqlite.invoke(command,out,binary_name+'-build',30000000000,os.environ.copy(),ROOT)
            write(out/(binary_name+'-build.json'),b)
            if b['exit_code']!=0 or b['timed_out']:raise ValueError('required memory diagnostic tool build failed')
            artifact=sqlite.archive(binary,ROOT,common)
            identity[source_name]={'artifact':artifact,'source_sha256':common.digest(ROOT/'core/benchmark/fs-bench-pro/diagnostics'/source_name)}
            if dynamic:memory=artifact
            else:proof_adapter=artifact
    helper=cold_native.build(ROOT,out,sqlite.invoke)
    case=init.CASES[sqlite.CASES['phase7-sqlite-init-1000-v2'].fixture]
    fixture=init.prepare(case,common.RESULTS/'sdk-prepared')
    identity.update({'driver':driver,'verifier':verifier,'cold_helper':helper,'fixture_manifest_sha256':fixture['manifest_sha256']})
    write(out/'frozen.json',{'identity':identity,'fixture':fixture,'baseline_clean':True if arm=='baseline' else None})
    claim=common.RESULTS/'cause-namespace-claims'/hashlib.sha256(json.dumps([arm,identity['source_tree'],identity['harness_seal'],driver['sha256'],fixture['manifest_sha256']],sort_keys=True).encode()).hexdigest()
    claim.parent.mkdir(parents=True,exist_ok=True)
    with claim.open('x') as f:f.write(str(out)+'\n')
    import time
    start=time.monotonic_ns();cold=cold_native.attest(fixture['source'],helper,out,15000000000,sqlite.invoke,ROOT)
    scratch=out/'scratch';scratch.mkdir()
    remaining=15000000000-(time.monotonic_ns()-start)
    if cold['status']!='PASS' or remaining<=0:write(out/'receipt.json',{**prospective,'cold':cold,'status':'INELIGIBLE','child':'NOT_RUN','diagnostic_child_count':0});raise ValueError('cold contract/budget failed')
    env={**os.environ,'LAYERFS_CONSTRUCTION_WORKERS':'1','LAYERFS_HISTORY_CURSOR_KEY':'28'*32,'TMPDIR':str(scratch)}
    if api is not None:env.update({'DYLD_INSERT_LIBRARIES':api['path'],'LAYERFS_CAUSE_API_LOG':str(out/'sqlite-api.json')})
    if vfs is not None:
        libraries=[x['path'] for x in [api,vfs] if x is not None]
        env.update({'DYLD_INSERT_LIBRARIES':':'.join(libraries),'LAYERFS_CAUSE_VFS_ENABLED':'1','LAYERFS_CAUSE_VFS_LOG':str(out/'sqlite-vfs.json')})
    if memory is not None:
        libraries=[x['path'] for x in [api,vfs,memory] if x is not None]
        env.update({'DYLD_INSERT_LIBRARIES':':'.join(libraries),'LAYERFS_CAUSE_MEMORY_ARM':arm,'LAYERFS_CAUSE_MEMORY_DB':str(out/'store.sqlite'),'LAYERFS_CAUSE_MEMORY_LOG':str(out/'sqlite-memory.json')})
    child=sqlite.invoke([driver['path'],fixture['source'],str(out/'store.sqlite'),str(scratch),'phase7-sqlite-init-1000-v2'],out,'driver',remaining,env,owner)
    wall=time.monotonic_ns()-start
    result={**prospective,'frozen':identity,'fixture':fixture,'cold':cold,'run':child,'diagnostic_child_count':1,'command_wall_ns':wall,'cleanup':'PASS' if not list(scratch.iterdir()) else 'FAIL','status':'DIAGNOSTIC','verification':'NOT_RUN'}
    write(out/'receipt.json',result)
    if child['exit_code']==0 and not child['timed_out'] and child['child'] and child['child'].get('status')=='DIAGNOSTIC':
        data=child['child'];proof_db=out/'store.sqlite';proof_start=time.monotonic_ns()
        proof_env={k:v for k,v in env.items() if k!='DYLD_INSERT_LIBRARIES' and not k.startswith('LAYERFS_CAUSE_')}
        if memory_off and arm=='candidate':
            import shutil
            original_hash=common.digest(proof_db);proof_db=out/'proof.sqlite'
            if proof_db.exists():raise ValueError('fresh proof copy required')
            shutil.copy2(out/'store.sqlite',proof_db)
            if common.digest(proof_db)!=original_hash:raise ValueError('proof byte copy mismatch')
            adapted=sqlite.invoke([proof_adapter['path'],str(proof_db)],out,'proof-journal',9500000000-(time.monotonic_ns()-proof_start),proof_env,owner)
            result['proof_copy']={'original_sha256':original_hash,'method':'independent shutil.copy2 byte copy; exact SHA match before journal header conversion','adapter':adapted}
            if adapted['exit_code']!=0 or adapted['timed_out']:raise ValueError('proof-copy header adaptation failed')
        proof=sqlite.invoke([verifier['path'],str(proof_db),str(out/'store.sqlite.history.sqlite' if arm=='baseline' else proof_db),data['root'],data['stack'],fixture['manifest'],fixture['manifest_sha256']],out,'verifier',9500000000-(time.monotonic_ns()-proof_start),proof_env,owner)
        if memory_off and arm=='candidate':
            result['proof_copy']['original_after_sha256']=common.digest(out/'store.sqlite')
            if result['proof_copy']['original_after_sha256']!=original_hash:raise ValueError('measured original changed during proof')
        result['verification']=proof
        result['verification_complete_wall_ns']=time.monotonic_ns()-proof_start
    else:result['status']='FAIL'
    if api is not None:
        log=out/'sqlite-api.json'
        result['api_observer']=json.loads(log.read_text()) if log.exists() else {'status':'UNAVAILABLE','reason':'interposed observer did not emit its fixed report'}
    if vfs is not None:
        log=out/'sqlite-vfs.json'
        result['vfs_observer']=json.loads(log.read_text()) if log.exists() else {'status':'UNAVAILABLE','reason':'delegated VFS report missing'}
    if memory is not None:
        log=out/'sqlite-memory.json'
        result['memory_profile']=json.loads(log.read_text()) if log.exists() else {'status':'UNAVAILABLE','reason':'profile intervention/readback report missing'}
    write(out/'receipt.json',result);common.manifest_run(out)
    return out
if __name__=='__main__':
    if len(sys.argv) not in (3,4) or len(sys.argv)==4 and sys.argv[3] not in ('--api','--vfs','--memory-off'):raise SystemExit('ARM FRESH_OUTPUT [--api|--vfs|--memory-off] required')
    print(run(sys.argv[1],sys.argv[2],len(sys.argv)==4,len(sys.argv)==4 and sys.argv[3] in ('--vfs','--memory-off'),len(sys.argv)==4 and sys.argv[3]=='--memory-off'))
