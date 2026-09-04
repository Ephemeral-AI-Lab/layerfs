"""Validate exactly five retained fb5 proofs; no other verdict is recomputed."""
import datetime,hashlib,importlib.util,json,subprocess
from pathlib import Path
OUT=Path(__file__).resolve().parent;ROOT=OUT.parents[4];CAMPAIGN=ROOT/'benchmark-results/fs-bench-pro/phase1-v013'
SOURCE=ROOT/'benchmark/fs-bench-pro/generate-workspace-report.py';revision='2013fae67a455629a27bf6414ea90f03bc8bad3a'
validator_bytes=SOURCE.read_bytes()
if validator_bytes!=subprocess.check_output(['git','show',f'{revision}:benchmark/fs-bench-pro/generate-workspace-report.py'],cwd=ROOT):raise ValueError('validator differs from specified committed2013 source')
spec=importlib.util.spec_from_file_location('fb5_additional_report',SOURCE);report=importlib.util.module_from_spec(spec);spec.loader.exec_module(report)
validator_sha=hashlib.sha256(validator_bytes).hexdigest()
prior_path=CAMPAIGN/'results/review.json';prior_bytes=prior_path.read_bytes();prior=json.loads(prior_bytes);prior_sha=hashlib.sha256(prior_bytes).hexdigest()
if f'{prior_sha}  review.json\n' not in (CAMPAIGN/'results/evidence.sha256').read_text():raise ValueError('prior report seal mismatch')
if prior['global_issues'] or prior['counts']['invalid_slots'] or prior['counts']['product_failed_outcomes']:raise ValueError('prior qualified report invalid')
build=report.read(CAMPAIGN/'assets-fb5b34f7/evidence/build.json')
if build.get('status')!='pass' or build['revision']!='fb5b34f7a882e257cd3647591fbd6c7f6ac6c2ec':raise ValueError('target fb5 build mismatch')
config_bytes=(CAMPAIGN/'evidence-builds.json').read_bytes();config=json.loads(config_bytes)
ledger=report.read(CAMPAIGN/'slots.json')
invalidated={str(Path(row['previous_evidence']).resolve()) for row in (report.decode(line) for line in (CAMPAIGN/'invalidations.jsonl').read_text().splitlines() if line)}
selected=[('tiny-stat-10',2,10,'tiny-stat','c62a1a011021'),('tiny-stat-10',3,10,'tiny-stat','ac814d4a1d74'),('tiny-stat-100',1,100,'tiny-stat','09022c0921bc'),('workspace-invalid-sdk-edit-proof',1,1,'invalid-sdk-edit','ec3c59af3544'),('workspace-invalid-namespace-proof',1,1,'invalid-namespace','d9b058799f49')]
rows=[];manifest_hashes={};joins=[];validations={};source_bridges={}
for scenario,seed,tier,operation,suffix in selected:
    matches=[row for row in ledger.values() if row.get('scenario_id')==scenario and row.get('seed')==seed and row.get('mode')=='verify' and row.get('source_revision')==build['revision'] and row.get('evidence_path','').endswith(suffix)]
    if len(matches)!=1:raise ValueError('exact selected proof missing/ambiguous')
    outcome=matches[0];directory=Path(outcome['evidence_path'])
    if str(directory.resolve()) in invalidated:raise ValueError('selected proof explicitly invalidated')
    proof_only=scenario.startswith('workspace-');family='workspace_reliability' if proof_only else 'tiny_file_churn'
    case=dict(kind='case',scenario_id=scenario,family_id=family,operation=operation,tier=tier,input_mode='store',proof_only=proof_only,inherited=False)
    result=report.validate_attempt(outcome,{},case,build)
    report.custody.write_json(OUT/f'{scenario}-s{seed}-validation.json',result)
    if result['issues'] or result['violations'] or result['product_status']!='pass' or not result['verification_pass']:raise ValueError(f'{scenario}: scoped full validation failed')
    row=dict(case=scenario,family_id=family,seed=seed,mode='verify',assurance_status='fully_verified',inherited=False,source_identity={key:outcome.get(key) for key in report.IDENTITY_FIELDS},source_arm=outcome.get('source_arm'),raw_product_status=outcome.get('product_status'),coverage_status=outcome.get('coverage_status'),product_status='pass',evidence_status='PASS',issues=[],violations=[],evidence=str(directory),metrics=result['metrics'],resource_observations=result['resource_observations'],observations=result['observations'],canonical_packages=result['canonical_packages'],verification_summary=report.verification_summary(result['observations'],result['canonical_packages']),environment_identity=outcome['environment_identity'],input_identity=outcome['input_identity'],invalidation_context=[],product_source_compatibility=None,product_predicate_scope=None,measured_current_product_binary=True,verification_source_compatibility='identical sealed source',performance_claim_eligible=False,verification_pass=True)
    if not proof_only:
        perf=next(item for item in prior['rows'] if item['case']==scenario and item['seed']==seed and item['mode']=='performance')
        if perf['evidence_status']!='PASS' or perf['product_status']!='pass' or perf['input_identity']!=outcome['input_identity'] or perf['environment_identity']!=outcome['environment_identity']:raise ValueError('tiny proof input/VM8 join differs from qualified performance')
        bridge=next((item for item in config['verification_compatibility'] if item['family_id']==family and item['performance_revision']==perf['source_identity']['source_revision'] and item['verification_revision']==outcome['source_revision']),None)
        if bridge is None:raise ValueError('missing exact current source join')
        if family not in source_bridges:
            if set(bridge['unchanged_paths'])!=report.bridge_dependency_paths(family):raise ValueError('source join omitted workload/oracle dependencies')
            for filename,want in bridge['unchanged_paths'].items():report.validate_bridge_path(filename,want,[bridge['performance_revision'],bridge['verification_revision']])
            source_bridges[family]=bridge
        joins.append(dict(case=scenario,seed=seed,performance_evidence=perf['evidence'],full_verification_evidence=str(directory),source_bridge_family=family,input_identity=outcome['input_identity'],environment_identity=outcome['environment_identity'],existing_performance_revalidated=False))
    else:
        joins.append(dict(case=scenario,seed=seed,full_verification_evidence=str(directory),source_identity=report.source_identity(outcome),input_identity=outcome['input_identity'],environment_identity=outcome['environment_identity'],performance_pair='not applicable: standalone full proof, no timed distribution'))
    rows.append(row);manifest_hashes[str(directory)]=report.custody.sha(directory/'evidence.sha256');validations[scenario+f':{seed}']=dict(verification_pass=True,issues=[],violations=[],input_identity=outcome['input_identity'],environment_identity=outcome['environment_identity'],source_revision=outcome['source_revision'])
if len({row['environment_identity'] for row in rows})!=1:raise ValueError('selected rows mix runtime environments')
if report.custody.sha(SOURCE)!=validator_sha:raise ValueError('validator source changed during scoped checks')
prior_checkpoints=[]
for name,file in [('issue-23-final-checkpoint','family-completion.json'),('full-verifier-e32469e9-retained-checkpoint','checkpoint.json'),('capped-final-checkpoint','family-completion.json')]:
    path=CAMPAIGN/'qualification'/name/file
    prior_checkpoints.append(dict(path=str(path),sha256=report.custody.sha(path),manifest_sha256=report.custody.sha(path.parent/'evidence.sha256'),revalidated=False))
report.custody.write_json(OUT/'incremental-full-rows.json',dict(schema='phase1-incremental-full-verification-rows-v1',rows=rows,source_revision=build['revision'],report_generator_sha256=validator_sha,attempt_manifest_sha256=manifest_hashes,prior_report_sha256=prior_sha))
report.custody.write_json(OUT/'checkpoint.json',dict(schema='phase1-additional-retained-full-proof-checkpoint-v1',status='PASS',recorded_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),new_full_proofs_validated=5,proof_counts=dict(tiny_stat=3,reliability_standalone=2),actual_source_revision=build['revision'],validator_source_revision=revision,report_generator_sha256=validator_sha,prior_report=dict(path=str(prior_path),sha256=prior_sha,qualified_performance_rows_reused=373,prior_full_verdicts_reused=46),prior_checkpoints=prior_checkpoints,current_source_mapping_sha256=hashlib.sha256(config_bytes).hexdigest(),attempt_manifest_sha256=manifest_hashes,validations=validations,joins=joins,source_bridges=source_bridges,scope='Only five new full verdicts computed. Owner-drop successor product compatibility is a separate binder gate; these rows preserve actual fb5 source/environment and do not assert successor timing cost or global terminal pass.',product_executions=0,full_report_reruns=0))
report.custody.seal(OUT)
print(json.dumps(dict(status='PASS',new_full_proofs=5,tiny_stat=3,reliability=2,issues=0,violations=0,incremental_rows=str(OUT/'incremental-full-rows.json'),validator_sha256=validator_sha)))
