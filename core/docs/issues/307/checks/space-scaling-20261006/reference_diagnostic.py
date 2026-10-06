import os,sys,json,hashlib,fcntl,subprocess
from pathlib import Path
primary=Path('/Users/yifanxu/Ephemeral-AI-Lab/layerfs');root=Path('/Users/yifanxu/.codex/worktrees/cluster-one-scaling-reference/layerfs')
sys.path.insert(0,str(primary/'core/benchmark/fs-bench-pro'))
from families.phase7_sqlite import invoke
(root/'benchmark_agent_report.md').read_text()
out=root/'benchmark-results/fs-bench-pro/cluster-one-reference-attribution-20261006';out.mkdir(parents=True)
lock=(out.parent/'phase7-sqlite.lock').open('a+b');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
env={**os.environ,'LAYERFS_CONSTRUCTION_WORKERS':'1','LAYERFS_HISTORY_CURSOR_KEY':'28'*32,'CARGO_TARGET_DIR':str(root/'core/target')}
prior=json.loads((primary/'core/docs/issues/307/checks/cluster-one-regression-20261006/raw/phase7-sqlite-init-100000-cluster-one-regression-v1/receipt.json').read_text());fixture=prior['fixture'];source=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip();assert source=='197d2fb7d0a141d7a9350852022febeec3255bf2'
def digest(path):
 h=hashlib.sha256()
 with Path(path).open('rb') as f:
  for block in iter(lambda:f.read(1024*1024),b''):h.update(block)
 return h.hexdigest()
def seal(paths):
 h=hashlib.sha256()
 for path in sorted(paths):h.update(str(path.relative_to(root)).encode()+b'\0');h.update(path.read_bytes())
 return h.hexdigest()
product=list((root/'core/crates').glob('*/src/**/*.rs'))+list((root/'core/crates').glob('*/sql/**/*.sql'))+list((root/'core/crates/layerfs-api').glob('*/src/**/*.rs'))
record={'kind':'matching actual cluster-one public Project Init count diagnostic','status':'INCOMPLETE','admission_eligible':False,'cache':'uncontrolled/instrumented; acceptance reference unchanged','source_commit':source,'product_seal':seal(product),'dependency_sha256':digest(root/'core/Cargo.lock'),'cargo_config_sha256':digest(root/'.cargo/config.toml'),'fixture':fixture,'sample_count':0,'script_sha256':digest(__file__),'constructor_workers':4,'environment_workers':1,'physical_io_scope':'delegated SQLite VFS calls; device bytes and native scratch syscall counts unavailable'}
assert record['product_seal']==prior['regression_reference']['product_seal']
try:
 record['build']=invoke(['cargo','+1.85.1','build','--manifest-path','core/Cargo.toml','--release','--locked','-p','layerfs-project','--example','attribution_init','--example','verify_namespace'],out,'build',30_000_000_000,env,root)
 if record['build']['exit_code'] or record['build']['timed_out']:raise RuntimeError('reference diagnostic build failed; no Init attempted')
 library=root/'core/target/ref-attribution-vfs.dylib'
 record['observer_build']=invoke(['clang','-O2','-Wall','-Wextra','-Werror','-dynamiclib',str(primary/'core/benchmark/fs-bench-pro/diagnostics/sqlite_init_vfs_observer.c'),'-lsqlite3','-o',str(library)],out,'observer-build',30_000_000_000,env,root)
 if record['observer_build']['exit_code'] or record['observer_build']['timed_out']:raise RuntimeError('reference VFS build failed')
 binaries={name:root/'core/target/release/examples'/name for name in ['attribution_init','verify_namespace']};binaries['vfs_library']=library
 record['binaries']={name:{'path':str(path),'sha256':digest(path)} for name,path in binaries.items()}
 scratch=out/'scratch';scratch.mkdir()
 denv={**env,'DYLD_INSERT_LIBRARIES':str(library),'LAYERFS_ATTRIBUTION_VFS':'1','LAYERFS_CAUSE_VFS_LOG':str(out/'vfs.json')}
 record['run']=invoke([str(binaries['attribution_init']),fixture['source'],str(out/'store.sqlite'),str(scratch),'phase7-sqlite-init-100000-shared-allocation-v1'],out,'driver',15_000_000_000,denv,root);record['sample_count']=1
 if record['run']['exit_code'] or record['run']['timed_out']:raise RuntimeError('reference diagnostic failed')
 record['units']=[json.loads(line) for line in (out/'driver.stdout').read_text().splitlines() if line.startswith('{')]
 record['proof']=invoke([str(binaries['verify_namespace']),str(out/'store.sqlite'),str(out/'store.sqlite'),prior['performance']['child']['root'],'41'*16,fixture['manifest'],fixture['manifest_sha256'],'durable'],out,'verifier',19_000_000_000,env,root)
 if record['proof']['exit_code'] or record['proof']['timed_out']:raise RuntimeError('reference proof failed')
 record['cleanup']='PASS' if not list(scratch.iterdir()) and not list(out.glob('.layerfs-allocation-*')) else 'FAIL'
 record['status']='DIAGNOSTIC_COMPLETE';record['product_seal_after']=seal(product);assert record['product_seal_after']==record['product_seal']
 print(json.dumps([x for x in record['units'] if 'unit' in x],indent=2))
except Exception as e:
 record['status']='FAIL';record['reason']=str(e);raise
finally:
 (out/'receipt.json').write_text(json.dumps(record,indent=2)+'\n');lock.close();print('RECEIPT',out/'receipt.json')
