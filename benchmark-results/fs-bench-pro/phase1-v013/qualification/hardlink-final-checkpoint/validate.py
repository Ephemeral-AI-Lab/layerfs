"""Exactly one corrected hardlink proof; all previous verdicts reused."""
import datetime,hashlib,importlib.util,json
from pathlib import Path
OUT=Path(__file__).resolve().parent;ROOT=OUT.parents[4];C=ROOT/'benchmark-results/fs-bench-pro/phase1-v013';SOURCE=ROOT/'benchmark/fs-bench-pro/generate-workspace-report.py'
SHA='1289ff9d8089195c78f9a61bdc19f50eedf374234cd2769acb573ae2e01b53d8';REV='e0922904e2bb607a138157755dab9613b441d5b9'
if hashlib.sha256(SOURCE.read_bytes()).hexdigest()!=SHA:raise ValueError('validator differs from frozen source')
spec=importlib.util.spec_from_file_location('hardlink_final_report',SOURCE);r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)
prior=r.read(C/'qualification/78-additional-reliability-checkpoint/checkpoint.json')['prior_report'];build=r.read(C/'assets-e0922904/evidence/build.json');ledger=r.read(C/'slots.json')
assert build['status']=='pass' and build['revision']==REV
invalidated={str(Path(x['previous_evidence']).resolve()) for x in [r.decode(line) for line in (C/'invalidations.jsonl').read_text().splitlines() if line]}
selected=[('hardlink-alias','9c319a18a1b8')]
rows=[];manifest_hashes={};validations={};joins=[]
for operation,suffix in selected:
    cdc=operation=='boundaries';scenario='dedup-cdc-boundaries-proof' if cdc else f'workspace-{operation}-proof';family='dedup_cdc_locality' if cdc else 'workspace_reliability'
    matches=[x for x in ledger.values() if x.get('source_revision')==REV and x.get('scenario_id')==scenario and x.get('seed')==1 and x.get('mode')=='verify' and x['evidence_path'].endswith(suffix)]
    assert len(matches)==1
    outcome=matches[0];p=Path(outcome['evidence_path']);assert str(p.resolve()) not in invalidated
    case=dict(kind='case',scenario_id=scenario,family_id=family,operation=operation,tier=1,input_mode='directory' if cdc else 'store',proof_only=True,inherited=False)
    destination=OUT/f'{operation}-validation.json'
    if destination.exists():raise ValueError('refuse to repeat scoped validation')
    value=r.validate_attempt(outcome,{},case,build);r.custody.write_json(destination,value)
    if value['issues'] or value['violations'] or value['product_status']!='pass' or not value['verification_pass']:raise ValueError(f"{scenario}: {value['issues']} {value['violations']}")
    rows.append(dict(case=scenario,family_id=family,seed=1,mode='verify',assurance_status='fully_verified',inherited=False,source_identity=r.source_identity(outcome),source_arm=outcome['source_arm'],raw_product_status=outcome['product_status'],coverage_status=outcome['coverage_status'],product_status='pass',evidence_status='PASS',issues=[],violations=[],evidence=str(p),metrics=value['metrics'],resource_observations=value['resource_observations'],observations=value['observations'],canonical_packages=value['canonical_packages'],verification_summary=r.verification_summary(value['observations'],value['canonical_packages']),environment_identity=outcome['environment_identity'],input_identity=outcome['input_identity'],invalidation_context=[],product_source_compatibility=None,product_predicate_scope=None,measured_current_product_binary=True,verification_source_compatibility='identical sealed e092 source',performance_claim_eligible=False,verification_pass=True))
    manifest_hashes[str(p)]=r.custody.sha(p/'evidence.sha256');validations[scenario]=dict(verification_pass=True,issues=[],violations=[],external_process_wall_ns=outcome['external_process_wall_ns'],supervisor_cleanup_status=outcome['supervisor_cleanup_status'])
    joins.append(dict(case=scenario,seed=1,evidence=str(p),source_identity=r.source_identity(outcome),input_identity=outcome['input_identity'],environment_identity=outcome['environment_identity'],performance_pair='not applicable: standalone targeted proof'))
assert len(rows)==1 and len({x['environment_identity'] for x in rows})==1
assert len({x['input_identity'] for x in rows if x['family_id']=='workspace_reliability'})==1
if r.custody.sha(SOURCE)!=SHA:raise ValueError('report changed during validation')
r.custody.write_json(OUT/'incremental-full-rows.json',dict(schema='phase1-incremental-full-verification-rows-v1',rows=rows,source_revision=REV,report_generator_sha256=SHA,attempt_manifest_sha256=manifest_hashes,prior_report_sha256=prior['sha256']))
r.custody.write_json(OUT/'checkpoint.json',dict(schema='phase1-hardlink-final-checkpoint-v1',status='PASS',recorded_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),new_full_proofs_validated=1,new_reliability_proofs=1,new_cdc_boundary_proofs=0,actual_source_revision=REV,report_generator_sha256=SHA,prior_report=prior,attempt_manifest_sha256=manifest_hashes,validations=validations,joins=joins,reused_without_revalidation='All prior performance and qualified full-proof verdicts',explicit_exclusions=['Failed hardlink-alias af050e843eb8 remains failed, no passing credit','Already qualified repaired shared-path-contention278b3a754568'],scope='One corrected e092 hardlink standalone full proof only. Combined with previously qualified28 targeted receipts, targeted coverage is29/29. No prior verdict revalidation or overall Phase1 terminal claim.',product_executions=0,full_report_reruns=0))
r.custody.seal(OUT)
print(json.dumps(dict(status='PASS',new_full_proofs=1,issues=0,violations=0,receipt=str(OUT/'incremental-full-rows.json'))))
