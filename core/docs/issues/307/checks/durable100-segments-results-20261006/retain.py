import pathlib,json,hashlib,shutil,sys,re

primary=pathlib.Path.cwd()
root=pathlib.Path('/Users/yifanxu/.codex/worktrees/init-entry-performance/layerfs')
cohort=sys.argv[1]
base=primary/'core/docs/issues/307/checks'/('durable100-'+cohort+'-results-20261006')
retained=primary/'benchmark-results/fs-bench-pro'/('durable100-'+cohort+'-retained-20261006')
retained.mkdir(exist_ok=True)
case='phase7-sqlite-init-100-acquisition-payload-segments-v1'
reference_case='phase7-sqlite-init-100-acquisition-v2'
src=root/'benchmark-results/fs-bench-pro'/('durable100-'+cohort+'-20261006-'+case+'-candidate')
assert not (base/'ledger.json').exists()
def sha(f):return hashlib.sha256(f.read_bytes()).hexdigest()
r=json.loads((src/'receipt.json').read_text())
refpath=primary/'core/docs/issues/307/checks/init-acquisition-benchmark-20261006/raw'/f'{reference_case}-baseline/receipt.json'
ref=json.loads(refpath.read_text())
manifest=json.loads((src/'manifest.json').read_text())
for f,meta in manifest['files'].items():assert sha(src/f)==meta['sha256'],f
dst=retained/case;assert not dst.exists();shutil.copytree(src,dst,copy_function=shutil.copy2)
copies=[{'source':str(f),'copy':str(dst/f.relative_to(src)),'sha256':sha(f),'bytes':f.stat().st_size} for f in src.rglob('*') if f.is_file()]
for f in src.rglob('*'):
    if f.is_file(): assert sha(f)==sha(dst/f.relative_to(src)),str(f)
compact=base/'raw'/case;compact.mkdir(parents=True)
for f in src.iterdir():
    if f.is_file():
        assert sha(f)==sha(dst/f.name)
        copies.append({'source':str(f),'copy':str(dst/f.name),'sha256':sha(f),'bytes':f.stat().st_size})
        if f.suffix in ['.json','.stdout','.stderr']:shutil.copy2(f,compact/f.name)
for k in ['cache_contract','command_budget_ns','verification_budget_ns','construction_workers','environment_workers','cold_source_sha256','reference_driver_source_sha256']:
    assert r[k]==ref[k],k
assert r['retained_reference_case']==reference_case
assert r['effective_layout']['schema_version']==10
assert r['fixture']['manifest_sha256']==ref['fixture']['manifest_sha256']
assert r['sample_count']==1 and r['status']=='COMPLETE' and not r['identity']['source_dirty']
assert r['verification_status']==r['cache_status']==r['cleanup']['status']=='PASS'
assert r['residency']['resident_after']==0
assert r['command_wall_ns']<=r['command_budget_ns']
assert r['verification_wall_ns']<=r['verification_budget_ns']
assert r['build']['wall_ns']<=30000000000
for b in r['build']['binaries'].values():
    f=pathlib.Path(b['path']);assert sha(f)==b['sha256']
    folder=retained/'binaries'/b['sha256'];folder.mkdir(parents=True,exist_ok=True)
    copy=folder/f.name
    if not copy.exists():shutil.copy2(f,copy)
    assert sha(copy)==b['sha256']
assert ref['performance']['child']['root']==r['performance']['child']['root']
speed=10*r['comparison_ns']<=11*ref['comparison_ns']
storage=r['storage_bytes']<=ref['storage_bytes']
sql={}
for name in ['statements','vm_steps','fullscan_steps','sorts','autoindex_rows','reprepares','write_transactions','statement_ns','commit_ns']:
    m=re.findall(r'\b'+name+r': (\d+)',(src/'driver.stderr').read_text())
    if m:sql[name]=int(m[0])
item={'case':case,'profile':r['requested_profile'],'joint_gate':'PASS' if speed and storage else 'FAIL','speed_gate':'PASS' if speed else 'FAIL','storage_gate':'PASS' if storage else 'FAIL','root_equal':True,'reference_product_ns':ref['comparison_ns'],'candidate_product_ns':r['comparison_ns'],'candidate_reference_ratio':r['comparison_ns']/ref['comparison_ns'],'speed_operands':[10*r['comparison_ns'],11*ref['comparison_ns']],'reference_allocation_bytes':ref['storage_bytes'],'candidate_allocation_bytes':r['storage_bytes'],'allocation_breakdown':r['storage'],'complete_performance_ns':r['command_wall_ns'],'proof_ns':r['verification_wall_ns'],'build_ns':r['build']['wall_ns'],'driver_cpu_ns':r['performance']['cpu_ns'],'driver_lifetime_rss_bytes':r['performance']['peak_rss_bytes'],'bootstrap_ns':r['performance']['child']['bootstrap_ns'],'init_ns':r['performance']['child']['init_ns'],'checkpoint_ns':r['performance']['child']['checkpoint_ns'],'close_ns':r['performance']['child']['close_ns'],'oracle':r['verification']['child'],'sql':sql,'binaries':r['build']['binaries'],'candidate_seals':{k:r[k] for k in ['product_seal','shipped_sql_seal','compilation_seal','dependency_seal','root_cargo_config_sha256']},'effective_profile':r['effective_profile'],'competing_work':r['competing_work'],'reference_receipt':str(refpath),'candidate_receipt':str(compact/'receipt.json')}
ledger={'source_commit':r['measured_source_commit'],'source_tree':r['identity']['source_tree'],'reference_receipt_sha256':sha(refpath),'new_case_identity':True,'gate':'10*candidate_product_ns <= 11*reference_product_ns; final allocation <= reference','budgets_ns':{'performance':30000000000,'proof':19000000000,'build':30000000000},'reference_reuse':'original eligible7edddbdb8 reference, unchanged and reverified; no resample','pairs':[item],'NOT_RUN':[f'phase7-sqlite-{profile}init-{size}-acquisition-v2' for profile in ['', 'disposable-'] for size in [100,1000,10000,100000] if not (profile=='' and size==100)],'v1':'all eight unsampled, original15/9.5s limits retained','limits':['distinct competitive MEMORY/OFF split reference versus PayloadSegments Durable WAL/FULL/fullfsync candidate','metadata residency unobserved','lifetime RSS not phase/system peak','final allocation not peak','independent verifier sampled payload','no S7/S9 runtime/resource closure']}
(base/'ledger.json').write_text(json.dumps(ledger,indent=2)+'\n')
(base/'closed-copy-manifest.json').write_text(json.dumps({'files':copies,'method':'ordinary independent byte copy after closed operation; complete manifest and all nested copy hashes verified; copied catalogue physical IDs still identify original files, so no portable reopening claim','retained_root':str(retained)},indent=2)+'\n')
print(json.dumps({'cohort':cohort,'source':ledger['source_commit'],'result':item['joint_gate'],'product_ns':item['candidate_product_ns'],'storage_bytes':item['candidate_allocation_bytes'],'sql':sql}))
