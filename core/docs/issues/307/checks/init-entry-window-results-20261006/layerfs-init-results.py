import pathlib,json,hashlib,shutil,subprocess,re
primary=pathlib.Path.cwd();root=pathlib.Path('/Users/yifanxu/.codex/worktrees/init-entry-performance/layerfs');base=primary/'core/docs/issues/307/checks/init-entry-window-results-20261006';retained=primary/'benchmark-results/fs-bench-pro/init-entry-window-retained-20261006';retained.mkdir()
def sha(f):return hashlib.sha256(f.read_bytes()).hexdigest()
ledger={'schema':'issue307-entry-window-results-20261006','source_commit':'a6bd6860c5061e03c4803c8962d847eb348d6069','source_tree':'91ba0a4867426cab042e0e4fdf5bb681e16c09e0','gate':'10*candidate_product_ns <= 11*reference_product_ns; final allocation <= reference','budget_ns':{'performance':30000000000,'proof':19000000000,'build':30000000000,'outer':120000000000},'baseline_reuse':'four qualified original7edddbdb8 reference receipts; no reference resample; setup-reference-reuse.json validates relevant scopes','old_candidate_source':'4c03b41bf5fcc684df3a707d223ad2b9166396f8','cache_contract':'phase7-cold-content-fresh-database-complete-lifecycle-v1','pairs':[],'NOT_RUN':[f'phase7-sqlite-{profile}init-{size}-acquisition-v2' for profile in ['', 'disposable-'] for size in [10000,100000]],'v1':'all eight remain unsampled at 15/9.5s; all old verdicts retained','limits':['distinct reference MEMORY/OFF split layout vs candidate Monolithic/Durable WAL FULL or Disposable MEMORY OFF','no eligible matched-profile old Project comparison','metadata residency unobserved','wait4 lifetime RSS not phase/system peak','final allocation not peak','independent verifier sampled payload not all bytes','S7/S8/S9 and R1-R4/E1-E4/Q1/C1 incomplete; Commit P3/P6/P7/P13/P14 later']}
old=json.loads((primary/'core/docs/issues/307/checks/init-acquisition-window-results-20261006/ledger.json').read_text());copies=[]
for case in ['phase7-sqlite-init-100-acquisition-v2','phase7-sqlite-disposable-init-100-acquisition-v2','phase7-sqlite-init-1000-acquisition-v2','phase7-sqlite-disposable-init-1000-acquisition-v2']:
 src=root/'benchmark-results/fs-bench-pro'/('entry-window-20261006-'+case+'-candidate');r=json.loads((src/'receipt.json').read_text());refpath=primary/'core/docs/issues/307/checks/init-acquisition-benchmark-20261006/raw'/f'{case}-baseline/receipt.json';ref=json.loads(refpath.read_text());previous=next(x for x in old['pairs'] if x['case']==case)
 manifest=json.loads((src/'manifest.json').read_text())
 for file,meta in manifest['files'].items():assert sha(src/file)==meta['sha256'],file
 dst=retained/case;shutil.copytree(src,dst,copy_function=shutil.copy2)
 for f in src.iterdir():
  if f.is_file():assert sha(f)==sha(dst/f.name);copies.append({'source':str(f),'copy':str(dst/f.name),'sha256':sha(f),'bytes':f.stat().st_size})
 compact=base/'raw'/case;compact.mkdir(parents=True)
 for f in src.iterdir():
  if f.is_file() and f.suffix in ['.json','.stdout','.stderr']:shutil.copy2(f,compact/f.name)
 for k in ['case','cache_contract','command_budget_ns','verification_budget_ns','construction_workers','environment_workers','cold_source_sha256','reference_driver_source_sha256']:
  assert r[k]==ref[k],(case,k)
 assert r['identity']['harness_seal']==ref['identity']['harness_seal']
 assert r['fixture']['manifest_sha256']==ref['fixture']['manifest_sha256']
 assert r['sample_count']==1 and r['status']=='COMPLETE'
 assert r['measured_source_commit']==ledger['source_commit'] and r['identity']['source_tree']==ledger['source_tree'] and not r['identity']['source_dirty']
 assert r['verification_status']=='PASS' and r['cache_status']=='PASS' and r['residency']['resident_after']==0 and r['cleanup']['status']=='PASS'
 assert r['command_wall_ns']<=r['command_budget_ns'] and r['verification_wall_ns']<=r['verification_budget_ns'] and r['build']['wall_ns']<=30000000000
 for b in r['build']['binaries'].values():
  file=pathlib.Path(b['path']);assert sha(file)==b['sha256'];folder=retained/'binaries'/b['sha256'];folder.mkdir(parents=True,exist_ok=True);dstb=folder/file.name
  if not dstb.exists():shutil.copy2(file,dstb)
  assert sha(dstb)==b['sha256']
 assert ref['performance']['child']['root']==r['performance']['child']['root']
 speed=10*r['comparison_ns']<=11*ref['comparison_ns'];storage=r['storage_bytes']<=ref['storage_bytes']
 diagnostic={}
 stderr=(src/'driver.stderr').read_text()
 for name in ['statements','vm_steps','fullscan_steps','sorts','autoindex_rows','reprepares','write_transactions','statement_ns','commit_ns']:
  matches=re.findall(r'\b'+name+r': (\d+)',stderr)
  if matches:diagnostic[name]=int(matches[0])
 item={'case':case,'profile':r['requested_profile'],'joint_gate':'PASS' if speed and storage else 'FAIL','speed_gate':'PASS' if speed else 'FAIL','storage_gate':'PASS' if storage else 'FAIL','root_equal':True,'reference_receipt':str(refpath),'candidate_receipt':str(compact/'receipt.json'),'reference_product_ns':ref['comparison_ns'],'previous_candidate_product_ns':previous['candidate_product_ns'],'candidate_product_ns':r['comparison_ns'],'candidate_reference_ratio':r['comparison_ns']/ref['comparison_ns'],'candidate_previous_delta_percent':100*(r['comparison_ns']-previous['candidate_product_ns'])/previous['candidate_product_ns'],'speed_operands':[10*r['comparison_ns'],11*ref['comparison_ns']],'reference_allocation_bytes':ref['storage_bytes'],'candidate_allocation_bytes':r['storage_bytes'],'allocation_breakdown':r['storage'],'complete_performance_ns':r['command_wall_ns'],'proof_ns':r['verification_wall_ns'],'build_ns':r['build']['wall_ns'],'driver_cpu_ns':r['performance']['cpu_ns'],'driver_lifetime_rss_bytes':r['performance']['peak_rss_bytes'],'bootstrap_ns':r['performance']['child']['bootstrap_ns'],'init_ns':r['performance']['child']['init_ns'],'checkpoint_ns':r['performance']['child']['checkpoint_ns'],'close_ns':r['performance']['child']['close_ns'],'oracle':r['verification']['child'],'diagnostic':diagnostic,'candidate_seals':{k:r[k] for k in ['product_seal','shipped_sql_seal','compilation_seal','dependency_seal','root_cargo_config_sha256']},'binaries':r['build']['binaries'],'effective_profile':r['effective_profile'],'competing_work':r['competing_work']}
 ledger['pairs'].append(item);print(case, item['joint_gate'], 'product',item['candidate_product_ns'],'previous_delta%',item['candidate_previous_delta_percent'],'reference_ratio',item['candidate_reference_ratio'])
units=[json.loads(l) for l in (base/'diagnostic/disposable.stdout').read_text().splitlines() if l.startswith('{')];ledger['acquisition_diagnostic']={'scope':'uncontrolled/instrumented counts only, clocks ineligible','units':units,'sum':{k:sum(x[k] for x in units if x['unit']!='complete_init') for k in ['statements','vm_steps','write_commits','fullscan_steps','returned_rows']},'previous_sum':{'statements':2200,'vm_steps':394041,'write_commits':8},'previous_put_entries':{'statements':2023,'vm_steps':165261},'root':'a71b9151bb269cdc3e71d9424e36669edd7d78ed39f9055bc355cebd4d93ec2c'}
print('acquisition',ledger['acquisition_diagnostic']['sum']);(base/'ledger.json').write_text(json.dumps(ledger,indent=2)+'\n');(base/'closed-copy-manifest.json').write_text(json.dumps({'files':copies,'scope':'independent complete raw outputs retained outside Git; compact text receipts in raw/ do not contain SQLite files','retained_root':str(retained)},indent=2)+'\n')
for f in base.glob('diagnostic/*'):shutil.copy2(f,retained/f.name)
store=root/'benchmark-results/fs-bench-pro/entry-window-count-diagnostic-20261006/store.sqlite';shutil.copy2(store,retained/'diagnostic-store.sqlite');assert sha(store)==sha(retained/'diagnostic-store.sqlite')
# Confirm root/profile/store provenance and ARM compilation flags after sampling.
flags=[]
for f in (root/'core/target/release/.fingerprint').glob('layerfs-*/lib-*.json'):
 r=json.loads(f.read_text());flags.append({'file':str(f),'rustflags':r.get('rustflags')})
assert flags and all(all(x in str(y['rustflags']) for x in ['aes_armv8','polyval_armv8','chacha20_force_neon','+aes,+sha2']) for y in flags)
(base/'arm-profile.json').write_text(json.dumps(flags,indent=2)+'\n')
