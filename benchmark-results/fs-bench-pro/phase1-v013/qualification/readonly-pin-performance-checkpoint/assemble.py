"""Validate corrected readonly-Pin timing replacements once; do not reopen prior samples."""
import datetime, hashlib, importlib.util, json, statistics
from pathlib import Path
OUT=Path(__file__).resolve().parent;ROOT=OUT.parents[4];C=ROOT/'benchmark-results/fs-bench-pro/phase1-v013'
SOURCE=ROOT/'benchmark/fs-bench-pro/generate-workspace-report.py';SHA='dbfabc637656e558e286a3f1fa276eb5bf96d3c16b4b7eeb4f401a101d9f47a3';REV='30d13deeec72b46ff7bc411f1ec08a46990541e1'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
if sha(SOURCE)!=SHA:raise ValueError('report source is not the frozen validator')
spec=importlib.util.spec_from_file_location('pin_performance_report',SOURCE);r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)
invpath=ROOT/'target/phase1-audit/readonly-pin-performance-impact.json';inv=r.read(invpath);ledger=r.read(C/'slots.json');config=r.read(C/'evidence-builds.json');build=r.read(C/'assets-30d13dee/evidence/build.json')
if build['revision']!=REV or build['status']!='pass':raise ValueError('corrected build mismatch')
r.custody.verify_manifest(C/'assets-30d13dee/evidence')
prior=r.read(C/'qualification/78-additional-reliability-checkpoint/checkpoint.json')['prior_report']
invalidated={str(Path(x['previous_evidence']).resolve()) for x in [r.decode(line) for line in (C/'invalidations.jsonl').read_text().splitlines() if line]}
new=[x for x in ledger.values() if x.get('source_revision')==REV and x.get('mode')=='performance' and x['scenario_id'] in inv['affected_case_ids']]
successes=[x for x in new if x['product_status']=='pass'];failures=[x for x in new if x['product_status']!='pass']
assert len(successes)==42 and len(failures)==1
suppressed_case='directory-content-scan-100';assert {x['scenario_id'] for x in successes}==set(inv['affected_case_ids'])-{suppressed_case}
def caseof(x):
 case=x['scenario_id'];prefix,tier=case.rsplit('-',1)
 return dict(kind='case',scenario_id=case,family_id=x['family_id'],operation='metadata' if prefix=='dedup-history-metadata' else prefix,tier=int(tier),input_mode='store',proof_only=False,inherited=False)
rows=[];manifest_hashes={}
for outcome in sorted(successes,key=lambda x:(x['scenario_id'],x['seed'])):
    case=caseof(outcome);path=Path(outcome['evidence_path']);destination=OUT/f"{outcome['scenario_id']}-s{outcome['seed']}-validation.json"
    if not destination.exists():raise ValueError('missing prior once-only validation')
    if str(path.resolve()) in invalidated:raise ValueError('new timing invalidated')
    selection=config['selections'].get(f"slot:{outcome['scenario_id']}:{outcome['seed']}:performance")
    if selection and selection['assets']!='assets-30d13dee':raise ValueError('map selects another timing source')
    value=r.read(destination) # Reuse all42 already completed verdicts; do not invoke the validator again.
    if value['issues'] or value['violations'] or value['product_status']!='pass':raise ValueError(f"{case['scenario_id']}: {value['issues']} {value['violations']}")
    rows.append(dict(case=case['scenario_id'],family_id=case['family_id'],seed=outcome['seed'],mode='performance',inherited=False,assurance_status='not_verified',source_identity=r.source_identity(outcome),source_arm=outcome['source_arm'],raw_product_status=outcome['product_status'],coverage_status=outcome['coverage_status'],product_status='pass',evidence_status='PASS',issues=[],violations=[],evidence=str(path),metrics=value['metrics'],resource_observations=value['resource_observations'],observations=value['observations'],canonical_packages=[],verification_summary=None,environment_identity=outcome['environment_identity'],input_identity=outcome['input_identity'],invalidation_context=[],product_source_compatibility=None,product_predicate_scope=None,measured_current_product_binary=True,performance_claim_eligible=True,verification_pass=False))
    manifest_hashes[str(path)]=r.custody.sha(path/'evidence.sha256')
# Suppression uses its own existing policy and mandatory resource checks, never fabricated failure reproduction.
x=failures[0];assert x['scenario_id']==suppressed_case and x['seed']==1 and x['evidence_path'].endswith('3d28647a3175')
p=Path(x['evidence_path']);r.custody.verify_manifest(p);sealed=r.read(p/'outcome.json');assert all(x.get(k)==v for k,v in sealed.items())
suppressions=r.read(C/'phase1-runtime-suppressions.json');entry=suppressions['cases'][suppressed_case];records=r.raw(p/'raw.jsonl');events=[z for z in records if z['kind']=='product-time-budget-exceeded'];assert len(events)==1
observation=r.runner.product_budget_observation(events[0]);assert observation==entry['observed_product_ns']==15_000_758_416
assert entry['limit_ns']==15_000_000_000 and entry['source_revision']==REV and entry['evidence_path']==str(p)
assert x['product_status']=='fail' and x['phase1_status']=='suppressed_phase1_time_budget' and x['exit_code']==124 and x['coverage_status']=='executed'
assert r.runner.budget_suppression_can_continue(x)
assert not any(z['kind']=='product-budget-observation-error' or z['kind']=='product-budget-phase' and z.get('phase_error') is not None for z in records)
issues=[];violations=[];resources=r.validate_resources(p,x,caseof(x),records,False,issues,violations)
assert not issues and not violations,(issues,violations)
assert not any(z['scenario_id']==suppressed_case and z['seed'] in [2,3] for z in new)
manifest_hashes[str(p)]=r.custody.sha(p/'evidence.sha256')
suppression=dict(schema='phase1-qualified-time-budget-suppression-v1',status='suppressed_phase1_time_budget',raw_product_status='fail',raw_coverage_status='executed',case=suppressed_case,seed=1,source_revision=REV,evidence=str(p),entry=entry,event=events[0],sound_continuation=True,issues=issues,violations=violations,resources=resources,not_run_seeds=[2,3],retained_old_timing_disposition='Old7948 timings for this exact case remain historical, no active coverage credit.')
r.custody.write_json(OUT/'suppression-validation.json',suppression)
# Per-case three-seed medians, grouped into four families; never pool unlike tiers or source revisions.
families={}
for case in sorted({x['case'] for x in rows}):
    group=[x for x in rows if x['case']==case];assert sorted(x['seed'] for x in group)==[1,2,3]
    metrics={k:dict(n=3,median=statistics.median(x['metrics'][k] for x in group),min=min(x['metrics'][k] for x in group),max=max(x['metrics'][k] for x in group)) for k in ['pure_call_sum_ns','exec_ns','commit_ns','create_ns','end_ns','workload_ns','inner_workload_ns'] if all(k in x['metrics'] for x in group)}
    families.setdefault(group[0]['family_id'],[]).append(dict(case=case,seeds=[1,2,3],metrics=metrics,evidence=[x['evidence'] for x in group]))
assert len(families)==4
if sha(SOURCE)!=SHA:raise ValueError('report changed during scoped validation')
r.custody.write_json(OUT/'family-medians.json',dict(schema='phase1-case-medians-by-family-v1',source_revision=REV,units='nanoseconds',scope='Three seeds per exact case; no pooling across tiers or old sources; performance only, correctness checked separately.',families=families))
r.custody.write_json(OUT/'incremental-performance-rows.json',dict(schema='phase1-incremental-performance-rows-v1',rows=rows,source_revision=REV,report_generator_sha256=SHA,attempt_manifest_sha256={k:v for k,v in manifest_hashes.items() if k!=str(p)},prior_report_sha256=prior['sha256']))
r.custody.write_json(OUT/'checkpoint.json',dict(schema='phase1-readonly-pin-performance-checkpoint-v1',status='PASS_WITH_EXPLICIT_SUPPRESSION',recorded_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),new_performance_passes=42,failed_budget_triggers=1,suppressed_unexecuted_slots=2,active_performance_total=370,prior_performance_retained_without_revalidation=328,suppressed_case_ids=len(suppressions['cases']),source_revision=REV,report_generator_sha256=SHA,prior_report=prior,impact_inventory_sha256=sha(invpath),source_map_sha256=sha(C/'evidence-builds.json'),attempt_manifest_sha256=manifest_hashes,issues=[],violations=[],scope='42 corrected Pin timing passes validated once; one sound budget failure preserved as failed/suppressed and two never-run slots. No prior timing/full-proof validator reruns or overall terminal claim.',product_executions=0,full_report_reruns=0))
r.custody.seal(OUT)
print(json.dumps(dict(status='PASS_WITH_EXPLICIT_SUPPRESSION',passes=42,failed_budget_triggers=1,not_run=2,issues=0,violations=0,receipt=str(OUT/'incremental-performance-rows.json'),medians=str(OUT/'family-medians.json'))))
