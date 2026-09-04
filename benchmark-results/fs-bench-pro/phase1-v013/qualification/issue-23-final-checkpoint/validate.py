"""Qualify only the newly completed proof; reuse the sealed report's payload rows."""
import datetime, hashlib, importlib.util, json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[5]
CAMPAIGN = ROOT / 'benchmark-results/fs-bench-pro/phase1-v013'
OUT = Path(__file__).resolve().parent
SOURCE = ROOT / 'benchmark/fs-bench-pro/generate-workspace-report.py'
spec = importlib.util.spec_from_file_location('issue23_report', SOURCE)
report = importlib.util.module_from_spec(spec); spec.loader.exec_module(report)
source_bytes = SOURCE.read_bytes()
report_path = CAMPAIGN / 'results/review.json'
report_bytes = report_path.read_bytes(); prior = json.loads(report_bytes)
report_sha = hashlib.sha256(report_bytes).hexdigest()
prior_seal = (CAMPAIGN / 'results/evidence.sha256').read_text()
if f'{report_sha}  review.json\n' not in prior_seal: raise ValueError('input report differs from its seal')
if prior['report_generator_sha256'] != hashlib.sha256(source_bytes).hexdigest(): raise ValueError('changed report validator requires separate qualification')
if prior['global_issues'] or prior['counts']['invalid_slots'] or prior['counts']['product_failed_outcomes']: raise ValueError('input report has unresolved validity/failure gates')
rows = [row for row in prior['rows'] if row['family_id'] == 'payload_create_read']
if len(rows) != 47: raise ValueError('expected24 qualified performance rows and23 existing full proofs')
for row in rows:
    if row['evidence_status'] != 'PASS' or row['product_status'] != 'pass' or row['issues'] or row['violations'] or row['invalidation_context']: raise ValueError('prior family row is not qualified')
    if row['mode'] == 'verify' and row['assurance_status'] != 'fully_verified': raise ValueError('existing proof is not exhaustive')

ledger = report.read(CAMPAIGN / 'slots.json')
new_rows = [row for row in ledger.values() if row.get('source_revision') == 'e32469e975e8e185ca525b02bb71d70bafa4e865' and row.get('scenario_id') == 'payload-create-1m' and row.get('seed') == 1 and row.get('mode') == 'verify']
if len(new_rows) != 1: raise ValueError('new exact proof slot missing/ambiguous')
new = new_rows[0]
case = dict(kind='case', scenario_id='payload-create-1m', family_id='payload_create_read', operation='payload-create', tier=1, input_mode='store', proof_only=False, inherited=False)
# Source definition for this exact descriptor is the frozen family module's1MiB create row.
family_path = ROOT / 'benchmark/fs-bench-pro/families/payload_create_read.rs'
result = report.validate_attempt(new, {}, case, prior['source'])
report.custody.write_json(OUT / 'new-proof-validation.json', result)
if result['issues'] or result['violations'] or result['product_status'] != 'pass' or not result['verification_pass']: raise ValueError('new full proof failed existing validate_attempt gates')

invocations = []
for path in (CAMPAIGN / 'invocations').glob('*.json'):
    value = report.read(path)
    if value.get('source_revision') == new['source_revision'] and value.get('image_id') == new['image_id'] and [new['scenario_id'],1,'verify'] in value.get('planned_slots',[]):
        if value.get('status') != 'pass': raise ValueError('new proof CLI invocation is not completed successfully')
        wall = report.number(value.get('invocation_wall_ns'),'invocation wall')
        if wall < report.number(value.get('source_validation_ns'),'source validation') + report.number(value.get('registry_query_ns'),'registry query'): raise ValueError('CLI wall hides required work')
        invocations.append(dict(path=str(path),sha256=report.custody.sha(path),receipt=value))
if len(invocations) != 1: raise ValueError('new proof lacks one exact completed CLI invocation')

new_row = dict(case=new['scenario_id'],family_id=new['family_id'],seed=1,mode='verify',assurance_status='fully_verified',source_identity={key:new[key] for key in report.IDENTITY_FIELDS},source_arm=new['source_arm'],environment_identity=new['environment_identity'],input_identity=new['input_identity'],evidence=new['evidence_path'],evidence_status='PASS',product_status='pass',issues=[],violations=[],invalidation_context=[],verification_summary=report.verification_summary(result['observations'],result['canonical_packages']))
rows.append(new_row)
ids = [f'payload-create-{n}m' for n in (1,10,100,500)] + [f'payload-random-read-{n}' for n in (1,10,100,500)]
expected = {(case,seed,mode) for case in ids for seed in (1,2,3) for mode in ('performance','verify')}
keyed = {(row['case'],row['seed'],row['mode']):row for row in rows}
if len(rows)!=48 or set(keyed)!=expected: raise ValueError('payload family exact8x3x2 coverage mismatch')
suppression = report.read(CAMPAIGN/'phase1-runtime-suppressions.json')
if set(ids)&set(suppression['cases']): raise ValueError('payload scope changed')
joins=[]; identities=[]
for case in ids:
    for seed in (1,2,3):
        timed, proof = keyed[(case,seed,'performance')],keyed[(case,seed,'verify')]
        if timed['input_identity'] != proof['input_identity'] or timed['environment_identity'] != proof['environment_identity'] or proof['environment_identity'] != new['environment_identity']: raise ValueError('payload input/VM8 environment join mismatch')
        same = timed['source_identity'] == proof['source_identity']
        bridge = next((item for item in prior['verification_compatibility'] if item['family_id']=='payload_create_read' and item['performance_revision']==timed['source_identity']['source_revision'] and item['verification_revision']==proof['source_identity']['source_revision']),None)
        if not same and bridge is None: raise ValueError('missing exact performance/full-proof source bridge')
        if (case,seed)==('payload-create-1m',1):
            if set(bridge['unchanged_paths']) != report.bridge_dependency_paths('payload_create_read'): raise ValueError('source bridge omitted oracle dependencies')
            for filename, expected_hash in bridge['unchanged_paths'].items(): report.validate_bridge_path(filename,expected_hash,[bridge['performance_revision'],bridge['verification_revision']])
        elif timed.get('performance_claim_eligible') is not True: raise ValueError('previously paired payload performance was not eligible')
        joins.append(dict(case=case,seed=seed,input_identity=timed['input_identity'],environment_identity=timed['environment_identity'],performance_evidence=timed['evidence'],full_verification_evidence=proof['evidence'],source_join='identical sealed source' if same else dict(family_id=bridge['family_id'],performance_revision=bridge['performance_revision'],verification_revision=bridge['verification_revision']),performance_claim_eligible=True))
for row in sorted(rows,key=lambda item:(item['case'],item['seed'],item['mode'])):
    identities.append({key:row[key] for key in ('case','seed','mode','source_identity','environment_identity','input_identity','evidence','evidence_status','product_status')} | {'assurance_status':row.get('assurance_status'),'attempt_manifest_sha256':report.custody.sha(Path(row['evidence'])/'evidence.sha256')})
receipt=dict(schema='phase1-family-completion-checkpoint-v1',recorded_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),issue=23,family_id='payload_create_read',family_status='PASS',counts=dict(cases=8,performance=24,full_verification=24,suppressed=0,missing=0,invalid=0,failures=0),environment_scope='VM8 only; no VM4 payload proof credit; original sources preserved',input_report=dict(path=str(report_path),sha256=report_sha,seal_sha256=hashlib.sha256(prior_seal.encode()).hexdigest(),source_revision=prior['source']['revision'],original_missing_global_slots=prior['counts']['missing_slots']),validator_sha256=hashlib.sha256(source_bytes).hexdigest(),family_definition_sha256=report.custody.sha(family_path),new_attempt=dict(path=new['evidence_path'],manifest_sha256=report.custody.sha(Path(new['evidence_path'])/'evidence.sha256'),validation='existing report.validate_attempt; only this new proof revalidated'),reused_qualified_rows=47,original_report_rewritten=False,whole_report_reruns=0,product_executions=0,new_invocation=invocations[0],joins=joins,identities=identities,limitations='Issue23 family checkpoint only; not PHASE1_TERMINAL_PASS, not release admission, and no claim that other family proofs are complete. Existing failed/historical outcomes remain in the original report inventory.')
report.custody.write_json(OUT/'family-completion.json',receipt)
report.custody.seal(OUT)
print(json.dumps({'family_status':'PASS','cases':8,'performance':24,'full_verification':24,'new_proofs_validated':1,'reused_qualified_rows':47,'issues':result['issues'],'violations':result['violations'],'receipt':str(OUT/'family-completion.json')}))
