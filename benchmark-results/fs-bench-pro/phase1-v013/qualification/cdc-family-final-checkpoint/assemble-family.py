"""Combine qualified CDC verdicts and exact source/input joins; no validator calls."""
import hashlib,importlib.util,json,statistics
from pathlib import Path
OUT=Path(__file__).resolve().parent;ROOT=OUT.parents[4];C=ROOT/'benchmark-results/fs-bench-pro/phase1-v013';SOURCE=ROOT/'benchmark/fs-bench-pro/generate-workspace-report.py';REV='e24a3b34b943e1f0a7f5ccf7fadf80217b6f1fb0';SHA='329fa5da5cf54d959cb5298ee0683fc95714be9bcf0a6cb9159d05885c21b932'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert sha(SOURCE)==SHA
spec=importlib.util.spec_from_file_location('cdc_family_assembly_report',SOURCE);r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)
new=r.read(OUT/'pending-fast-rows.json');assert len(new['rows'])==36
q=lambda name,file:r.runner.qualified_json(C/'qualification'/name/file)
oldfast=q('e092-before-collision-fast-checkpoint','incremental-fast-rows.json');oldfast=[x for x in oldfast['rows'] if x['family_id']=='dedup_cdc_locality'];assert len(oldfast)==22
correctedfast=q('cdc-delete-collision-final-checkpoint','incremental-fast-rows.json')['rows'];correctedperf=q('cdc-delete-collision-final-checkpoint','incremental-performance-rows.json')['rows'];assert len(correctedfast)==len(correctedperf)==1
first=q('fast-v2-first-dedup','receipt.json');assert first['fast_iteration_pass'] and not first['fully_verified'];firstvalidation=q('fast-v2-first-dedup','validation.json');assert firstvalidation['fast_iteration_pass'] and not firstvalidation['issues'] and not firstvalidation['violations']
firstout=r.read(Path(first['evidence'])/'outcome.json');firstrow=dict(case='dedup-cdc-common-body-1',seed=1,evidence=first['evidence'],source_identity=r.source_identity(firstout),environment_identity=first['environment_identity'],input_identity=first['input_identity'],assurance_status='fast_iteration_verified',fast_iteration_pass=True,verification_pass=False,issues=[],violations=[])
fast=oldfast+correctedfast+[firstrow]+new['rows'];assert len(fast)==60
prior=r.read(OUT/'checkpoint.json')['prior_report'];reportpath=Path(prior['path']);assert sha(reportpath)==prior['sha256'];previous=r.read(reportpath)
performance=[x for x in previous['rows'] if x['family_id']=='dedup_cdc_locality' and x['mode']=='performance' and (x['case'],x['seed'])!=('dedup-cdc-delete-500',3)];assert len(performance)==59;performance+=correctedperf
boundary=[x for x in q('30d-additional-targeted-checkpoint','incremental-full-rows.json')['rows'] if x['case']=='dedup-cdc-boundaries-proof'];assert len(boundary)==1 and boundary[0]['verification_pass'] and boundary[0]['assurance_status']=='fully_verified'
config=r.read(C/'evidence-builds.json');binding_path=C/'qualification/fast-source-binding/1788538675483490000/receipt.json';binding=r.runner.qualified_json(binding_path);assert binding['revision']==REV and binding['mapping_sha256']==sha(C/'evidence-builds.json')
def selected(case,seed,mode):
    for key in [f'slot:{case}:{seed}:{mode}',f'case:{case}:{mode}',f'family:dedup_cdc_locality:{mode}','family:dedup_cdc_locality','default']:
        if key in config['selections']:return config['selections'][key]['assets']
    return 'assets-e24a3b34'
fs={(x['case'],x['seed']):x for x in fast};ps={(x['case'],x['seed']):x for x in performance};assert len(fs)==len(ps)==60 and set(fs)==set(ps)
bridges=[]
for bridge in config['verification_compatibility']:
    if bridge['family_id']!='dedup_cdc_locality':continue
    assert set(bridge['unchanged_paths'])==r.bridge_dependency_paths('dedup_cdc_locality')
    for filename,expected in bridge['unchanged_paths'].items():r.validate_bridge_path(filename,expected,[bridge['performance_revision'],bridge['verification_revision']])
    bridges.append(bridge)
joins=[]
for key,proof in sorted(fs.items()):
    perf=ps[key];assert proof['fast_iteration_pass'] and not proof['verification_pass'] and not proof['issues'] and not proof['violations'];assert perf['product_status']=='pass' and perf['evidence_status']=='PASS' and not perf['issues'] and not perf['violations']
    assert proof['input_identity']==perf['input_identity'] and proof['environment_identity']==perf['environment_identity']
    pr=perf['source_identity']['source_revision'];vr=proof['source_identity']['source_revision'];assert selected(*key,'performance')=='assets-'+pr[:8];assert selected(*key,'fast-verify')=='assets-'+vr[:8]
    if pr!=vr:assert any(x['performance_revision']==pr and x['verification_revision']==vr for x in bridges)
    if key==('dedup-cdc-delete-500',3):assert pr==vr==REV and proof['input_identity']=='29a94581378e59218e01df37b5705f1ce5b601c307af0f9e37b6781836b0c682'
    joins.append(dict(case=key[0],seed=key[1],performance_evidence=perf['evidence'],performance_source=perf['source_identity'],fast_evidence=proof['evidence'],fast_source=proof['source_identity'],input_identity=proof['input_identity'],environment_identity=proof['environment_identity'],assurance_status='fast_iteration_verified',fully_verified=False))
assert selected('dedup-cdc-boundaries-proof',1,'verify')=='assets-30d13dee'
medians=[]
for case in sorted({x['case'] for x in performance}):
    group=[x for x in performance if x['case']==case]
    for source in sorted({x['source_identity']['source_revision'] for x in group}):
        current=[x for x in group if x['source_identity']['source_revision']==source];values=[x['metrics']['initialize_ns'] for x in current]
        medians.append(dict(case=case,source_revision=source,seeds=sorted(x['seed'] for x in current),n=len(current),initialize_ns=dict(median=statistics.median(values),min=min(values),max=max(values))))
r.custody.write_json(OUT/'performance-medians.json',dict(schema='phase1-source-separated-case-medians-v1',rows=medians,scope='Public initialize call nanoseconds; source groups kept separate, including corrected deletion500 seed3.'))
receipt=dict(schema='phase1-qualified-family-completion-v1',status='PASS',issue=31,family_id='dedup_cdc_locality',performance_passes=60,accepted_routine_fast_passes=60,full_routine_verification_passes=0,required_boundary_full_proofs=1,joins=joins,boundary=boundary[0],source_binding_receipt=str(binding_path),source_binding_receipt_sha256=sha(binding_path),source_map_sha256=binding['mapping_sha256'],recipe_bridges=bridges,prior_report=prior,new_fast_validations=36,reused_fast_verdicts=24,reused_performance_verdicts=60,reused_boundary_verdicts=1,corrected_input=q('cdc-delete-collision-final-checkpoint','input-impact-and-pair.json'),scope='Family31 complete under routine fast acceptance plus mandatory full CDC boundary proof. Fast is not exhaustive; original failed collision and old recipe-invalid performance remain preserved. No globalPhase1 terminal claim.')
r.custody.write_json(OUT/'family-completion.json',receipt)
checkpoint=r.read(OUT/'checkpoint.json');checkpoint.update(family_completion_status='PASS',family_performance_passes=60,family_fast_passes=60,boundary_full_proofs=1);r.custody.write_json(OUT/'checkpoint.json',checkpoint)
(OUT/'pending-fast-rows.json').rename(OUT/'incremental-fast-rows.json');assert sha(SOURCE)==SHA;r.custody.seal(OUT)
print(json.dumps(dict(status='PASS',performance=60,fast=60,boundary=1,new_validations=36,receipt=str(OUT/'family-completion.json'))))
