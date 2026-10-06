import pathlib,shutil,hashlib,json,subprocess,time
root=pathlib.Path('/Users/yifanxu/.codex/worktrees/init-entry-performance/layerfs'); primary=pathlib.Path.cwd(); old=pathlib.Path('/Users/yifanxu/.codex/worktrees/namespace-init-benchmark/layerfs')
p=primary/'core/docs/issues/307/checks/init-entry-window-results-20261006';p.mkdir();start=time.monotonic_ns()
def digest(f):return hashlib.sha256(f.read_bytes()).hexdigest()
def copy_tree(src,dst):
 assert not dst.exists();shutil.copytree(src,dst,copy_function=shutil.copy2)
 files=[]
 for f in sorted(src.rglob('*')):
  if f.is_file():
   rel=f.relative_to(src);sha=digest(f);assert sha==digest(dst/rel);files.append([str(rel),sha])
 return {'source':str(src),'destination':str(dst),'method':'independent ordinary byte copy of closed inputs, source and destination SHA256 verified','files':files}
r={'candidate_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'candidate_tree':subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=root,text=True).strip(),'copies':[],'cache':'setup copies are not cold; native input-content invalidation/whole-input residency attestation required anew','reference_reuse':[]}
assert not subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True)
r['copies'].append(copy_tree(primary/'core/target/cluster2-runtime-tests/release',root/'core/target/release'))
for src in sorted((old/'benchmark-results/fs-bench-pro/sdk-prepared').iterdir()):
 r['copies'].append(copy_tree(src,root/'benchmark-results/fs-bench-pro/sdk-prepared'/src.name))
# Inspect actual unchanged reference checkout and verify the original relevant seals.
ref=old/'target/phase7-baseline/layerfs';assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=ref,text=True).strip()=='7edddbdb8e8512627aed0ed42533ef099d802384';assert not subprocess.check_output(['git','status','--porcelain'],cwd=ref,text=True)
def scope(paths):
 h=hashlib.sha256()
 for f in sorted(paths):h.update(str(f.relative_to(ref)).encode()+b'\0');h.update(f.read_bytes())
 return h.hexdigest()
crates=ref/'core/crates';product=list(crates.glob('*/src/**/*.rs'))+list(crates.glob('*/sql/**/*.sql'))+list((crates/'layerfs-api').glob('*/src/**/*.rs'))
compilation=product+list(crates.glob('*/Cargo.toml'))+list((crates/'layerfs-api').glob('*/Cargo.toml'))+list(crates.glob('*/examples/**/*.rs'))+list((crates/'layerfs-api').glob('*/examples/**/*.rs'))+[ref/'core/Cargo.toml',ref/'core/Cargo.lock',ref/'.cargo/config.toml']
seals={'product_seal':scope(product),'shipped_sql_seal':scope(list(crates.glob('*/sql/**/*.sql'))),'compilation_seal':scope(compilation),'dependency_seal':digest(ref/'core/Cargo.lock'),'root_cargo_config_sha256':digest(ref/'.cargo/config.toml')}
for case in ['phase7-sqlite-init-100-acquisition-v2','phase7-sqlite-disposable-init-100-acquisition-v2','phase7-sqlite-init-1000-acquisition-v2','phase7-sqlite-disposable-init-1000-acquisition-v2']:
 receipt=primary/'core/docs/issues/307/checks/init-acquisition-benchmark-20261006/raw'/f'{case}-baseline/receipt.json';v=json.loads(receipt.read_text())
 for k,sha in seals.items():assert sha==v[k],(case,k,sha,v[k])
 for b in v['build']['binaries'].values():assert digest(pathlib.Path(b['path']))==b['sha256']
 assert digest(root/'core/benchmark/fs-bench-pro/diagnostics/sqlite_reference_init.rs')==v['reference_driver_source_sha256']
 assert digest(root/'core/benchmark/fs-bench-pro/shared/cold_native.c')==v['cold_source_sha256']
 assert digest(pathlib.Path(v['cold_helper']['binary']))==v['cold_helper']['binary_sha256']
 assert v['verification_status']=='PASS' and v['cache_status']=='PASS' and v['cleanup']['status']=='PASS'
 r['reference_reuse'].append({'case':case,'receipt':str(receipt),'receipt_sha256':digest(receipt),'relevant_seals':seals,'binaries':v['build']['binaries'],'proof_status':v['verification_status'],'root':v['performance']['child']['root'],'fixture_sha256':v['fixture']['manifest_sha256']})
r['wall_ns']=time.monotonic_ns()-start;(p/'setup-reference-reuse.json').write_text(json.dumps(r,indent=2)+'\n');print('verified setup copies',len(r['copies']),'reference rows',len(r['reference_reuse']),'wall_ns',r['wall_ns'])
