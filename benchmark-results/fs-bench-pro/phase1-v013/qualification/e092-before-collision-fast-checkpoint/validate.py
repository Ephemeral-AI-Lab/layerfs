"""Exactly48 pending e092 dedup fast results; all previously qualified fast results excluded."""
import datetime,hashlib,importlib.util,json
from pathlib import Path
OUT=Path(__file__).resolve().parent;ROOT=OUT.parents[4];C=ROOT/'benchmark-results/fs-bench-pro/phase1-v013';SOURCE=ROOT/'benchmark/fs-bench-pro/generate-workspace-report.py'
SHA='329fa5da5cf54d959cb5298ee0683fc95714be9bcf0a6cb9159d05885c21b932';REV='e0922904e2bb607a138157755dab9613b441d5b9'
if hashlib.sha256(SOURCE.read_bytes()).hexdigest()!=SHA:raise ValueError('validator differs from frozen source')
spec=importlib.util.spec_from_file_location('additional30d_fast_report',SOURCE);r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)
prior=r.read(C/'qualification/78-additional-reliability-checkpoint/checkpoint.json')['prior_report'];build=r.read(C/'assets-e0922904/evidence/build.json');ledger=r.read(C/'slots.json');assert build['revision']==REV and build['status']=='pass'
invalidated={str(Path(x['previous_evidence']).resolve()) for x in [r.decode(line) for line in (C/'invalidations.jsonl').read_text().splitlines() if line]}
selected=r.read(OUT/'selected-slots.json')
rows=[];seals={};coverage=[]
for slot in selected:
    scenario=slot['case'];seed=slot['seed'];tier=int(scenario.rsplit('-',1)[1]);family=slot['family_id'];operation=scenario.rsplit('-',1)[0].removeprefix('dedup-cdc-').removeprefix('dedup-cross-file-')
    matches=[x for x in ledger.values() if x.get('source_revision')==REV and x.get('scenario_id')==scenario and x.get('seed')==seed and x.get('mode')=='fast-verify' and x['evidence_path']==slot['evidence']];assert len(matches)==1
    outcome=matches[0];p=Path(outcome['evidence_path']);assert str(p.resolve()) not in invalidated
    case=dict(kind='case',scenario_id=scenario,family_id=family,operation=operation,tier=tier,input_mode='directory',proof_only=False,inherited=False)
    destination=OUT/f'{scenario}-s{seed}-validation.json'
    if destination.exists():raise ValueError('refuse repeated scoped validation')
    value=r.validate_fast_attempt(outcome,{},case,build);r.custody.write_json(destination,value)
    if value['issues'] or value['violations'] or value['product_status']!='pass' or not value['fast_iteration_pass'] or value['verification_pass']:raise ValueError(f"{scenario}: {value['issues']} {value['violations']}")
    receipts=r.read(p/'verification/fast-verification/receipts.json');canonical=json.loads(next(x['receipt'] for x in receipts if x['kind']=='fast-canonical-verification'));native=dict(line.split('=',1) for line in next(x['receipt'] for x in receipts if x['kind']=='fast-native-verification').splitlines() if line)
    assert canonical['fully_verified']==native['fully_verified']=='false'
    cov=dict(case=scenario,seed=seed,evidence=str(p),reference_assurance=canonical['reference_assurance'],canonical=canonical,native=native)
    coverage.append(cov)
    rows.append(dict(case=scenario,family_id=family,seed=seed,mode='fast-verify',required_mode='verify',phase1_verification_accepted=True,assurance_status='fast_iteration_verified',inherited=False,source_identity=r.source_identity(outcome),source_arm=outcome['source_arm'],raw_product_status=outcome['product_status'],coverage_status=outcome['coverage_status'],product_status='pass',evidence_status='PASS',issues=[],violations=[],evidence=str(p),metrics=value['metrics'],resource_observations=value['resource_observations'],observations=value['observations'],canonical_packages=value['canonical_packages'],verification_summary=r.verification_summary(value['observations'],value['canonical_packages']),environment_identity=outcome['environment_identity'],input_identity=outcome['input_identity'],invalidation_context=[],product_source_compatibility=None,product_predicate_scope=None,measured_current_product_binary=True,performance_claim_eligible=False,verification_pass=False,fast_iteration_pass=True,counts_toward_full_phase1_gate=False,counts_toward_routine_phase1_acceptance=True,fast_coverage=cov))
    seals[str(p)]=r.custody.sha(p/'evidence.sha256')
assert len(rows)==48 and len({x['environment_identity'] for x in rows})==1
if r.custody.sha(SOURCE)!=SHA:raise ValueError('report changed during validation')
r.custody.write_json(OUT/'incremental-fast-rows.json',dict(schema='phase1-incremental-fast-verification-rows-v1',rows=rows,source_revision=REV,report_generator_sha256=SHA,attempt_manifest_sha256=seals,prior_report_sha256=prior['sha256']))
r.custody.write_json(OUT/'coverage-and-omissions.json',dict(schema='phase1-fast-coverage-and-omissions-v1',assurance_status='fast_iteration_verified',fully_verified=False,rows=coverage,scope='Full current namespace/global inode/metadata/alias authentication, complete native namespace membership/types, independently selected changed/uncertified/witness bytes and native metadata. Explicit certified unchanged bodies and untouched native metadata omitted as recorded; full canonical census is not performed.'))
r.custody.write_json(OUT/'checkpoint.json',dict(schema='phase1-additional-fast-checkpoint-v1',status='PASS',assurance_status='fast_iteration_verified',new_fast_proofs_validated=48,new_full_proofs_validated=0,recorded_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),actual_source_revision=REV,report_generator_sha256=SHA,prior_report=prior,attempt_manifest_sha256=seals,issues=[],violations=[],explicit_exclusions=['All12 already qualified mixed fast results','First CDC common-body1 seed1 fast82fd0eb49817 already qualified','Failed CDC delete500 seed3 bf6d01cc7939','Unexecuted cross-file unique100 seed3 bcde3546377c','All prior full proofs/performance'],scope='Routine Phase1 fast acceptance only; no exhaustive verification credit and no successor product timing equivalence. Later deletion-fixture collision correction is a separate exact source/input bridge; no changed input is present in this cohort.',product_executions=0,full_report_reruns=0))
r.custody.seal(OUT)
print(json.dumps(dict(status='PASS',assurance='fast_iteration_verified',new_fast_proofs=48,new_full_proofs=0,issues=0,violations=0,receipt=str(OUT/'incremental-fast-rows.json'))))
