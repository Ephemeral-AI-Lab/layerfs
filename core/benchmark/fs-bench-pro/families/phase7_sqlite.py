"""SQLite-only Step10 registry and direct Init runner; unresolved gates fail closed."""
from dataclasses import dataclass
from pathlib import Path
import fcntl,hashlib,json,os,resource,shutil,subprocess,time,threading,signal
from families import init_namespace as init
from shared import sqlite_contract as contract
from shared import cold_native
BASE='7edddbdb8e8512627aed0ed42533ef099d802384'
@dataclass(frozen=True)
class Case:
    id:str
    fixture:str|None
    states:int|None
    storage_ceiling:int|None
    command_budget_ns:int|None
    verification_budget_ns:int=9_500_000_000
    profile:str="durable"
CASES={c.id:c for c in (
    *(Case(f'phase7-sqlite-init-{n}-v2',f.id,None,None,15_000_000_000) for n,f in zip((100,1000,10000,100000),init.CASES.values())),
    Case('phase7-sqlite-history-stride10-v1',None,17,54_278_964,60_000_000_000),
    Case('phase7-sqlite-history-stride3-v1',None,53,70_427_034,170_000_000_000),
    Case('phase7-sqlite-history-stride1-v1',None,157,92_342_273,170_000_000_000),
)}
REQUIRED=tuple(CASES)
# Supported direct-open Disposable is a separate seven-case identity, never a
# relabeling of Durable or the earlier intervention diagnostic.
DISPOSABLE={c.id.replace('phase7-sqlite-','phase7-sqlite-disposable-').rsplit('-v',1)[0]+'-v1':c for c in CASES.values()}
for name,c in DISPOSABLE.items():
    CASES[name]=Case(name,c.fixture,c.states,c.storage_ceiling,c.command_budget_ns,c.verification_budget_ns,'disposable')
LEGACY_HISTORY=tuple(name for name,c in CASES.items() if c.states is not None)
# Corrected completed-save proof requires new prospective scenario versions;
# historical v1 cases/receipts remain visible and cannot be sampled again.
for name in LEGACY_HISTORY:
    c=CASES[name];new=name.rsplit('-v',1)[0]+'-v2'
    CASES[new]=Case(new,c.fixture,c.states,c.storage_ceiling,c.command_budget_ns,c.verification_budget_ns,c.profile)
REQUIRED=tuple(name if CASES[name].states is None else name.rsplit('-v',1)[0]+'-v2' for name in REQUIRED)
REQUIRED_BY_PROFILE={'durable':REQUIRED,'disposable':tuple(name if CASES[name].states is None else name.rsplit('-v',1)[0]+'-v2' for name in DISPOSABLE)}
# Owner 2026-10-04: modest prospective stride1 increase only. Preserve v2/170s
# receipts and registry values; new v3 selections carry 190s in both profiles.
RETIRED_STRIDE1=tuple(name for name,c in CASES.items() if c.states==157 and name.endswith('-v2'))
for name in RETIRED_STRIDE1:
    c=CASES[name];new=name.rsplit('-v',1)[0]+'-v3'
    CASES[new]=Case(new,c.fixture,c.states,c.storage_ceiling,190_000_000_000,c.verification_budget_ns,c.profile)
LEGACY_HISTORY+=RETIRED_STRIDE1
REQUIRED=tuple(name.rsplit('-v',1)[0]+'-v3' if name in RETIRED_STRIDE1 else name for name in REQUIRED)
REQUIRED_BY_PROFILE={profile:tuple(name.rsplit('-v',1)[0]+'-v3' if name in RETIRED_STRIDE1 else name for name in names)
                     for profile,names in REQUIRED_BY_PROFILE.items()}
PROFILE_IDS={'durable':contract.PROFILE,'disposable':'sqlite-memory-off-macos-v1'}
# Missing user rulings are explicit; no measurement uses a guessed admission gate.
INIT_ALLOCATION_RULE="candidate-final-database-wal-shm-allocation<=matched-baseline-final-total-v1"
HISTORY_BUDGET_RULE='owner-2026-10-04-60-170-190-performance-only-v2'

def invoke(command,folder,label,budget_ns,env,cwd):
    start=time.monotonic_ns();timeout=False
    with (folder/f'{label}.stdout').open('xb') as out,(folder/f'{label}.stderr').open('xb') as err:
        child=subprocess.Popen(command,cwd=cwd,env=env,stdout=out,stderr=err,start_new_session=True)
        expired=threading.Event()
        def expire():
            expired.set()
            try: os.killpg(child.pid,signal.SIGKILL)
            except ProcessLookupError: pass
        timer=threading.Timer(max(0,(budget_ns-(time.monotonic_ns()-start))/1e9),expire)
        timer.daemon=True;timer.start()
        try:
            _,status,usage=os.wait4(child.pid,0)
            child.returncode=os.waitstatus_to_exitcode(status)
        finally:
            timer.cancel()
        timeout=expired.is_set()
    wall=time.monotonic_ns()-start
    try:data=json.loads((folder/f'{label}.stdout').read_text())
    except (ValueError,UnicodeError):data=None
    return {'command':command,'wall_ns':wall,'budget_ns':budget_ns,'exit_code':child.returncode,'timed_out':timeout,'child':data,
            'cpu_ns':round((usage.ru_utime+usage.ru_stime)*1e9),'peak_rss_bytes':usage.ru_maxrss,
            'rss_scope':'Darwin wait4 per-child lifetime for the recorded command; no phase-only attribution'}

def archive(binary,root,common):
    sha=common.digest(binary);dst=root/'benchmark-results/fs-bench-pro/binary-archive'/sha/binary.name
    dst.parent.mkdir(parents=True,exist_ok=True)
    if not dst.exists():shutil.copy2(binary,dst);dst.chmod(0o555)
    if common.digest(dst)!=sha:raise ValueError('immutable archive identity mismatch')
    return {'path':str(dst),'sha256':sha}

def build(root,arm,out,common):
    package='layerfs-project' if arm=='candidate' else 'layerfs-sdk'
    example='benchmark_init' if arm=='candidate' else 'sqlite_reference_init'
    target=root/'core/target';temporary=None
    if arm=='baseline':
        temporary=root/'core/crates/layerfs-api/sdk/examples/sqlite_reference_init.rs'
        if temporary.exists():raise ValueError('reference example path already occupied')
        shutil.copy2(common.ROOT/'core/benchmark/fs-bench-pro/diagnostics/sqlite_reference_init.rs',temporary)
    try:
        command=['cargo','+1.85.1','build','--manifest-path','core/Cargo.toml','--release','--locked','-p',package,'--example',example,'--example','verify_namespace']
        if arm=='baseline':command+=['-p','layerfs-server']
        r=invoke(command,out,'build',30_000_000_000,{**os.environ,'CARGO_TARGET_DIR':str(target)},root)
        r['profile']='release/locked';r['dependency_reuse']='worktree-local incremental target';r['reference_product_unmodified']=arm=='baseline'
        if r['exit_code']!=0 or r['timed_out']:r['status']='BUILD_SLOW' if r['timed_out'] else 'FAIL';return r
        r['status']='PASS';r['binaries']={example:archive(target/'release/examples'/example,root,common),'verify_namespace':archive(target/'release/examples/verify_namespace',root,common)}
        return r
    finally:
        if temporary is not None:temporary.unlink()

def run(selection,output,arm,baseline_root,common,corpus_root=None,reference_pins=None):
    case=CASES[selection]
    if selection in LEGACY_HISTORY:
        raise ValueError('historical history selection retired; original receipts preserved; use current required case version')
    if case.fixture is None:
        from families import phase7_history
        return phase7_history.run(case,output,arm,baseline_root,common,corpus_root,reference_pins)
    if INIT_ALLOCATION_RULE is None:
        raise ValueError('prospective Init allocation contract is pending; no admission arm is authorized under a guessed gate')
    if os.uname().sysname!='Darwin':raise ValueError('required SQLite full-sync profile and wait4 accounting are macOS-only')
    root=common.ROOT if arm=='candidate' else Path(baseline_root).resolve()
    if arm=='baseline' and (not root.is_relative_to(common.ROOT/'target/phase7-baseline') or subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()!=BASE or subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True)):
        raise ValueError('reference requires clean pinned owned checkout')
    out=common.owned(output);out.mkdir(parents=True)
    record={'schema':'phase7-sqlite-step10-v2','case':case.id,'arm':arm,'status':'NOT_RUN','sample_count':0,'verification_status':'NOT_RUN','cache_status':'INCOMPLETE','cleanup':{'status':'NOT_RUN'},'comparison_scope':'inner complete product clock including fresh database create/open, real Init, required checkpoint and final close; external child wall reported separately','margin_arithmetic':'10*candidate_ns<=11*baseline_ns','cache_contract':contract.CACHE,'requested_profile':case.profile,'profile':PROFILE_IDS[case.profile] if arm=='candidate' else 'Phase4.5 MEMORY/OFF disclosed','command_budget_ns':case.command_budget_ns,'verification_budget_ns':case.verification_budget_ns,'construction_workers':4,'environment_workers':1,'required_case_ids':REQUIRED_BY_PROFILE[case.profile],'allocation_rule':INIT_ALLOCATION_RULE}
    locks=[]
    try:
        for p in [common.RESULTS/'phase7-sqlite.lock']+([root/'target/phase7-sqlite.lock'] if arm=='baseline' else []):
            p.parent.mkdir(parents=True,exist_ok=True);h=p.open('a+b');locks.append(h);fcntl.flock(h,fcntl.LOCK_EX|fcntl.LOCK_NB)
        identity=common.identities()
        if identity['source_dirty']:raise ValueError('freeze committed product/harness before an arm')
        def scope_seal(paths):
            value=hashlib.sha256()
            for p in sorted(paths):value.update(str(p.relative_to(root)).encode()+b"\0");value.update(p.read_bytes())
            return value.hexdigest()
        crates=root/'core/crates'
        product=list(crates.glob('*/src/**/*.rs'))+list(crates.glob('*/sql/**/*.sql'))
        product+=list((crates/'layerfs-api').glob('*/src/**/*.rs'))
        record['product_seal']=scope_seal(product)
        record['shipped_sql_seal']=scope_seal(list(crates.glob('*/sql/**/*.sql')))
        compilation=product+list(crates.glob('*/Cargo.toml'))+list((crates/'layerfs-api').glob('*/Cargo.toml'))+list(crates.glob('*/examples/**/*.rs'))+list((crates/'layerfs-api').glob('*/examples/**/*.rs'))+[root/'core/Cargo.toml',root/'core/Cargo.lock',root/'.cargo/config.toml']
        record['compilation_seal']=scope_seal(compilation)
        record['cold_source_sha256']=common.digest(cold_native.SOURCE)
        record['reference_driver_source_sha256']=common.digest(common.ROOT/'core/benchmark/fs-bench-pro/diagnostics/sqlite_reference_init.rs')
        record['setup_method']='fresh output; database creation is measured; identity-checked prepared source fixture reuse only'
        record['identity']=identity;record['measured_source_commit']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip();record['root_cargo_config_sha256']=common.digest(root/'.cargo/config.toml');record['dependency_seal']=common.digest(root/'core/Cargo.lock');record['competing_work']=common.competing_work()
        record['build']=build(root,arm,out,common)
        if record['build']['status']!='PASS':record['reason']='build failed or exceeded fixed bound';return out
        record['cold_helper']=cold_native.build(common.ROOT,out,invoke)
        fixture_case=init.CASES[case.fixture];fixture=init.prepare(fixture_case,common.RESULTS/'sdk-prepared');record['fixture']=fixture
        scratch=out/'scratch';scratch.mkdir();env={**os.environ,'LAYERFS_CONSTRUCTION_WORKERS':'1','LAYERFS_HISTORY_CURSOR_KEY':'28'*32,'TMPDIR':str(scratch)}
        db=out/'store.sqlite';history=out/'history.sqlite'
        binaries=record['build']['binaries'];driver=binaries['benchmark_init' if arm=='candidate' else 'sqlite_reference_init']['path']
        command=[driver,fixture['source'],str(db),str(scratch if arm=='candidate' else history),case.id]+([case.profile] if arm=='candidate' else [])
        claim=common.RESULTS/'phase7-sqlite-sample-claims'/hashlib.sha256(json.dumps([case.id,arm,identity['source_tree'],identity['harness_seal'],record['measured_source_commit'],fixture['manifest_sha256']],sort_keys=True).encode()).hexdigest()
        claim.parent.mkdir(parents=True,exist_ok=True)
        with claim.open('x') as h:h.write(str(out)+'\n')
        perf_start=time.monotonic_ns();record['residency']=cold_native.attest(fixture['source'],record['cold_helper'],out,case.command_budget_ns,invoke,common.ROOT);record['cache_status']=record['residency']['status']
        if record['cache_status']!='PASS':record['status']='INELIGIBLE';return out
        remaining=case.command_budget_ns-(time.monotonic_ns()-perf_start)
        if remaining<=0:record['status']='NOT_RUN';record['reason']='cold attestation exhausted complete performance command budget';return out
        sample=invoke(command,out,'driver',remaining,env,root);record['sample_count']=1;record['performance']=sample;record['comparison_ns']=sample['child'].get('operation_ns') if isinstance(sample['child'],dict) else None
        record['storage']=contract.allocations([db] if arm=='candidate' else [db,history]);record['storage_bytes']=record['storage']['total_bytes'];record['cleanup']={'status':'PASS' if not list(scratch.iterdir()) and not list(out.glob('.layerfs-allocation-*')) else 'FAIL','scope':'measured child exited, ordering/allocation scratch empty, database evidence retained'};record['command_wall_ns']=time.monotonic_ns()-perf_start
        child=sample['child']
        if sample['exit_code']!=0 or sample['timed_out'] or not isinstance(child,dict) or child.get('status')!='COMPLETE':record['status']='FAIL';return out
        record['status']='COMPLETE'
        if arm=='candidate':
            prefix='EFFECTIVE_PROFILE '
            matches=[line[len(prefix):] for line in (out/'driver.stderr').read_text().splitlines() if line.startswith(prefix)]
            if len(matches)!=1:raise ValueError('actual selected-profile readback missing or ambiguous')
            effective=json.loads(matches[0]);record['effective_profile']=effective
            expected={'identity':PROFILE_IDS[case.profile],'journal_mode':'wal' if case.profile=='durable' else 'memory','synchronous':2 if case.profile=='durable' else 0,'foreign_keys':1,'fullfsync':1 if case.profile=='durable' else 0,'checkpoint_fullfsync':1,'page_size':4096,'cache_size':-2048,'mmap_size':0,'temp_store':2,'wal_checkpoint_performed':case.profile=='durable'}
            if effective!=expected:raise ValueError('actual selected-profile settings/completion mismatch')
        proof=invoke([binaries['verify_namespace']['path'],str(db),str(db if arm=='candidate' else history),child['root'],child['stack'],fixture['manifest'],fixture['manifest_sha256']]+([case.profile] if arm=='candidate' else []),out,'verifier',case.verification_budget_ns,env,root)
        record['verification']=proof;record['verification_wall_ns']=proof['wall_ns'];record['verification_status']='PASS' if proof['exit_code']==0 and not proof['timed_out'] and common.lite_verification_pass(proof['child'],fixture_case,child,fixture) else 'FAIL'
        return out
    except Exception as e:
        record['status']='INCOMPLETE';record['reason']=str(e);raise
    finally:
        common.write_json(out/'receipt.json',record);common.manifest_run(out)
        for h in reversed(locks):h.close()
