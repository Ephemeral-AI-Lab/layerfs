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
def run(arm,output):
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
    child=sqlite.invoke([driver['path'],fixture['source'],str(out/'store.sqlite'),str(scratch),'phase7-sqlite-init-1000-v2'],out,'driver',remaining,env,owner)
    wall=time.monotonic_ns()-start
    result={**prospective,'frozen':identity,'fixture':fixture,'cold':cold,'run':child,'diagnostic_child_count':1,'command_wall_ns':wall,'cleanup':'PASS' if not list(scratch.iterdir()) else 'FAIL','status':'DIAGNOSTIC','verification':'NOT_RUN'}
    write(out/'receipt.json',result)
    if child['exit_code']==0 and not child['timed_out'] and child['child'] and child['child'].get('status')=='DIAGNOSTIC':
        data=child['child'];proof=sqlite.invoke([verifier['path'],str(out/'store.sqlite'),str(out/'store.sqlite.history.sqlite' if arm=='baseline' else out/'store.sqlite'),data['root'],data['stack'],fixture['manifest'],fixture['manifest_sha256']],out,'verifier',9500000000,env,owner)
        result['verification']=proof
    else:result['status']='FAIL'
    write(out/'receipt.json',result);common.manifest_run(out)
    return out
if __name__=='__main__':
    if len(sys.argv)!=3:raise SystemExit('ARM FRESH_OUTPUT required')
    print(run(sys.argv[1],sys.argv[2]))
