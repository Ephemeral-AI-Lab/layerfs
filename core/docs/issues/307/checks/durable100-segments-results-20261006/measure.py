import os,sys,pathlib,subprocess,time,json,hashlib,signal
root=pathlib.Path('/Users/yifanxu/.codex/worktrees/init-entry-performance/layerfs');primary=pathlib.Path('/Users/yifanxu/Ephemeral-AI-Lab/layerfs');case=sys.argv[1]
p=primary/'core/docs/issues/307/checks/durable100-segments-results-20261006/commands';p.mkdir(exist_ok=True);stem=p/case
assert not stem.with_suffix('.json').exists()
report=(primary/'benchmark_agent_report.md').read_bytes()
out=root/'benchmark-results/fs-bench-pro'/('durable100-segments-20261006-'+case+'-candidate');assert not out.exists()
cmd=['python3','-B','core/benchmark/fs-bench-pro/runner.py','run','--case',case,'--arm','candidate','--out',str(out)]
start=time.monotonic_ns()
with stem.with_suffix('.stdout').open('xb') as stdout,stem.with_suffix('.stderr').open('xb') as stderr:
 child=subprocess.Popen(cmd,cwd=root,stdout=stdout,stderr=stderr,start_new_session=True,env={**os.environ,'LAYERFS_CONSTRUCTION_WORKERS':'1'})
 timeout=False
 try:code=child.wait(timeout=120)
 except subprocess.TimeoutExpired:
  timeout=True;os.killpg(child.pid,signal.SIGKILL);code=child.wait()
r={'command':cmd,'cwd':str(root),'wall_timeout_s':120,'wall_ns':time.monotonic_ns()-start,'exit_code':code,'timed_out':timeout,'output':str(out),'report_template_sha256':hashlib.sha256(report).hexdigest(),'candidate_source':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'candidate_dirty_after':subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True).splitlines()}
stem.with_suffix('.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r));print(stem.with_suffix('.stderr').read_text()[-2000:]);
if (out/'receipt.json').exists():
 v=json.loads((out/'receipt.json').read_text());print(json.dumps({k:v.get(k) for k in ['case','status','comparison_ns','command_wall_ns','verification_wall_ns','verification_status','cache_status','storage_bytes','cleanup','reason']}))
sys.exit(code if code>=0 else 1)
