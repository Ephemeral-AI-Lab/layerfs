"""Cross-file family joins reuse every prior verdict; no validators are invoked."""
import hashlib,importlib.util,json,statistics
from pathlib import Path
OUT=Path(__file__).resolve().parent;ROOT=OUT.parents[4];C=ROOT/'benchmark-results/fs-bench-pro/phase1-v013';SOURCE=ROOT/'benchmark/fs-bench-pro/generate-workspace-report.py';REV='e24a3b34b943e1f0a7f5ccf7fadf80217b6f1fb0';SHA='329fa5da5cf54d959cb5298ee0683fc95714be9bcf0a6cb9159d05885c21b932';FAMILY='dedup_cross_file'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest();assert sha(SOURCE)==SHA
spec=importlib.util.spec_from_file_location('cross_family_assembly_report',SOURCE);r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)
new=r.read(OUT/'pending-fast-rows.json');assert len(new['rows'])==4
priorfastpath=C/'qualification/e092-before-collision-fast-checkpoint/incremental-fast-rows.json';priorfast=r.runner.qualified_json(priorfastpath);oldfast=[x for x in priorfast['rows'] if x['family_id']==FAMILY];assert len(oldfast)==26
fast=oldfast+new['rows'];prior=r.read(OUT/'checkpoint.json')['prior_report'];reportpath=Path(prior['path']);assert sha(reportpath)==prior['sha256'];reportdata=r.read(reportpath);performance=[x for x in reportdata['rows'] if x['family_id']==FAMILY and x['mode']=='performance'];assert len(performance)==30
config=r.read(C/'evidence-builds.json');binding_path=C/'qualification/fast-source-binding/1788538675483490000/receipt.json';binding=r.runner.qualified_json(binding_path);assert binding['revision']==REV and binding['mapping_sha256']==sha(C/'evidence-builds.json')
def selected(case,seed,mode):
    for key in [f'slot:{case}:{seed}:{mode}',f'case:{case}:{mode}',f'family:{FAMILY}:{mode}',f'family:{FAMILY}','default']:
        if key in config['selections']:return config['selections'][key]['assets']
    return 'assets-e24a3b34'
bridges=[x for x in config['verification_compatibility'] if x['family_id']==FAMILY]
for bridge in bridges:
    assert set(bridge['unchanged_paths'])==r.bridge_dependency_paths(FAMILY)
    for filename,expected in bridge['unchanged_paths'].items():r.validate_bridge_path(filename,expected,[bridge['performance_revision'],bridge['verification_revision']])
fs={(x['case'],x['seed']):x for x in fast};ps={(x['case'],x['seed']):x for x in performance};assert len(fs)==len(ps)==30 and set(fs)==set(ps)
joins=[]
for key,proof in sorted(fs.items()):
    perf=ps[key];assert proof['fast_iteration_pass'] and not proof['verification_pass'] and not proof['issues'] and not proof['violations'];assert perf['product_status']=='pass' and perf['evidence_status']=='PASS' and not perf['issues'] and not perf['violations']
    assert proof['input_identity']==perf['input_identity'] and proof['environment_identity']==perf['environment_identity'];pr=perf['source_identity']['source_revision'];vr=proof['source_identity']['source_revision'];assert selected(*key,'performance')=='assets-'+pr[:8] and selected(*key,'fast-verify')=='assets-'+vr[:8]
    if pr!=vr:assert any(x['performance_revision']==pr and x['verification_revision']==vr for x in bridges)
    joins.append(dict(case=key[0],seed=key[1],performance_evidence=perf['evidence'],performance_source=perf['source_identity'],fast_evidence=proof['evidence'],fast_source=proof['source_identity'],input_identity=proof['input_identity'],environment_identity=proof['environment_identity'],assurance_status='fast_iteration_verified',fully_verified=False))
medians=[]
for case in sorted({x['case'] for x in performance}):
    group=[x for x in performance if x['case']==case];assert sorted(x['seed'] for x in group)==[1,2,3] and len({x['source_identity']['source_revision'] for x in group})==1;values=[x['metrics']['initialize_ns'] for x in group]
    medians.append(dict(case=case,source_revision=group[0]['source_identity']['source_revision'],seeds=[1,2,3],n=3,initialize_ns=dict(median=statistics.median(values),min=min(values),max=max(values))))
notrun_path=C/'attempts/dedup-cross-file-unique-100-s3-fast-verify-bcde3546377c/outcome.json';notrun=r.read(notrun_path);assert notrun['product_status']=='not-run'
r.custody.write_json(OUT/'performance-medians.json',dict(schema='phase1-source-separated-case-medians-v1',rows=medians,scope='Public initialization nanoseconds, three qualified original-source seeds per exact case; no new timing claim.'))
r.custody.write_json(OUT/'family-completion.json',dict(schema='phase1-qualified-family-completion-v1',status='PASS',issue=30,family_id=FAMILY,performance_passes=30,accepted_routine_fast_passes=30,full_routine_verification_passes=0,suppressed_cases=[],joins=joins,source_binding_receipt=str(binding_path),source_binding_receipt_sha256=sha(binding_path),source_map_sha256=binding['mapping_sha256'],recipe_bridges=bridges,prior_report=prior,prior_fast_receipt_sha256=sha(priorfastpath),new_fast_validations=4,reused_fast_verdicts=26,reused_performance_verdicts=30,preserved_unexecuted_attempt=dict(path=str(notrun_path),raw_status='not-run',disposition='Original cache-capacity/pre-product attempt retained, not relabeled as successful product execution.'),scope='Family30 routine fast acceptance complete with exact input/environment/source joins; fast is not exhaustive and no full census claim. Original unexecuted cache-capacity attempt preserved. No globalPhase1 terminal claim.'))
checkpoint=r.read(OUT/'checkpoint.json');checkpoint.update(family_completion_status='PASS',family_performance_passes=30,family_fast_passes=30);r.custody.write_json(OUT/'checkpoint.json',checkpoint)
(OUT/'pending-fast-rows.json').rename(OUT/'incremental-fast-rows.json');assert sha(SOURCE)==SHA;r.custody.seal(OUT)
print(json.dumps(dict(status='PASS',performance=30,fast=30,new_validations=4,receipt=str(OUT/'family-completion.json'))))
