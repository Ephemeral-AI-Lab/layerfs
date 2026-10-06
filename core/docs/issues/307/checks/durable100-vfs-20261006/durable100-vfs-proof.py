import pathlib,sys,os,json,hashlib
root=pathlib.Path('/Users/yifanxu/.codex/worktrees/init-entry-performance/layerfs')
sys.path.insert(0,str(root/'core/benchmark/fs-bench-pro'))
import runner
from families.phase7_sqlite import invoke
(root/'benchmark_agent_report.md').read_text()
src=root/'benchmark-results/fs-bench-pro/durable100-vfs-count-20261006'
r=json.loads((src/'receipt.json').read_text());out=root/'benchmark-results/fs-bench-pro/durable100-vfs-proof-20261006';assert not out.exists();out.mkdir()
cmd=r['proof']['command'];before=runner.digest(src/'store.sqlite')
proof=invoke(cmd,out,'verifier',9500000000,{**os.environ,'LAYERFS_CONSTRUCTION_WORKERS':'1','LAYERFS_HISTORY_CURSOR_KEY':'28'*32},root)
after=runner.digest(src/'store.sqlite')
assert before==after
receipt={'kind':'separate corrected read-only proof after verifier configuration refusal','original_count_receipt_sha256':runner.digest(src/'receipt.json'),'original_proof_failure':'NotPresent: missing LAYERFS_HISTORY_CURSOR_KEY, failed before Store open','producer_replayed':False,'proof':proof,'environment':{'LAYERFS_CONSTRUCTION_WORKERS':'1','LAYERFS_HISTORY_CURSOR_KEY':'declared fixture cursor 28 repeated32'},'main_database_before_sha256':before,'main_database_after_sha256':after,'status':'PASS' if proof['exit_code']==0 and not proof['timed_out'] and proof['child']['status']=='PASS' else 'FAIL'}
runner.write_json(out/'receipt.json',receipt);runner.manifest_run(out);print(json.dumps(receipt))
assert receipt['status']=='PASS'
