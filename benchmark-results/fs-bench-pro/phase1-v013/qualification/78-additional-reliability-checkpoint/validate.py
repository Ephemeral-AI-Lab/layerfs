"""Exactly five new78 plus one repaired30d reliability verdicts; all prior verdicts are reused without revalidation."""
import datetime,hashlib,importlib.util,json,subprocess
from pathlib import Path
OUT=Path(__file__).resolve().parent;ROOT=OUT.parents[4];CAMPAIGN=ROOT/'benchmark-results/fs-bench-pro/phase1-v013'
SOURCE=ROOT/'benchmark/fs-bench-pro/generate-workspace-report.py';validator_revision='30d13deeec72b46ff7bc411f1ec08a46990541e1'
source_bytes=SOURCE.read_bytes()
if hashlib.sha256(source_bytes).hexdigest()!='01160a1d01252763f99eaa7f0fc05694695ca23152206769577e57b597a485e8':raise ValueError('validator differs from exact frozen report bytes')
spec=importlib.util.spec_from_file_location('additional78_report',SOURCE);report=importlib.util.module_from_spec(spec);spec.loader.exec_module(report)
validator_sha=hashlib.sha256(source_bytes).hexdigest()
prior_path=CAMPAIGN/'results/review.json';prior_bytes=prior_path.read_bytes();prior=json.loads(prior_bytes);prior_sha=hashlib.sha256(prior_bytes).hexdigest()
published='b1f6ff18fe3e16b89989a0d0b5b665e56980a984';relative='benchmark-results/fs-bench-pro/phase1-v013/results/review.json'
expected_blob=subprocess.check_output(['git','rev-parse',f'{published}:{relative}'],cwd=ROOT,text=True).strip()
actual_blob=subprocess.check_output(['git','hash-object','--stdin'],cwd=ROOT,input=prior_bytes).decode().strip()
if expected_blob!=actual_blob or f'{prior_sha}  review.json\n' not in (CAMPAIGN/'results/evidence.sha256').read_text():raise ValueError('prior report not exact authenticatedb1f publication')
if prior['global_issues'] or prior['counts']['invalid_slots'] or prior['counts']['product_failed_outcomes']:raise ValueError('prior report validity differs')
build=report.read(CAMPAIGN/'assets-78d0f46d/evidence/build.json');actual_source='78d0f46d90744bbce729909cdf57f6eafe2eb9e6'
if build.get('status')!='pass' or build['revision']!=actual_source:raise ValueError('target78 proofbuild mismatch')
ledger=report.read(CAMPAIGN/'slots.json');invalidated={str(Path(row['previous_evidence']).resolve()) for row in (report.decode(line) for line in (CAMPAIGN/'invalidations.jsonl').read_text().splitlines() if line)}
selected=[('workload-cancel','c7a34133754e'),('dirty-runtime-disconnect','2e969fd5d84f'),('corrupt-descendant','3c8a1f12c1e2'),('missing-descendant','fbe2a2784bad'),('parallel-read-write','2f940d084aa8'),('shared-path-contention','278b3a754568')]
bridge=report.readonly_pin_source_proof(actual_source,validator_revision)
report.custody.write_json(OUT/'readonly-pin-functional-compatibility.json',dict(source_proof=bridge,scope='Only independently qualified healthy full-state proofs retained at actual78 source; shared-path-contention failure excluded and requires repaired execution. No cost equivalence or affected45 timing retention.'))
rows=[];manifest_hashes={};validations={};joins=[]
for operation,suffix in selected:
    scenario=f'workspace-{operation}-proof'
    row_source=validator_revision if operation=='shared-path-contention' else actual_source
    row_build=report.read(CAMPAIGN/'assets-30d13dee/evidence/build.json') if operation=='shared-path-contention' else build
    if row_build.get('status')!='pass' or row_build['revision']!=row_source:raise ValueError('row build mismatch')
    matches=[row for row in ledger.values() if row.get('scenario_id')==scenario and row.get('seed')==1 and row.get('mode')=='verify' and row.get('source_revision')==row_source and row.get('evidence_path','').endswith(suffix)]
    if len(matches)!=1:raise ValueError('exact additional proof missing/ambiguous')
    outcome=matches[0];directory=Path(outcome['evidence_path'])
    if str(directory.resolve()) in invalidated:raise ValueError('selected proof was invalidated')
    case=dict(kind='case',scenario_id=scenario,family_id='workspace_reliability',operation=operation,tier=1,input_mode='store',proof_only=True,inherited=False)
    if (OUT/f'{operation}-validation.json').exists():raise ValueError('refuse to repeat existing scoped validation')
    value=report.validate_attempt(outcome,{},case,row_build)
    predicate_issues=[]
    predicate=report.validate_readonly_pin_records([],case,outcome,predicate_issues)
    if predicate_issues:raise ValueError(predicate_issues)
    report.custody.write_json(OUT/f'{operation}-validation.json',value)
    if value['issues'] or value['violations'] or value['product_status']!='pass' or not value['verification_pass']:raise ValueError(f'{scenario}: full proof validation failed')
    row=dict(case=scenario,family_id='workspace_reliability',seed=1,mode='verify',assurance_status='fully_verified',inherited=False,source_identity={key:outcome.get(key) for key in report.IDENTITY_FIELDS},source_arm=outcome.get('source_arm'),raw_product_status=outcome.get('product_status'),coverage_status=outcome.get('coverage_status'),product_status='pass',evidence_status='PASS',issues=[],violations=[],evidence=str(directory),metrics=value['metrics'],resource_observations=value['resource_observations'],observations=value['observations'],canonical_packages=value['canonical_packages'],verification_summary=report.verification_summary(value['observations'],value['canonical_packages']),environment_identity=outcome['environment_identity'],input_identity=outcome['input_identity'],invalidation_context=[],product_source_compatibility=(dict(kind=report.READONLY_PIN_BRIDGE_KIND,old_revision=actual_source,new_revision=validator_revision,source_proof=bridge) if row_source==actual_source else None),product_predicate_scope=(predicate if row_source==actual_source else None),measured_current_product_binary=(row_source==validator_revision),verification_source_compatibility=('actual78 full state retained through exact readonly Pin acknowledgement repair' if row_source==actual_source else 'identical sealed30d source'),performance_claim_eligible=False,verification_pass=True)
    rows.append(row);manifest_hashes[str(directory)]=report.custody.sha(directory/'evidence.sha256')
    validations[scenario]=dict(verification_pass=True,issues=[],violations=[],external_process_wall_ns=outcome['external_process_wall_ns'],supervisor_cleanup_status=outcome['supervisor_cleanup_status'])
    joins.append(dict(case=scenario,seed=1,full_verification_evidence=str(directory),source_identity=report.source_identity(outcome),input_identity=outcome['input_identity'],environment_identity=outcome['environment_identity'],performance_pair='not applicable: standalone full proof',prepared_input_binding='existing validate_attempt authenticated acquisition/master/clone/producer and independent full-state proof'))
if len(rows)!=6 or len({row['environment_identity'] for row in rows})!=1 or len({row['input_identity'] for row in rows})!=1:raise ValueError('additional cohort exactcount/input/environment mismatch')
if report.custody.sha(SOURCE)!=validator_sha:raise ValueError('report changed during scoped validations')
report.custody.write_json(OUT/'incremental-full-rows.json',dict(schema='phase1-incremental-full-verification-rows-v1',rows=rows,source_revision=validator_revision,source_revisions=sorted({r["source_identity"]["source_revision"] for r in rows}),report_generator_sha256=validator_sha,attempt_manifest_sha256=manifest_hashes,prior_report_sha256=prior_sha))
report.custody.write_json(OUT/'checkpoint.json',dict(schema='phase1-additional-reliability-checkpoint-v1',status='PASS',recorded_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),new_full_proofs_validated=6,actual_source_revisions=sorted({r["source_identity"]["source_revision"] for r in rows}),validator_source_revision=None,validator_worktree_base_revision=validator_revision,report_generator_sha256=validator_sha,prior_report=dict(path=str(prior_path),sha256=prior_sha,published_commit=published,git_blob=expected_blob),reused_without_revalidation=dict(performance='all prior rows; affected45 replacement handled separately',full_verdicts='all prior qualified receipts'),explicit_exclusions=['failed shared-path-contention56ec remains failed','all previously qualified full proofs and performance','45 replacement timings owned by coordinator'],attempt_manifest_sha256=manifest_hashes,validations=validations,joins=joins,scope='Five actual78 standalone full-proof successes plus repaired30d contention success, with explicit readonly-Pin functional source compatibility for old78. Preserve failed contention evidence. No global terminal claim or successor performance-cost assertion.',product_executions=0,full_report_reruns=0))
report.custody.seal(OUT)
print(json.dumps(dict(status='PASS',new_full_proofs=6,issues=0,violations=0,incremental_rows=str(OUT/'incremental-full-rows.json'),validator_sha256=validator_sha)))
