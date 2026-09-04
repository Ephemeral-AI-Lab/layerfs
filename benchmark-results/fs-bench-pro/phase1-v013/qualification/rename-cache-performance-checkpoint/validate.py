"""Validate33 corrected Rename timing replacements once; retain337 prior timings without reading them."""
import datetime, hashlib, importlib.util, json, statistics
from pathlib import Path
OUT=Path(__file__).resolve().parent;ROOT=OUT.parents[4];C=ROOT/'benchmark-results/fs-bench-pro/phase1-v013'
SOURCE=ROOT/'benchmark/fs-bench-pro/generate-workspace-report.py';SHA='1289ff9d8089195c78f9a61bdc19f50eedf374234cd2769acb573ae2e01b53d8';REV='e0922904e2bb607a138157755dab9613b441d5b9'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
if sha(SOURCE)!=SHA:raise ValueError('report source is not the frozen validator')
spec=importlib.util.spec_from_file_location('rename_performance_report',SOURCE);r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)
planpath=C/'qualification/rename-cache-performance-e0922904/plan.json';plan=r.read(planpath);expected={(x['case'],x['seed']) for x in plan['commands']};assert len(expected)==33 and plan['runtime_revision']==REV;ledger=r.read(C/'slots.json');config=r.read(C/'evidence-builds.json');build=r.read(C/'assets-e0922904/evidence/build.json')
if build['revision']!=REV or build['status']!='pass':raise ValueError('corrected build mismatch')
r.custody.verify_manifest(C/'assets-e0922904/evidence')
prior=r.read(C/'qualification/78-additional-reliability-checkpoint/checkpoint.json')['prior_report']
invalidated={str(Path(x['previous_evidence']).resolve()) for x in [r.decode(line) for line in (C/'invalidations.jsonl').read_text().splitlines() if line]}
new=[x for x in ledger.values() if x.get('source_revision')==REV and x.get('mode')=='performance' and (x['scenario_id'],x['seed']) in expected]
successes=[x for x in new if x['product_status']=='pass'];failures=[x for x in new if x['product_status']!='pass']
assert len(successes)==33 and not failures and {(x["scenario_id"],x["seed"]) for x in successes}==expected
def caseof(x):
 case=x['scenario_id'];prefix,tier=case.rsplit('-',1)
 return dict(kind='case',scenario_id=case,family_id=x['family_id'],operation='metadata' if prefix=='dedup-history-metadata' else prefix,tier=int(tier),input_mode='store',proof_only=False,inherited=False)
rows=[];manifest_hashes={}
for outcome in sorted(successes,key=lambda x:(x['scenario_id'],x['seed'])):
    case=caseof(outcome);path=Path(outcome['evidence_path']);destination=OUT/f"{outcome['scenario_id']}-s{outcome['seed']}-validation.json"
    if destination.exists():raise ValueError('refuse repeat validation')
    if str(path.resolve()) in invalidated:raise ValueError('new timing invalidated')
    selection=config['selections'].get(f"slot:{outcome['scenario_id']}:{outcome['seed']}:performance")
    if selection and selection['assets']!='assets-e0922904':raise ValueError('map selects another timing source')
    value=r.validate_attempt(outcome,{},case,build);r.custody.write_json(destination,value)
    if value['issues'] or value['violations'] or value['product_status']!='pass':raise ValueError(f"{case['scenario_id']}: {value['issues']} {value['violations']}")
    rows.append(dict(case=case['scenario_id'],family_id=case['family_id'],seed=outcome['seed'],mode='performance',inherited=False,assurance_status='not_verified',source_identity=r.source_identity(outcome),source_arm=outcome['source_arm'],raw_product_status=outcome['product_status'],coverage_status=outcome['coverage_status'],product_status='pass',evidence_status='PASS',issues=[],violations=[],evidence=str(path),metrics=value['metrics'],resource_observations=value['resource_observations'],observations=value['observations'],canonical_packages=[],verification_summary=None,environment_identity=outcome['environment_identity'],input_identity=outcome['input_identity'],invalidation_context=[],product_source_compatibility=None,product_predicate_scope=None,measured_current_product_binary=True,performance_claim_eligible=True,verification_pass=False))
    manifest_hashes[str(path)]=r.custody.sha(path/'evidence.sha256')
families={}
for case in sorted({x['case'] for x in rows}):
    group=[x for x in rows if x['case']==case];assert sorted(x['seed'] for x in group)==[1,2,3]
    metrics={k:dict(n=3,median=statistics.median(x['metrics'][k] for x in group),min=min(x['metrics'][k] for x in group),max=max(x['metrics'][k] for x in group)) for k in ['pure_call_sum_ns','exec_ns','commit_ns','create_ns','end_ns','workload_ns','preparation_ns','command_wall_ns','external_process_wall_ns','store.file_bytes.delta','store.allocated_bytes.delta'] if all(k in x['metrics'] for x in group)}
    families.setdefault(group[0]['family_id'],[]).append(dict(case=case,seeds=[1,2,3],metrics=metrics,evidence=[x['evidence'] for x in group]))
assert len(rows)==33 and len(families)==3
if sha(SOURCE)!=SHA:raise ValueError('report changed during scoped validation')
r.custody.write_json(OUT/'family-medians.json',dict(schema='phase1-case-medians-by-family-v1',source_revision=REV,units='time ns; Store deltas bytes',scope='Three seeds per exact case; no pooling across tiers or source revisions; performance only, correctness checked separately.',families=families))
r.custody.write_json(OUT/'incremental-performance-rows.json',dict(schema='phase1-incremental-performance-rows-v1',rows=rows,source_revision=REV,report_generator_sha256=SHA,attempt_manifest_sha256=manifest_hashes,prior_report_sha256=prior['sha256']))
r.custody.write_json(OUT/'checkpoint.json',dict(schema='phase1-rename-cache-performance-checkpoint-v1',status='PASS',recorded_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),new_performance_passes=33,family_counts={'namespace_mutation':9,'workspace_change_locality':12,'mixed_load_bearing':12},active_performance_total=370,prior_performance_retained_without_revalidation=337,new_suppressions=0,source_revision=REV,report_generator_sha256=SHA,prior_report=prior,selected_plan_sha256=sha(planpath),source_map_sha256=sha(C/'evidence-builds.json'),attempt_manifest_sha256=manifest_hashes,issues=[],violations=[],scope='33 corrected Rename timing passes validated once.337 prior timings retain their actual producing identities and values, without rereading or validating them. No overall Phase1 terminal claim.',product_executions=0,full_report_reruns=0))
r.custody.seal(OUT)
print(json.dumps(dict(status='PASS',passes=33,issues=0,violations=0,receipt=str(OUT/'incremental-performance-rows.json'),medians=str(OUT/'family-medians.json'))))
