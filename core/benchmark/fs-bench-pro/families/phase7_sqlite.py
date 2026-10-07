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
    proof_policy:str="every-tenth-content-path-and-final"
    pack_layout:str="monolithic"
    proof_envelope:str="lite12-v1"
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
# Owner explicitly sets 300s after the 190.012s reference timeout. New v4
# identities apply equally to both arms/profiles; preserve all v3/190s evidence.
RETIRED_STRIDE1_V3=tuple(name for name,c in CASES.items() if c.states==157 and name.endswith('-v3'))
for name in RETIRED_STRIDE1_V3:
    c=CASES[name];new=name.rsplit('-v',1)[0]+'-v4'
    CASES[new]=Case(new,c.fixture,c.states,c.storage_ceiling,300_000_000_000,c.verification_budget_ns,c.profile)
LEGACY_HISTORY+=RETIRED_STRIDE1_V3
REQUIRED=tuple(name.rsplit('-v',1)[0]+'-v4' if name in RETIRED_STRIDE1_V3 else name for name in REQUIRED)
REQUIRED_BY_PROFILE={profile:tuple(name.rsplit('-v',1)[0]+'-v4' if name in RETIRED_STRIDE1_V3 else name for name in names)
                     for profile,names in REQUIRED_BY_PROFILE.items()}
# Owner approves prospective all-state structure with bounded representative
# content and 12s combined proof. Case identities distinguish this lite scope.
LITE_PROOF_POLICY='all-state-structure-five-anchor-bounded-content-v1'
RETIRED_FULL_CONTENT=tuple(name for names in REQUIRED_BY_PROFILE.values() for name in names if CASES[name].states is not None)
for name in RETIRED_FULL_CONTENT:
    c=CASES[name];version=int(name.rsplit('-v',1)[1])+1;new=name.rsplit('-v',1)[0]+f'-v{version}'
    CASES[new]=Case(new,c.fixture,c.states,c.storage_ceiling,c.command_budget_ns,12_000_000_000,c.profile,LITE_PROOF_POLICY)
LEGACY_HISTORY+=RETIRED_FULL_CONTENT
REQUIRED_BY_PROFILE={profile:tuple(name.rsplit('-v',1)[0]+f'-v{int(name.rsplit("-v",1)[1])+1}' if name in RETIRED_FULL_CONTENT else name for name in names)
                     for profile,names in REQUIRED_BY_PROFILE.items()}
REQUIRED=REQUIRED_BY_PROFILE['durable']
# Owner-approved explicit new-store group-row schema; separate prospective IDs.
GROUP_ROW_CASES=[]
for profile,names in REQUIRED_BY_PROFILE.items():
    for name in names:
        c=CASES[name]
        if c.states is not None:
            new=name.rsplit('-v',1)[0]+'-group-rows-v1'
            CASES[new]=Case(new,c.fixture,c.states,c.storage_ceiling,c.command_budget_ns,c.verification_budget_ns,c.profile,c.proof_policy,'group-rows')
            GROUP_ROW_CASES.append(new)
# Explicit owner follow-up: stride1 only, prospective15s independent proof.
# Historical12s rows remain unchanged; successful performance may be shared.
for name in tuple(GROUP_ROW_CASES):
    c=CASES[name]
    if c.states==157:
        new=name.rsplit('-v',1)[0]+'-v2'
        CASES[new]=Case(new,c.fixture,c.states,c.storage_ceiling,c.command_budget_ns,15_000_000_000,c.profile,c.proof_policy,c.pack_layout,'owner-stride1-proof15-v2')
        GROUP_ROW_CASES.append(new)
# Owner requested more proof time; preserve v1/v2 and prospectively add30s.
for name in tuple(GROUP_ROW_CASES):
    c=CASES[name]
    if c.states==157 and name.endswith('-v1'):
        new=name.rsplit('-v',1)[0]+'-v3'
        CASES[new]=Case(new,c.fixture,c.states,c.storage_ceiling,c.command_budget_ns,30_000_000_000,c.profile,c.proof_policy,c.pack_layout,'owner-stride1-proof30-v3')
        GROUP_ROW_CASES.append(new)
# Owner-approved2026-10-04 schema3 covering index, fresh matched selection.
# Owner2026-10-05 ladder10->3->1 adds17/53 with their existing unchanged bounds.
INDEXED_GROUP_ROW_CASES=[]
for name in tuple(GROUP_ROW_CASES):
    c=CASES[name]
    if (c.states==157 and name.endswith('-v3')) or (c.states in (17,53) and name.endswith('-v1')):
        new=name.rsplit('-v',1)[0].replace('-group-rows','-group-rows-indexed')+'-v1'
        CASES[new]=Case(new,c.fixture,c.states,c.storage_ceiling,c.command_budget_ns,c.verification_budget_ns,c.profile,c.proof_policy,'group-rows-indexed',c.proof_envelope)
        INDEXED_GROUP_ROW_CASES.append(new)
# Owner2026-10-05 explicitly doubles absolute caps for a new closure round.
OWNER_CLOSURE_CASES_BY_PROFILE={'durable':[], 'disposable':[]}
for profile in OWNER_CLOSURE_CASES_BY_PROFILE:
    for n in (100,1000,10000,100000):
        old=f'phase7-sqlite-init-{n}-v2' if profile=='durable' else f'phase7-sqlite-disposable-init-{n}-v1'
        c=CASES[old];new=old.rsplit('-v',1)[0]+('-v3' if profile=='durable' else '-v2')
        CASES[new]=Case(new,c.fixture,None,None,2*c.command_budget_ns,2*c.verification_budget_ns,profile)
        OWNER_CLOSURE_CASES_BY_PROFILE[profile].append(new)
for old in INDEXED_GROUP_ROW_CASES:
    c=CASES[old]
    if c.profile=='durable':
        new=old.rsplit('-v',1)[0]+'-v2'
        CASES[new]=Case(new,None,c.states,c.storage_ceiling,2*c.command_budget_ns,2*c.verification_budget_ns,c.profile,c.proof_policy,c.pack_layout,'owner-double-caps-20261005-v2')
        OWNER_CLOSURE_CASES_BY_PROFILE['durable'].append(new)

# Shared bounded main-file allocation treatment; new prospective Init identities.
SHARED_ALLOCATION_CASES_BY_PROFILE={'durable':[], 'disposable':[]}
for profile,names in OWNER_CLOSURE_CASES_BY_PROFILE.items():
    for old in names:
        c=CASES[old]
        if c.fixture is not None:
            new=old.rsplit('-v',1)[0]+'-shared-allocation-v1'
            CASES[new]=Case(new,c.fixture,None,None,c.command_budget_ns,c.verification_budget_ns,profile)
            SHARED_ALLOCATION_CASES_BY_PROFILE[profile].append(new)

WAL_RESERVATION_CASES_BY_PROFILE={'durable':[], 'disposable':[]}
for profile,names in SHARED_ALLOCATION_CASES_BY_PROFILE.items():
    for old in names:
        c=CASES[old];new=old.replace('-shared-allocation-v1','-wal-reservation-v1')
        CASES[new]=Case(new,c.fixture,None,None,c.command_budget_ns,c.verification_budget_ns,profile)
        WAL_RESERVATION_CASES_BY_PROFILE[profile].append(new)

# Rejected WAL-headroom experiment: immutable registry retained, no replay.
RETIRED_WAL_RESERVATION_CASES=tuple(name for names in WAL_RESERVATION_CASES_BY_PROFILE.values() for name in names)

# S9 A3 (2026-10-06): Init's input-sized working state moved from ordering-run
# scratch files to provider rows in the measured Store. The driver takes no
# scratch argument and creates a Store with the acquisition tables. That is a
# different operation and Store format, so it has its own prospective
# identities at the default 15s command and 9.5s proof budgets. Every earlier
# Init identity keeps its receipts and cannot be sampled again in either arm:
# its candidate vehicle no longer exists in source.
ACQUISITION_VEHICLE='provider-sqlite-acquisition-rows-v1'
ACQUISITION_STORE_SCHEMA='monolithic-with-acquisition-tables-user-version-4'
RETIRED_RUN_BACKED_INIT=tuple(name for name,c in CASES.items() if c.fixture is not None)
ACQUISITION_CASES_BY_PROFILE={'durable':[], 'disposable':[]}
for profile in ACQUISITION_CASES_BY_PROFILE:
    for n,fixture in zip((100,1000,10000,100000),init.CASES.values()):
        new=f'phase7-sqlite-init-{n}-acquisition-v1' if profile=='durable' else f'phase7-sqlite-disposable-init-{n}-acquisition-v1'
        CASES[new]=Case(new,fixture.id,None,None,15_000_000_000,9_500_000_000,profile)
        ACQUISITION_CASES_BY_PROFILE[profile].append(new)

# Owner2026-10-06: lift the new Init caps prospectively to the earlier
# 30s/19s allowance. v1 is unsampled and retained at its original limits.
# Vehicle, fixture, oracle, profiles, relative speed and allocation gates stay
# unchanged; new case identities apply symmetrically to both arms.
ACQUISITION_V1_CASES_BY_PROFILE=ACQUISITION_CASES_BY_PROFILE
ACQUISITION_CASES_BY_PROFILE={'durable':[], 'disposable':[]}
for profile,names in ACQUISITION_V1_CASES_BY_PROFILE.items():
    for old in names:
        c=CASES[old];new=old.rsplit('-v',1)[0]+'-v2'
        CASES[new]=Case(new,c.fixture,c.states,c.storage_ceiling,
                       30_000_000_000,19_000_000_000,c.profile,
                       c.proof_policy,c.pack_layout,'owner-init-caps-30-19-20261006-v2')
        ACQUISITION_CASES_BY_PROFILE[profile].append(new)

# Explicitly withdrawn payload format. Preserve its case identity/limits, but
# the active Monolithic product cannot execute that historical vehicle.
PAYLOAD_SEGMENT_CASE='phase7-sqlite-init-100-acquisition-payload-segments-v1'
old=CASES['phase7-sqlite-init-100-acquisition-v2']
CASES[PAYLOAD_SEGMENT_CASE]=Case(PAYLOAD_SEGMENT_CASE,old.fixture,None,None,
    old.command_budget_ns,old.verification_budget_ns,old.profile,
    old.proof_policy,'payload-segments',old.proof_envelope)
RETIRED_PAYLOAD_SEGMENT_CASES=(PAYLOAD_SEGMENT_CASE,)

# Owner-selected full final-source same-profile cluster-one-end regression.
CLUSTER_ONE_END='197d2fb7d0a141d7a9350852022febeec3255bf2'
REGRESSION_CASES_BY_PROFILE={'durable':[], 'disposable':[]}
for profile,names in ACQUISITION_CASES_BY_PROFILE.items():
    for original in names:
        old=CASES[original]
        new=original.replace('-acquisition-v2','-cluster-one-regression-v1')
        CASES[new]=Case(new,old.fixture,None,None,old.command_budget_ns,
            old.verification_budget_ns,old.profile,old.proof_policy,'monolithic',old.proof_envelope)
        REGRESSION_CASES_BY_PROFILE[profile].append(new)
REGRESSION_CASES=tuple(name for names in REGRESSION_CASES_BY_PROFILE.values() for name in names)
REGRESSION_ALLOCATION_RULE='candidate-final-database-wal-shm<=same-profile-cluster-one-end-final-total-v1'

# Owner2026-10-06: fix retained free-page allocation and investigate scaling.
# Same complete lifecycle, profiles, workloads, cold state, workers and gates;
# bounded page reclamation is paid inside the product clock. Preserve v1 rows.
SPACE_SCALING_CASES=[]
for original in REGRESSION_CASES:
    old=CASES[original];new=original.replace('-cluster-one-regression-v1','-space-scaling-v1')
    CASES[new]=Case(new,old.fixture,None,None,old.command_budget_ns,
        old.verification_budget_ns,old.profile,old.proof_policy,'monolithic',old.proof_envelope)
    SPACE_SCALING_CASES.append(new)
SPACE_SCALING_CASES=tuple(SPACE_SCALING_CASES)
SPACE_SCALING_V1_CASES=SPACE_SCALING_CASES
SPACE_SCALING_CASES=[]
for original in SPACE_SCALING_V1_CASES:
    old=CASES[original];new=original.rsplit('-v',1)[0]+'-v2'
    CASES[new]=Case(new,old.fixture,None,None,old.command_budget_ns,
        old.verification_budget_ns,old.profile,old.proof_policy,'monolithic',old.proof_envelope)
    SPACE_SCALING_CASES.append(new)
SPACE_SCALING_CASES=tuple(SPACE_SCALING_CASES)

# Owner-selected insertion/read work reduction and combined final disposal.
# Keep earlier cases and gates unchanged; this source has a new case identity.
WORK_REDUCTION_CASES=[]
for original in SPACE_SCALING_CASES:
    old=CASES[original];new=original.replace('-space-scaling-v2','-work-reduction-v1')
    CASES[new]=Case(new,old.fixture,None,None,old.command_budget_ns,
        old.verification_budget_ns,old.profile,old.proof_policy,old.pack_layout,old.proof_envelope)
    WORK_REDUCTION_CASES.append(new)
WORK_REDUCTION_CASES=tuple(WORK_REDUCTION_CASES)

# Owner-selected bounded streaming completion: same schema, profiles and gates.
STREAMING_CASES=[]
for original in WORK_REDUCTION_CASES:
    old=CASES[original];new=original.replace('-work-reduction-v1','-streaming-v1')
    CASES[new]=Case(new,old.fixture,None,None,old.command_budget_ns,
        old.verification_budget_ns,old.profile,old.proof_policy,old.pack_layout,old.proof_envelope)
    STREAMING_CASES.append(new)
STREAMING_CASES=tuple(STREAMING_CASES)

# Same streaming semantics; bounded demand-aware completion coalescing.
STREAMING_BATCHED_CASES=[]
for original in STREAMING_CASES:
    old=CASES[original];new=original.rsplit('-v',1)[0]+'-v2'
    CASES[new]=Case(new,old.fixture,None,None,old.command_budget_ns,
        old.verification_budget_ns,old.profile,old.proof_policy,old.pack_layout,old.proof_envelope)
    STREAMING_BATCHED_CASES.append(new)
STREAMING_BATCHED_CASES=tuple(STREAMING_BATCHED_CASES)

# Same completion ownership, bounded admission bursts at half-free window.
STREAMING_REFILL_CASES=[]
for original in STREAMING_BATCHED_CASES:
    old=CASES[original];new=original.rsplit('-v',1)[0]+'-v3'
    CASES[new]=Case(new,old.fixture,None,None,old.command_budget_ns,
        old.verification_budget_ns,old.profile,old.proof_policy,old.pack_layout,old.proof_envelope)
    STREAMING_REFILL_CASES.append(new)
STREAMING_REFILL_CASES=tuple(STREAMING_REFILL_CASES)
RETIRED_STREAMING_CASES=STREAMING_CASES+STREAMING_BATCHED_CASES+STREAMING_REFILL_CASES

# Owner2026-10-07: streaming withdrawn; ordinary product restored byte-for-byte
# to the work-reduction source. New identity, same inputs, limits and gates.
INCUMBENT_RESTORED_CASES=[]
for original in WORK_REDUCTION_CASES:
    old=CASES[original];new=original.replace('-work-reduction-v1','-incumbent-restored-v1')
    CASES[new]=Case(new,old.fixture,None,None,old.command_budget_ns,
        old.verification_budget_ns,old.profile,old.proof_policy,old.pack_layout,old.proof_envelope)
    INCUMBENT_RESTORED_CASES.append(new)
INCUMBENT_RESTORED_CASES=tuple(INCUMBENT_RESTORED_CASES)

# Owner-dispatched pre-S8 decision: one new WAL profile, no old row retargeted.
SERVERLESS_WAL_CASE = 'phase7-sqlite-disposable-init-1000-serverless-wal-v1'
CASES[SERVERLESS_WAL_CASE] = Case(SERVERLESS_WAL_CASE, 'namespace-1000-compact-v3',
    None, None, 30_000_000_000, 19_000_000_000, 'disposable')
RETIRED_ALLOCATION_INIT = tuple(name for name, case in CASES.items()
                               if case.fixture is not None and name != SERVERLESS_WAL_CASE)

PROFILE_IDS={'durable':contract.PROFILE,'disposable':'sqlite-memory-off-macos-v1'}
# Missing user rulings are explicit; no measurement uses a guessed admission gate.
INIT_ALLOCATION_RULE="candidate-final-database-wal-shm-allocation<=matched-baseline-final-total-v1"
HISTORY_BUDGET_RULE='owner-2026-10-04-60-170-300-performance-only-v3'

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
    if selection == SERVERLESS_WAL_CASE:
        raise ValueError('single WAL Init decision is closed; retained sample is the owner-accepted baseline; no resampling')
    if selection in RETIRED_STREAMING_CASES:raise ValueError('withdrawn streaming treatment; source and receipts retained on codex/init-streaming-candidate; ordinary incumbent restored')
    if selection in RETIRED_PAYLOAD_SEGMENT_CASES:raise ValueError('payload layout withdrawn; historical receipts retained; no active vehicle')
    regression=selection in REGRESSION_CASES+SPACE_SCALING_V1_CASES+SPACE_SCALING_CASES+WORK_REDUCTION_CASES+STREAMING_CASES+STREAMING_BATCHED_CASES+STREAMING_REFILL_CASES+INCUMBENT_RESTORED_CASES
    if regression and arm!='candidate':raise ValueError('regression baseline is reused qualified Project Init at197d2fb7d; never the old Service wrapper')
    if selection in RETIRED_WAL_RESERVATION_CASES:
        raise ValueError('rejected WAL reservation selection retired; original receipts retained')
    if selection in LEGACY_HISTORY:
        raise ValueError('historical history selection retired; original receipts preserved; use current required case version')
    if case.fixture is None:
        from families import phase7_history
        return phase7_history.run(case,output,arm,baseline_root,common,corpus_root,reference_pins)
    if INIT_ALLOCATION_RULE is None:
        raise ValueError('prospective Init allocation contract is pending; no admission arm is authorized under a guessed gate')
    if selection in RETIRED_RUN_BACKED_INIT:
        raise ValueError('run-backed Init selection retired with its scratch vehicle; original receipts retained; use the acquisition case')
    if selection in RETIRED_ALLOCATION_INIT:
        raise ValueError('NOT_RUN — mechanism removed: former Init allocation/profile vehicle; historical receipts retained')
    if os.uname().sysname!='Darwin':raise ValueError('required SQLite full-sync profile and wait4 accounting are macOS-only')
    root=common.ROOT if arm=='candidate' else Path(baseline_root).resolve()
    if arm=='baseline' and (not root.is_relative_to(common.ROOT/'target/phase7-baseline') or subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()!=BASE or subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True)):
        raise ValueError('reference requires clean pinned owned checkout')
    out=common.owned(output);out.mkdir(parents=True)
    record={'schema':'phase7-sqlite-step10-v2','case':case.id,'arm':arm,'status':'NOT_RUN','sample_count':0,'verification_status':'NOT_RUN','cache_status':'INCOMPLETE','cleanup':{'status':'NOT_RUN'},'comparison_scope':'inner complete product clock including fresh database create/open, real Init, required checkpoint and final close; external child wall reported separately','margin_arithmetic':'10*candidate_ns<=11*baseline_ns','cache_contract':contract.CACHE,'requested_profile':case.profile,'profile':PROFILE_IDS[case.profile] if arm=='candidate' else 'Phase4.5 MEMORY/OFF disclosed','command_budget_ns':case.command_budget_ns,'verification_budget_ns':case.verification_budget_ns,'construction_workers':4,'environment_workers':1,'acquisition_vehicle':ACQUISITION_VEHICLE if arm=='candidate' else 'reference driver at pinned base; no provider acquisition','store_schema':ACQUISITION_STORE_SCHEMA if arm=='candidate' else 'reference','required_case_ids':REGRESSION_CASES if regression else ACQUISITION_CASES_BY_PROFILE[case.profile] if case.id in ACQUISITION_CASES_BY_PROFILE[case.profile] else WAL_RESERVATION_CASES_BY_PROFILE[case.profile] if case.id in WAL_RESERVATION_CASES_BY_PROFILE[case.profile] else SHARED_ALLOCATION_CASES_BY_PROFILE[case.profile] if case.id in SHARED_ALLOCATION_CASES_BY_PROFILE[case.profile] else OWNER_CLOSURE_CASES_BY_PROFILE[case.profile] if case.id in OWNER_CLOSURE_CASES_BY_PROFILE[case.profile] else REQUIRED_BY_PROFILE[case.profile],'allocation_rule':REGRESSION_ALLOCATION_RULE if regression else INIT_ALLOCATION_RULE,'pack_layout':case.pack_layout}
    locks=[]
    try:
        if selection in SPACE_SCALING_CASES+WORK_REDUCTION_CASES+STREAMING_CASES+STREAMING_BATCHED_CASES+STREAMING_REFILL_CASES+INCUMBENT_RESTORED_CASES:
            record['required_case_ids']=INCUMBENT_RESTORED_CASES if selection in INCUMBENT_RESTORED_CASES else STREAMING_REFILL_CASES if selection in STREAMING_REFILL_CASES else STREAMING_BATCHED_CASES if selection in STREAMING_BATCHED_CASES else STREAMING_CASES if selection in STREAMING_CASES else WORK_REDUCTION_CASES if selection in WORK_REDUCTION_CASES else SPACE_SCALING_CASES
            record['space_policy']='new acquisition Store incremental vacuum; acknowledged 512-page jobs before final checkpoint, all timed'
        if selection in WORK_REDUCTION_CASES+INCUMBENT_RESTORED_CASES:
            record['work_reduction_policy']='narrow insertion bindings; owned single-statement read snapshots; projected directory bindings; atomic final bounded disposal'
        if selection in STREAMING_CASES:
            record['streaming_policy']='512 admitted files; canonical completion consumption; one assembly Save; no file-root SQL updates or root read pass'
        if selection in STREAMING_BATCHED_CASES:
            record['streaming_policy']='512 admitted files; canonical consumption; one assembly Save; no file-root SQL; bounded demand-aware success coalescing with immediate error/idle/large-next-file flush'
        if selection in STREAMING_REFILL_CASES:
            record['streaming_policy']='512 aggregate admitted identities; refill bursts when at most256 remain; canonical completion demand; bounded success coalescing; immediate error/idle/large-next-file flush; one assembly Save and no file-root SQL'
        for p in [common.RESULTS/'phase7-sqlite.lock']+([root/'target/phase7-sqlite.lock'] if arm=='baseline' else []):
            p.parent.mkdir(parents=True,exist_ok=True);h=p.open('a+b');locks.append(h);fcntl.flock(h,fcntl.LOCK_EX|fcntl.LOCK_NB)
        if regression:
            from shared.cluster_one_control import reference
            record['regression_reference']=reference(common.ROOT,case.profile,case.fixture)
            record['comparison_kind']='same-profile current public Project Init versus cluster-one-end public Project Init; acquisition backing differs'
            record['older_competitive_control']='historical7edddb Service MEMORY/OFF split Store, reported separately only'
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
        # The candidate holds acquisition rows in the measured Store; TMPDIR stays an empty observed directory.
        command=[driver,fixture['source'],str(db),case.id,case.profile] if arm=='candidate' else [driver,fixture['source'],str(db),str(history),case.id]
        if case.command_budget_ns == 30_000_000_000: command.append(str(case.command_budget_ns//1_000_000_000))
        claim=common.RESULTS/'phase7-sqlite-sample-claims'/hashlib.sha256(json.dumps([case.id,arm,identity['source_tree'],identity['harness_seal'],record['measured_source_commit'],fixture['manifest_sha256']],sort_keys=True).encode()).hexdigest()
        claim.parent.mkdir(parents=True,exist_ok=True)
        with claim.open('x') as h:h.write(str(out)+'\n')
        perf_start=time.monotonic_ns();record['residency']=cold_native.attest(fixture['source'],record['cold_helper'],out,case.command_budget_ns,invoke,common.ROOT);record['cache_status']=record['residency']['status']
        if record['cache_status']!='PASS':record['status']='INELIGIBLE';return out
        remaining=case.command_budget_ns-(time.monotonic_ns()-perf_start)
        if remaining<=0:record['status']='NOT_RUN';record['reason']='cold attestation exhausted complete performance command budget';return out
        sample=invoke(command,out,'driver',remaining,env,root);record['sample_count']=1;record['performance']=sample;record['comparison_ns']=sample['child'].get('operation_ns') if isinstance(sample['child'],dict) else None
        record['storage']=contract.allocations([db] if arm=='candidate' else [db,history]);record['storage_bytes']=record['storage']['total_bytes'];record['cleanup']={'status':'PASS' if not list(scratch.iterdir()) and not list(out.glob('.layerfs-allocation-*')) else 'FAIL','scope':'measured child exited, TMPDIR and allocation scratch empty, database evidence retained'};record['command_wall_ns']=time.monotonic_ns()-perf_start
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
            if selection in SPACE_SCALING_CASES+WORK_REDUCTION_CASES+STREAMING_CASES+STREAMING_BATCHED_CASES+STREAMING_REFILL_CASES+INCUMBENT_RESTORED_CASES:
                lines=[line[len('SPACE_PROFILE '):] for line in (out/'driver.stderr').read_text().splitlines() if line.startswith('SPACE_PROFILE ')]
                if len(lines)!=1:raise ValueError('space-policy readback missing or ambiguous')
                record['space_profile']=json.loads(lines[0])
                if record['space_profile']['auto_vacuum']!=2 or record['space_profile']['page_budget']!=512 or not child.get('reclamation_jobs'):raise ValueError('declared bounded reclamation was not performed')
        proof=invoke([binaries['verify_namespace']['path'],str(db),str(db if arm=='candidate' else history),child['root'],child['stack'],fixture['manifest'],fixture['manifest_sha256']]+([case.profile] if arm=='candidate' else []),out,'verifier',case.verification_budget_ns,env,root)
        record['verification']=proof;record['verification_wall_ns']=proof['wall_ns'];record['verification_status']='PASS' if proof['exit_code']==0 and not proof['timed_out'] and common.lite_verification_pass(proof['child'],fixture_case,child,fixture) else 'FAIL'
        return out
    except Exception as e:
        record['status']='INCOMPLETE';record['reason']=str(e);raise
    finally:
        common.write_json(out/'receipt.json',record);common.manifest_run(out)
        for h in reversed(locks):h.close()
