"""Validate only the five new capped full proofs; reuse25 published performance rows."""
import datetime,hashlib,importlib.util,json,subprocess
from pathlib import Path

OUT=Path(__file__).resolve().parent;ROOT=OUT.parents[4];CAMPAIGN=ROOT/'benchmark-results/fs-bench-pro/phase1-v013'
SOURCE=ROOT/'benchmark/fs-bench-pro/generate-workspace-report.py'
spec=importlib.util.spec_from_file_location('capped_checkpoint_report',SOURCE);report=importlib.util.module_from_spec(spec);spec.loader.exec_module(report)
validator_sha=report.custody.sha(SOURCE)
published='b1f6ff18fe3e16b89989a0d0b5b665e56980a984'
report_relative='benchmark-results/fs-bench-pro/phase1-v013/results/review.json'
report_path=ROOT/report_relative;report_bytes=report_path.read_bytes();prior=json.loads(report_bytes)
blob=subprocess.check_output(['git','rev-parse',f'{published}:{report_relative}'],cwd=ROOT,text=True).strip()
actual_blob=subprocess.check_output(['git','hash-object','--stdin'],cwd=ROOT,input=report_bytes).decode().strip()
if blob!=actual_blob:raise ValueError('input report is not exact publishedb1f report')
prior_sha=hashlib.sha256(report_bytes).hexdigest()
if f'{prior_sha}  review.json\n' not in (CAMPAIGN/'results/evidence.sha256').read_text():raise ValueError('published report seal mismatch')
if prior['global_issues'] or prior['counts']['invalid_slots'] or prior['counts']['product_failed_outcomes']:raise ValueError('published report has unresolved validity/failure gates')
performance=[row for row in prior['rows'] if row['family_id']=='edit_length_changing_capped' and row['mode']=='performance']
ids={row['case'] for row in performance}
if len(ids)!=5 or len(performance)!=25 or {(row['case'],row['seed']) for row in performance}!={(case,rep) for case in ids for rep in range(1,6)}:raise ValueError('capped performance identity/repetition count mismatch')
for row in performance:
    if row['evidence_status']!='PASS' or row['product_status']!='pass' or row['issues'] or row['violations'] or row['invalidation_context']:raise ValueError('published capped performance row not qualified')
if ids&set(report.read(CAMPAIGN/'phase1-runtime-suppressions.json')['cases']):raise ValueError('capped scope is suppressed')
config_bytes=(CAMPAIGN/'evidence-builds.json').read_bytes();config=json.loads(config_bytes)
build_path=CAMPAIGN/'assets-fb5b34f7/evidence/build.json';build=report.read(build_path)
if build.get('status')!='pass' or build['revision']!='fb5b34f7a882e257cd3647591fbd6c7f6ac6c2ec':raise ValueError('unexpected capped proof build')
source_proof=report.configured_full_verifier_bridge(config,build)
bridge=next((item for item in config['verification_compatibility'] if item['family_id']=='edit_length_changing_capped' and item['performance_revision']=='7948df2de269e5ffd47a232ffd8091ff83f8869f' and item['verification_revision']==build['revision']),None)
if bridge is None or set(bridge['unchanged_paths'])!=report.bridge_dependency_paths('edit_length_changing_capped'):raise ValueError('missing full exact capped source join')
for filename,want in bridge['unchanged_paths'].items():report.validate_bridge_path(filename,want,[bridge['performance_revision'],bridge['verification_revision']])
ledger=report.read(CAMPAIGN/'slots.json');proofs=[];validations={};identities=[];joins=[]
invalidated={str(Path(row['previous_evidence']).resolve()) for row in (report.decode(line) for line in (CAMPAIGN/'invalidations.jsonl').read_text().splitlines() if line)}
for scenario in sorted(ids):
    matches=[row for row in ledger.values() if row.get('scenario_id')==scenario and row.get('seed')==1 and row.get('mode')=='verify' and row.get('source_revision')==build['revision']]
    if len(matches)!=1:raise ValueError('capped full proof missing/ambiguous')
    outcome=matches[0];directory=Path(outcome['evidence_path'])
    if str(directory.resolve()) in invalidated:raise ValueError('capped proof was invalidated')
    case=dict(kind='case',scenario_id=scenario,family_id='edit_length_changing_capped',operation=scenario.split('-input-',1)[0],tier=500,input_mode='store',proof_only=False,inherited=True)
    result=report.validate_attempt(outcome,{},case,build)
    report.custody.write_json(OUT/f'{scenario}-validation.json',result)
    if result['issues'] or result['violations'] or result['product_status']!='pass' or not result['verification_pass']:raise ValueError(f'new capped full proof failed: {scenario}')
    if outcome['hard_deadline_seconds']!=30 or outcome['external_process_wall_ns']>30_000_000_000:raise ValueError('capped full verifier exceeded existing30sdeadline')
    row=dict(case=scenario,family_id='edit_length_changing_capped',seed=1,mode='verify',assurance_status='fully_verified',inherited=True,source_identity={key:outcome.get(key) for key in report.IDENTITY_FIELDS},source_arm=outcome.get('source_arm'),raw_product_status=outcome.get('product_status'),coverage_status=outcome.get('coverage_status'),product_status='pass',evidence_status='PASS',issues=[],violations=[],evidence=str(directory),metrics=result['metrics'],resource_observations=result['resource_observations'],observations=result['observations'],canonical_packages=result['canonical_packages'],verification_summary=report.verification_summary(result['observations'],result['canonical_packages']),environment_identity=outcome['environment_identity'],input_identity=outcome['input_identity'],invalidation_context=[],product_source_compatibility=None,product_predicate_scope=None,measured_current_product_binary=True,verification_source_compatibility='identical sealed source',performance_claim_eligible=False,verification_pass=True)
    proofs.append(row);validations[scenario]=dict(verification_pass=True,issues=[],violations=[],hard_deadline_seconds=30,external_process_wall_ns=outcome['external_process_wall_ns'],supervisor_cleanup_status=outcome['supervisor_cleanup_status'],mutable_sample_cleanup_status=outcome['mutable_sample_cleanup_status'])
    for perf in [item for item in performance if item['case']==scenario]:
        if perf['input_identity']!=row['input_identity'] or perf['environment_identity']!=row['environment_identity'] or perf['source_identity']['source_revision']!=bridge['performance_revision']:raise ValueError('capped repetition/full-proof source,input orVM8environment differs')
        joins.append(dict(case=scenario,repetition=perf['seed'],performance_evidence=perf['evidence'],full_verification_evidence=row['evidence'],input_identity=row['input_identity'],environment_identity=row['environment_identity'],performance_claim_eligible=True))
for row in performance+proofs:
    identities.append({key:row[key] for key in ('case','seed','mode','source_identity','environment_identity','input_identity','evidence','evidence_status','product_status')} | {'assurance_status':row.get('assurance_status'),'attempt_manifest_sha256':report.custody.sha(Path(row['evidence'])/'evidence.sha256')})
invocations=[];covered=set()
for path in (CAMPAIGN/'invocations').glob('*.json'):
    value=report.read(path);members={(slot[0],slot[1],slot[2]) for slot in value.get('planned_slots',[]) if slot[0] in ids and slot[2]=='verify'}
    if value.get('source_revision')!=build['revision'] or value.get('image_id')!=build['image_id'] or not members:continue
    if value.get('status')!='pass':raise ValueError('capped proof invocation incomplete/failed')
    if report.number(value.get('invocation_wall_ns'),'CLI wall')<report.number(value.get('source_validation_ns'),'source validation')+report.number(value.get('registry_query_ns'),'registry query'):raise ValueError('CLI wall omitsrequiredwork')
    covered.update(members);invocations.append(dict(path=str(path),sha256=report.custody.sha(path)))
if covered!={(case,1,'verify') for case in ids}:raise ValueError('capped full invocation coverage mismatch')
if report.custody.sha(SOURCE)!=validator_sha:raise ValueError('validator changed during checkpoint')
manifest_hashes={row['evidence']:report.custody.sha(Path(row['evidence'])/'evidence.sha256') for row in proofs}
report.custody.write_json(OUT/'incremental-full-rows.json',dict(schema='phase1-incremental-full-verification-rows-v1',rows=proofs,source_revision=build['revision'],report_generator_sha256=validator_sha,attempt_manifest_sha256=manifest_hashes,prior_report_sha256=prior_sha))
receipt=dict(schema='phase1-capped-completion-checkpoint-v1',recorded_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),family_status='PASS',family_id='edit_length_changing_capped',counts=dict(cases=5,performance=25,full_verification=5,missing=0,invalid=0,failures=0),input_report=dict(published_commit=published,path=str(report_path),git_blob=blob,sha256=prior_sha,report_generator_sha256=prior['report_generator_sha256']),validator_sha256=validator_sha,new_build_manifest_sha256=report.custody.sha(build_path),current_source_mapping_sha256=hashlib.sha256(config_bytes).hexdigest(),full_verifier_source_compatibility=source_proof,performance_to_full_source_join=bridge,identities=identities,joins=joins,validations=validations,invocations=invocations,validation_scope=dict(new_full_proofs_validated=5,qualified_performance_rows_reused=25,performance_revalidated=False,full_report_reruns=0,product_executions=0),inherited_disposition='Five versioned bounded replacements are complete. Five oversized original definitions remain preserved, not executed under the incompatible cap. Other58 inherited definitions/results retain historical release disposition; no58freshpasses or overallreleasequalification asserted.',limitations='Capped component of#35 only. No#35closure orPHASE1_TERMINAL_PASS; othermandatoryfamilyproofscontinue. All original failures andsource/environmentobservationsremainpreserved.')
report.custody.write_json(OUT/'family-completion.json',receipt);report.custody.seal(OUT)
print(json.dumps(dict(family_status='PASS',cases=5,performance=25,full_verification=5,new_full_validations=5,reused_performance=25,issues=0,violations=0,receipt=str(OUT/'family-completion.json'))))
