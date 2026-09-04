"""Incremental qualification of exactly two newly sealed full proofs."""
import datetime,hashlib,importlib.util,json
from pathlib import Path

OUT=Path(__file__).resolve().parent
ROOT=OUT.parents[4]
CAMPAIGN=ROOT/'benchmark-results/fs-bench-pro/phase1-v013'
SOURCE=ROOT/'benchmark/fs-bench-pro/generate-workspace-report.py'
spec=importlib.util.spec_from_file_location('incremental_e324_report',SOURCE)
report=importlib.util.module_from_spec(spec);spec.loader.exec_module(report)
validator_sha=report.custody.sha(SOURCE)
prior_path=CAMPAIGN/'results/review.json';prior_bytes=prior_path.read_bytes();prior=json.loads(prior_bytes);prior_sha=hashlib.sha256(prior_bytes).hexdigest()
if prior['source']['revision']!='e32469e975e8e185ca525b02bb71d70bafa4e865' or prior['global_issues'] or prior['counts']['invalid_slots'] or prior['counts']['product_failed_outcomes']:raise ValueError('prior qualified report identity/gates differ')
if f'{prior_sha}  review.json\n' not in (CAMPAIGN/'results/evidence.sha256').read_text():raise ValueError('prior report seal mismatch')
ledger=report.read(CAMPAIGN/'slots.json');rows=[];manifest_hashes={};joins=[];validations={}
for name,scenario,seed,tier in [('tiny-stat-1-s3-verify-a1c6c8d3ac93','tiny-stat-1',3,1),('tiny-stat-10-s1-verify-e31e225c0f45','tiny-stat-10',1,10)]:
    directory=CAMPAIGN/'attempts'/name
    if not (directory/'outcome.json').is_file() or not (directory/'evidence.sha256').is_file():raise ValueError('proof is not sealed yet')
    outcome=report.read(directory/'outcome.json')
    selected=[entry for entry in ledger.values() if entry.get('evidence_path')==str(directory)]
    if len(selected)!=1 or any(selected[0].get(key)!=value for key,value in outcome.items()):raise ValueError('exact proof ledger/outcome mismatch')
    if outcome.get('source_revision')!=prior['source']['revision'] or outcome.get('scenario_id')!=scenario or outcome.get('seed')!=seed or outcome.get('mode')!='verify':raise ValueError('wrong exact proof identity')
    case=dict(kind='case',scenario_id=scenario,family_id='tiny_file_churn',operation='tiny-stat',tier=tier,input_mode='store',proof_only=False,inherited=False)
    result=report.validate_attempt(outcome,{},case,prior['source'])
    report.custody.write_json(OUT/f'{scenario}-s{seed}-validation.json',result)
    if result['issues'] or result['violations'] or result['product_status']!='pass' or not result['verification_pass']:raise ValueError(f'{name}: full proof validation failed')
    perf=next(row for row in prior['rows'] if row['case']==scenario and row['seed']==seed and row['mode']=='performance')
    if perf['evidence_status']!='PASS' or perf['product_status']!='pass' or perf['input_identity']!=outcome['input_identity'] or perf['environment_identity']!=outcome['environment_identity']:raise ValueError('qualified performance/input/VM8 environment mismatch')
    bridge=next((item for item in prior['verification_compatibility'] if item['family_id']=='tiny_file_churn' and item['performance_revision']==perf['source_identity']['source_revision'] and item['verification_revision']==outcome['source_revision']),None)
    if bridge is None:raise ValueError('prior report lacks exact performance/full-proof source join')
    row=dict(case=scenario,family_id='tiny_file_churn',seed=seed,mode='verify',assurance_status='fully_verified',inherited=False,
             source_identity={key:outcome.get(key) for key in report.IDENTITY_FIELDS},source_arm=outcome.get('source_arm'),raw_product_status=outcome.get('product_status'),coverage_status=outcome.get('coverage_status'),
             product_status=result['product_status'],evidence_status='PASS',issues=result['issues'],violations=result['violations'],evidence=str(directory),metrics=result['metrics'],resource_observations=result['resource_observations'],observations=result['observations'],canonical_packages=result['canonical_packages'],
             verification_summary=report.verification_summary(result['observations'],result['canonical_packages']),environment_identity=outcome.get('environment_identity'),input_identity=outcome.get('input_identity'),invalidation_context=[],product_source_compatibility=None,product_predicate_scope=None,measured_current_product_binary=True,verification_source_compatibility='identical sealed source',performance_claim_eligible=False,verification_pass=True)
    rows.append(row);manifest_hashes[str(directory)]=report.custody.sha(directory/'evidence.sha256')
    joins.append(dict(case=scenario,seed=seed,performance_evidence=perf['evidence'],full_verification_evidence=str(directory),input_identity=outcome['input_identity'],environment_identity=outcome['environment_identity'],source_bridge=bridge,performance_row_revalidated=False))
    validations[name]=dict(issues=result['issues'],violations=result['violations'],verification_pass=result['verification_pass'])
if report.custody.sha(SOURCE)!=validator_sha:raise ValueError('validator source changed during checkpoint')
payload=CAMPAIGN/'qualification/issue-23-final-checkpoint/family-completion.json'
value=dict(schema='phase1-incremental-full-verification-rows-v1',rows=rows,source_revision=prior['source']['revision'],report_generator_sha256=validator_sha,attempt_manifest_sha256=manifest_hashes,prior_report_sha256=prior_sha)
report.custody.write_json(OUT/'incremental-full-rows.json',value)
report.custody.write_json(OUT/'checkpoint.json',dict(schema='phase1-retained-full-proof-checkpoint-v1',status='PASS',recorded_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),new_full_proofs_validated=2,source_revision=prior['source']['revision'],report_generator_sha256=validator_sha,
    prior_report=dict(path=str(prior_path),sha256=prior_sha,validated_performance_rows_reused=373,full_report_reruns=0),payload_family_checkpoint=dict(path=str(payload),sha256=report.custody.sha(payload),seal_sha256=report.custody.sha(payload.parent/'evidence.sha256'),revalidated=False),attempt_manifest_sha256=manifest_hashes,validations=validations,joins=joins,
    scope='Two new VM8 fully_verified rows only. Prior373 performance,37 VM8 proofs and standalone endurance remain reusable through their existing qualified report; payload final checkpoint remains separately sealed. No product execution, no full-report regeneration, no global terminal claim.'))
report.custody.seal(OUT)
print(json.dumps(dict(status='PASS',new_full_proofs=2,issues=0,violations=0,incremental_rows=str(OUT/'incremental-full-rows.json'),report_generator_sha256=validator_sha)))
