import os,sys,json,fcntl,hashlib,subprocess
from pathlib import Path
root=Path('/Users/yifanxu/.codex/worktrees/init-entry-performance/layerfs')
sys.path.insert(0,str(root/'core/benchmark/fs-bench-pro'))
import runner
from families.phase7_sqlite import invoke,archive
os.chdir(root)
(root/'benchmark_agent_report.md').read_text()
out=root/'benchmark-results/fs-bench-pro/init-streaming-refill-counts-20261007';out.mkdir()
lock=(runner.RESULTS/'phase7-sqlite.lock').open('a+b');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
env={**os.environ,'LAYERFS_CONSTRUCTION_WORKERS':'1','CARGO_TARGET_DIR':str(root/'core/target'),'LAYERFS_HISTORY_CURSOR_KEY':'28'*32}
record={'kind':'count-driven per-port scaling diagnostic','status':'INCOMPLETE','admission_eligible':False,'cache':'uncontrolled; no performance acceptance','identity':runner.identities(),'sample_count':0,'competing_work':runner.competing_work()}
assert not record['identity']['source_dirty']
prior=json.loads((Path('/Users/yifanxu/Ephemeral-AI-Lab/layerfs')/'core/docs/issues/307/checks/cluster-one-regression-20261006/raw/phase7-sqlite-init-100000-cluster-one-regression-v1/receipt.json').read_text());fixture=prior['fixture'];record['fixture']=fixture
try:
 record['build']=invoke(['cargo','+1.85.1','build','--manifest-path','core/Cargo.toml','--release','--locked','-p','layerfs-project','--example','durable_init_costs','--example','verify_namespace','--example','benchmark_init'],out,'build',30_000_000_000,env,root)
 assert record['build']['exit_code']==0 and not record['build']['timed_out']
 driver=archive(root/'core/target/release/examples/durable_init_costs',root,runner);verifier=archive(root/'core/target/release/examples/verify_namespace',root,runner);record.update(driver=driver,verifier=verifier)
 record['run']=invoke([driver['path'],fixture['source'],str(out/'store.sqlite'),'durable'],out,'driver',15_000_000_000,env,root);record['sample_count']=1
 assert record['run']['exit_code']==0 and not record['run']['timed_out']
 record['units']=[json.loads(line) for line in (out/'driver.stdout').read_text().splitlines() if line.startswith('{')]
 record['proof']=invoke([verifier['path'],str(out/'store.sqlite'),str(out/'store.sqlite'),prior['performance']['child']['root'],'41'*16,fixture['manifest'],fixture['manifest_sha256'],'durable'],out,'verifier',19_000_000_000,env,root)
 assert record['proof']['exit_code']==0 and not record['proof']['timed_out']
 record['status']='DIAGNOSTIC_COMPLETE';print(json.dumps(record['units'],indent=2))
except Exception as e:
 record['status']='FAIL';record['reason']=str(e);raise
finally:
 runner.write_json(out/'receipt.json',record);runner.manifest_run(out);lock.close();print('RECEIPT',out/'receipt.json')
