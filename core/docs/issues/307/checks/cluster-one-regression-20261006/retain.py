from pathlib import Path
import hashlib,json,shutil,re,subprocess
primary=Path.cwd();root=Path('/Users/yifanxu/.codex/worktrees/init-entry-performance/layerfs')
out=primary/'core/docs/issues/307/checks/cluster-one-regression-20261006';retained=primary/'benchmark-results/fs-bench-pro/cluster-one-regression-retained-20261006';retained.mkdir(exist_ok=False)
assert not (out/'ledger.json').exists()
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
rows=[];copies=[];post={};seals=set()
for profile in ['durable','disposable']:
 for n in [100,1000,10000,100000]:
  case=f'phase7-sqlite-{"disposable-" if profile=="disposable" else ""}init-{n}-cluster-one-regression-v1'
  src=root/'benchmark-results/fs-bench-pro'/f'cluster-one-regression-20261006-{case}-candidate';r=json.loads((src/'receipt.json').read_text());manifest=json.loads((src/'manifest.json').read_text());ref=r['regression_reference']
  assert r['case']==case and r['sample_count']==1 and r['measured_source_commit']=='7878bbbb40b2d162e03dcb6e4e43da7b63d5b5e4' and not r['identity']['source_dirty']
  assert r['status']=='COMPLETE' and r['cache_status']==r['verification_status']==r['cleanup']['status']=='PASS'
  assert r['residency']['resident_after']==0 and r['performance']['child']['root']==ref['root']
  assert r['fixture']['manifest_sha256']==ref['fixture']['manifest_sha256'] and r['construction_workers']==4 and r['environment_workers']==1
  assert r['effective_profile']==ref['effective_profile'];assert r['pack_layout']=='monolithic'
  assert r['command_wall_ns']<=30_000_000_000 and r['verification_wall_ns']<=19_000_000_000 and r['build']['wall_ns']<=30_000_000_000
  assert r['product_seal']=='44f07dd216a13d3103440b15b6d295a026ee80878f6e572e468855f27e2e74c5'
  for key,entry in manifest['files'].items():assert sha(src/key)==entry['sha256'],(case,key)
  dst=retained/case;shutil.copytree(src,dst,copy_function=shutil.copy2)
  for p in src.rglob('*'):
   if p.is_file():assert sha(p)==sha(dst/p.relative_to(src));copies.append({'source':str(p),'copy':str(dst/p.relative_to(src)),'sha256':sha(p),'bytes':p.stat().st_size})
  compact=out/'raw'/case;compact.mkdir(parents=True)
  for p in src.iterdir():
   if p.is_file() and p.suffix in ['.json','.stdout','.stderr']:shutil.copy2(p,compact/p.name)
  for binary in r['build']['binaries'].values():
   p=Path(binary['path']);assert sha(p)==binary['sha256'];post[str(p)]=sha(p)
   archive=retained/'binaries'/binary['sha256'];archive.mkdir(parents=True,exist_ok=True)
   if not (archive/p.name).exists():shutil.copy2(p,archive/p.name)
   assert sha(archive/p.name)==binary['sha256']
  for p,expected in [(r['fixture']['manifest'],r['fixture']['manifest_sha256']),(r['cold_helper']['binary'],r['cold_helper']['binary_sha256'])]:assert sha(p)==expected;post[p]=expected
  seals.add((r['product_seal'],r['compilation_seal'],r['dependency_seal'],r['identity']['harness_seal']))
  speed=10*r['comparison_ns']<=11*ref['comparison_ns'];storage=r['storage_bytes']<=ref['storage_bytes'];old=ref['old_competitive_reference'];old_speed=10*r['comparison_ns']<=11*old['comparison_ns'];old_storage=r['storage_bytes']<=old['storage_bytes']
  sql={}
  for name in ['statements','vm_steps','fullscan_steps','sorts','autoindex_rows','reprepares','write_transactions','write_commits','sealed_body_bytes','preallocation_calls','preallocation_bytes','statement_ns','commit_ns','transaction_ns']:
   values=re.findall(r'\b'+name+r': (\d+)',(src/'driver.stderr').read_text())
   if values:sql[name]=int(values[0])
  child=r['performance']['child']
  rows.append({'case':case,'profile':profile,'files':n,'result':'PASS' if speed and storage else 'FAIL','speed_gate':'PASS' if speed else 'FAIL','storage_gate':'PASS' if storage else 'FAIL','control_ns':ref['comparison_ns'],'candidate_ns':r['comparison_ns'],'latency_delta_ns':r['comparison_ns']-ref['comparison_ns'],'latency_delta_percent':100*(r['comparison_ns']-ref['comparison_ns'])/ref['comparison_ns'],'speed_integer_operands':[10*r['comparison_ns'],11*ref['comparison_ns']],'control_allocation_bytes':ref['storage_bytes'],'candidate_allocation_bytes':r['storage_bytes'],'allocation_delta_bytes':r['storage_bytes']-ref['storage_bytes'],'allocation_delta_percent':100*(r['storage_bytes']-ref['storage_bytes'])/ref['storage_bytes'],'storage_breakdown':r['storage'],'complete_command_ns':r['command_wall_ns'],'proof_ns':r['verification_wall_ns'],'build_ns':r['build']['wall_ns'],'phases':{k:child[k] for k in ['bootstrap_ns','init_ns','checkpoint_ns','close_ns']},'cpu_ns':r['performance']['cpu_ns'],'driver_lifetime_rss_bytes':r['performance']['peak_rss_bytes'],'sql':sql,'cold':r['residency'],'oracle':r['verification']['child'],'functional':'PASS','cleanup':'PASS','root_equal':True,'main_control':ref,'candidate_receipt':str(compact/'receipt.json'),'candidate_seals':{k:r[k] for k in ['product_seal','compilation_seal','dependency_seal','root_cargo_config_sha256','shipped_sql_seal']},'identity':r['identity'],'binaries':r['build']['binaries'],'competing_work':r['competing_work'],'old_competitive_gate':{'control_source':old['source'],'control_ns':old['comparison_ns'],'control_allocation_bytes':old['storage_bytes'],'speed':'PASS' if old_speed else 'FAIL','storage':'PASS' if old_storage else 'FAIL','joint':'PASS' if old_speed and old_storage else 'FAIL','scope':'older7edddb Service MEMORY/OFF split Store; separate contextual competitive screen, not same-profile regression control'}})
assert len(seals)==1
actual_head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip();dirty=subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True).splitlines();assert actual_head=='7878bbbb40b2d162e03dcb6e4e43da7b63d5b5e4' and not dirty
ledger={'status':'COMPLETE','source':actual_head,'tree':'5d5d885bd8935041c5740d3c2e158f19fd6a6576','main_control_source':'197d2fb7d0a141d7a9350852022febeec3255bf2','main_control_tree':'dbbe49212b26294024be28986e868c27f4de4825','restored_behavior':'9b74ac035faaf271edd01726913f63f084adaaa2','gate':'10*current_ns <= 11*same-profile cluster-one-end ns AND final DB/WAL/SHM <= same-profile cluster-one-end allocated total; required correctness/cache/cleanup/limits','budgets_ns':{'build':30000000000,'complete_performance':30000000000,'separate_proof':19000000000},'selected_order':[r['case'] for r in rows],'sample_count_per_case':1,'speed_passes':sum(r['speed_gate']=='PASS' for r in rows),'storage_passes':sum(r['storage_gate']=='PASS' for r in rows),'joint_passes':sum(r['result']=='PASS' for r in rows),'rows':rows,'unrun_selected':[],'historical_cases':'not resampled or relabeled; all original FAIL/NOT_RUN remain','limits':['same profile and whole public Project lifecycle, with old run-backed versus current backed-acquisition difference disclosed','metadata residency unobserved','cold contract is content pages only','driver lifetime RSS/CPU not phase/system residency or exclusive engine work','final allocated bytes not high-water','independent verifier samples file content','no full Workspace/FUSE/daemon runtime or S7/S8/S9 acceptance'],'post_source_dirty':dirty,'post_binary_helper_fixture_seals':post,'retained_root':str(retained)}
(out/'ledger.json').write_text(json.dumps(ledger,indent=2)+'\n');(out/'closed-copy-manifest.json').write_text(json.dumps({'status':'PASS','method':'full closed append-only raw manifests rehashed, ordinary independent byte copies including stores and immutable binaries, every copy hash rechecked','files':copies},indent=2)+'\n')
(out/'measure.py').write_bytes(Path('/tmp/cluster-one-regression-measure.py').read_bytes());(out/'retain.py').write_bytes(Path(__file__).read_bytes())
print(json.dumps({'status':'COMPLETE','speed_passes':ledger['speed_passes'],'storage_passes':ledger['storage_passes'],'joint_passes':ledger['joint_passes'],'rows':[{k:r[k] for k in ['profile','files','control_ns','candidate_ns','latency_delta_percent','control_allocation_bytes','candidate_allocation_bytes','allocation_delta_bytes','allocation_delta_percent','speed_gate','storage_gate']} for r in rows]},indent=2))
