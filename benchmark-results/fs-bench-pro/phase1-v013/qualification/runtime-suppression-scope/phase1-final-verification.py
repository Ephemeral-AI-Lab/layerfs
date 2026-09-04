import argparse,json,subprocess
from pathlib import Path

parser=argparse.ArgumentParser(description='Run the approved remaining Phase1 verifier plan after source-qualified active performance collection.')
parser.add_argument('assets',type=Path,help='Required newly qualified runtime assets directory')
args=parser.parse_args()
repo=Path(__file__).resolve().parents[1]
r=repo/'benchmark-results/fs-bench-pro/phase1-v013';q=r/'qualification';assets=args.assets.resolve()
plan=json.loads((repo/'target/phase1-final-verification-plan.json').read_text())
build=json.loads((assets/'evidence/build.json').read_text());revision=build['revision']
if build.get('status')!='pass' or not (assets/'evidence/evidence.sha256').is_file():raise SystemExit('Qualified sealed runtime assets required')
review=json.loads((r/'results/review.json').read_text())
ledger=json.loads((r/'phase1-runtime-suppressions.json').read_text())
if set(ledger['cases'])!=set(plan['runtime_suppression']['scenario_ids']):raise SystemExit('Update verifier plan to the persistent suppression inventory first')
performance=[row for row in review['rows'] if row['mode']=='performance']
expected=plan['performance_prerequisite']['active_performance_slots']
if review['source']['revision']!=revision or review['global_issues'] or len(performance)!=expected or any(row['evidence_status']!='PASS' or row['product_status']!='pass' for row in performance):raise SystemExit('Current-source report must first qualify every active performance slot; missing verification is allowed')
planned=[tuple(slot) for row in plan['invocations'] for slot in row['planned_slots']]
if len(planned)!=plan['counts']['new_executions'] or len(set(planned))!=len(planned) or len(planned)+len(plan['reused_proofs'])!=plan['counts']['total_required']:raise SystemExit('Verifier plan active slot counts differ')
print(f'PREREQUISITE_PASS revision={revision} active_performance={expected} remaining_verification={len(planned)}',flush=True)
for row in plan['invocations']:
 label=f"{row['family_id']}-{row['ordinal']}";command=[str(assets) if x=='{clean_runtime_assets}' else x for x in row['argv']]
 p=r/'run-status.json';d=json.loads(p.read_text());d.update(phase='final-independent-verification',active_family=row['family_id'],active_command=label,verification_revision=revision);p.write_text(json.dumps(d,indent=2)+'\n');print(f'START revision={revision} '+label,flush=True)
 with (q/f'final-verification-{revision[:12]}-{label}.stdout.txt').open('a') as o,(q/f'final-verification-{revision[:12]}-{label}.stderr.txt').open('a') as e:result=subprocess.run(command,stdout=o,stderr=e,cwd=repo)
 print(f'FINISH revision={revision} '+label+' exit='+str(result.returncode),flush=True)
 if result.returncode:raise SystemExit(result.returncode)
print(f'FINAL_VERIFICATION_COMMANDS_COMPLETE revision={revision}',flush=True)
