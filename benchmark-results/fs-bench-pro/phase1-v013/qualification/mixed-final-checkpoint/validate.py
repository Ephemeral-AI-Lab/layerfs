"""One new e092 mixed fast result; reuse eleven prior qualified fast results."""
import datetime,hashlib,importlib.util,json
from pathlib import Path
OUT=Path(__file__).resolve().parent;ROOT=OUT.parents[4];C=ROOT/'benchmark-results/fs-bench-pro/phase1-v013';SOURCE=ROOT/'benchmark/fs-bench-pro/generate-workspace-report.py'
SHA='1289ff9d8089195c78f9a61bdc19f50eedf374234cd2769acb573ae2e01b53d8';REV='e0922904e2bb607a138157755dab9613b441d5b9'
if hashlib.sha256(SOURCE.read_bytes()).hexdigest()!=SHA:raise ValueError('validator differs from frozen source')
spec=importlib.util.spec_from_file_location('additional30d_fast_report',SOURCE);r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)
prior=r.read(C/'qualification/78-additional-reliability-checkpoint/checkpoint.json')['prior_report'];build=r.read(C/'assets-e0922904/evidence/build.json');ledger=r.read(C/'slots.json');assert build['revision']==REV and build['status']=='pass'
invalidated={str(Path(x['previous_evidence']).resolve()) for x in [r.decode(line) for line in (C/'invalidations.jsonl').read_text().splitlines() if line]}
selected=[(500,3,'b8176d2d9bf7')]
rows=[];seals={};coverage=[]
for tier,seed,suffix in selected:
    scenario=f'agent-episodes-{tier}';matches=[x for x in ledger.values() if x.get('source_revision')==REV and x.get('scenario_id')==scenario and x.get('seed')==seed and x.get('mode')=='fast-verify' and x['evidence_path'].endswith(suffix)];assert len(matches)==1
    outcome=matches[0];p=Path(outcome['evidence_path']);assert str(p.resolve()) not in invalidated
    case=dict(kind='case',scenario_id=scenario,family_id='mixed_load_bearing',operation='agent-episodes',tier=tier,input_mode='store',proof_only=False,inherited=False)
    destination=OUT/f'{scenario}-s{seed}-validation.json'
    if destination.exists():raise ValueError('refuse repeated scoped validation')
    value=r.validate_fast_attempt(outcome,{},case,build);r.custody.write_json(destination,value)
    if value['issues'] or value['violations'] or value['product_status']!='pass' or not value['fast_iteration_pass'] or value['verification_pass']:raise ValueError(f"{scenario}: {value['issues']} {value['violations']}")
    receipts=r.read(p/'verification/fast-verification/receipts.json');canonical=json.loads(next(x['receipt'] for x in receipts if x['kind']=='fast-canonical-verification'));native=dict(line.split('=',1) for line in next(x['receipt'] for x in receipts if x['kind']=='fast-native-verification').splitlines() if line)
    assert canonical['fully_verified']==native['fully_verified']=='false'
    cov=dict(case=scenario,seed=seed,evidence=str(p),reference_assurance=canonical['reference_assurance'],canonical=canonical,native=native)
    coverage.append(cov)
    rows.append(dict(case=scenario,family_id='mixed_load_bearing',seed=seed,mode='fast-verify',required_mode='verify',phase1_verification_accepted=True,assurance_status='fast_iteration_verified',inherited=False,source_identity=r.source_identity(outcome),source_arm=outcome['source_arm'],raw_product_status=outcome['product_status'],coverage_status=outcome['coverage_status'],product_status='pass',evidence_status='PASS',issues=[],violations=[],evidence=str(p),metrics=value['metrics'],resource_observations=value['resource_observations'],observations=value['observations'],canonical_packages=value['canonical_packages'],verification_summary=r.verification_summary(value['observations'],value['canonical_packages']),environment_identity=outcome['environment_identity'],input_identity=outcome['input_identity'],invalidation_context=[],product_source_compatibility=None,product_predicate_scope=None,measured_current_product_binary=True,performance_claim_eligible=False,verification_pass=False,fast_iteration_pass=True,counts_toward_full_phase1_gate=False,counts_toward_routine_phase1_acceptance=True,fast_coverage=cov))
    seals[str(p)]=r.custody.sha(p/'evidence.sha256')
assert len(rows)==1 and len({x['environment_identity'] for x in rows})==1
if r.custody.sha(SOURCE)!=SHA:raise ValueError('report changed during validation')
r.custody.write_json(OUT/'incremental-fast-rows.json',dict(schema='phase1-incremental-fast-verification-rows-v1',rows=rows,source_revision=REV,report_generator_sha256=SHA,attempt_manifest_sha256=seals,prior_report_sha256=prior['sha256']))
r.custody.write_json(OUT/'coverage-and-omissions.json',dict(schema='phase1-fast-coverage-and-omissions-v1',assurance_status='fast_iteration_verified',fully_verified=False,rows=coverage,scope='Full current namespace/global inode/metadata/alias authentication, complete native namespace membership/types, independently selected changed/uncertified/witness bytes and native metadata. Explicit certified unchanged bodies and untouched native metadata omitted as recorded; full canonical census is not performed.'))
r.custody.write_json(OUT/'checkpoint.json',dict(schema='phase1-additional-fast-checkpoint-v1',status='PASS',assurance_status='fast_iteration_verified',new_fast_proofs_validated=1,new_full_proofs_validated=0,recorded_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),actual_source_revision=REV,report_generator_sha256=SHA,prior_report=prior,attempt_manifest_sha256=seals,issues=[],violations=[],explicit_exclusions=['First agent-episodes-1 seed1 fast59aad892ea34 already qualified; not rerun','All prior full proofs/performance','Any failed hardlink or contention evidence'],scope='Routine Phase1 fast acceptance only; no exhaustive verification credit and no successor product timing equivalence. Earlier fast proofs retain their original30d source through the exact reviewed rename semantic bridge.',product_executions=0,full_report_reruns=0))
# Family completion assembled below from sealed prior verdicts, without revalidation.
prior_fast_dir=C/'qualification/30d-additional-fast-checkpoint';prior_perf_dir=C/'qualification/rename-cache-performance-checkpoint';first_dir=C/'qualification/fast-v2-first-selected'
for directory in [prior_fast_dir,prior_perf_dir,first_dir]:r.custody.verify_manifest(directory)
previous_fast=r.read(prior_fast_dir/'incremental-fast-rows.json')['rows'];previous_perf=[x for x in r.read(prior_perf_dir/'incremental-performance-rows.json')['rows'] if x['family_id']=='mixed_load_bearing'];first=r.read(first_dir/'receipt.json');first_value=r.read(first_dir/'validation.json');assert first['fast_iteration_pass'] and not first['fully_verified'] and first_value['fast_iteration_pass'] and not first_value['issues'] and not first_value['violations']
first_outcome=r.read(Path(first['evidence'])/'outcome.json');first_row=dict(case='agent-episodes-1',seed=1,mode='fast-verify',evidence=first['evidence'],source_identity=r.source_identity(first_outcome),environment_identity=first['environment_identity'],input_identity=first['input_identity'],assurance_status='fast_iteration_verified',fast_iteration_pass=True,verification_pass=False,issues=[],violations=[])
all_fast=[first_row]+previous_fast+rows;assert len(all_fast)==len(previous_perf)==12
fast_slots={(x['case'],x['seed']):x for x in all_fast};perf_slots={(x['case'],x['seed']):x for x in previous_perf};assert len(fast_slots)==len(perf_slots)==12 and set(fast_slots)==set(perf_slots)
config=r.read(C/'evidence-builds.json');bridge=next(x for x in config['product_compatibility'] if x['kind']==r.RENAME_CACHE_BRIDGE_KIND and x['old_revision']==r.RENAME_CACHE_PARENT and x['new_revision']==REV);assert bridge['source_proof']==r.rename_cache_source_proof(r.RENAME_CACHE_PARENT,REV)
recipe_bridge=next(x for x in config['verification_compatibility'] if x['family_id']=='mixed_load_bearing' and x['performance_revision']==REV and x['verification_revision']==r.RENAME_CACHE_PARENT);assert set(recipe_bridge['unchanged_paths'])==r.bridge_dependency_paths('mixed_load_bearing')
for filename,expected in recipe_bridge['unchanged_paths'].items():r.validate_bridge_path(filename,expected,[REV,r.RENAME_CACHE_PARENT])
joins=[]
for slot,proof in sorted(fast_slots.items()):
    perf=perf_slots[slot];assert proof['input_identity']==perf['input_identity'] and proof['environment_identity']==perf['environment_identity'] and proof['fast_iteration_pass'] and not proof['verification_pass']
    assert perf['source_identity']['source_revision']==REV and not perf['issues'] and not perf['violations']
    joins.append(dict(case=slot[0],seed=slot[1],performance_evidence=perf['evidence'],performance_source=perf['source_identity'],fast_evidence=proof['evidence'],fast_source=proof['source_identity'],input_identity=proof['input_identity'],environment_identity=proof['environment_identity'],assurance_status='fast_iteration_verified',fully_verified=False))
r.custody.write_json(OUT/'family-completion.json',dict(schema='phase1-qualified-family-completion-v1',status='PASS',issue=29,family_id='mixed_load_bearing',performance_passes=12,accepted_routine_fast_passes=12,full_verification_passes=0,suppressed_cases=[],joins=joins,rename_source_bridge=bridge,recipe_bridge=recipe_bridge,prior_qualified_receipts={str(d):r.custody.sha(d/'evidence.sha256') for d in [prior_fast_dir,prior_perf_dir,first_dir]},source_map_sha256=r.custody.sha(C/'evidence-builds.json'),new_validations=1,reused_fast_verdicts=11,reused_performance_verdicts=12,scope='Family29 routine Phase1 acceptance complete under fast amendment. Fast remains not fully verified; original actual source identities and all historical failures preserved. Not a globalPhase1 terminal claim.'))

r.custody.seal(OUT)
print(json.dumps(dict(status='PASS',assurance='fast_iteration_verified',new_fast_proofs=1,new_full_proofs=0,issues=0,violations=0,receipt=str(OUT/'incremental-fast-rows.json'))))
