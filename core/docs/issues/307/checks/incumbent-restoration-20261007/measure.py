"""One sample of one registered case/arm in its measurement worktree; append-only."""
import os,sys,pathlib,subprocess,time,json,hashlib,signal
primary=pathlib.Path('/Users/yifanxu/Ephemeral-AI-Lab/layerfs')
root=pathlib.Path(sys.argv[1]);case=sys.argv[2];arm=sys.argv[3];limit=int(sys.argv[4]);extra=sys.argv[5:]
campaign=os.environ.get('LAYERFS_CAMPAIGN','incumbent-restoration-20261007')
folder=root/'benchmark-results/fs-bench-pro'/(campaign+'-wrapper');folder.mkdir(exist_ok=True);stem=folder/(case+'-'+arm)
report=(primary/'benchmark_agent_report.md').read_bytes()
out=root/'benchmark-results/fs-bench-pro'/(campaign+'-'+case+'-'+arm);assert not out.exists()
cmd=['python3','-B','core/benchmark/fs-bench-pro/runner.py','run','--case',case,'--arm',arm,'--out',str(out)]+extra
start=time.monotonic_ns()
with stem.with_suffix('.stdout').open('xb') as stdout,stem.with_suffix('.stderr').open('xb') as stderr:
 child=subprocess.Popen(cmd,cwd=root,stdout=stdout,stderr=stderr,start_new_session=True,env={**os.environ,'LAYERFS_CONSTRUCTION_WORKERS':'1'})
 timed_out=False
 try:code=child.wait(timeout=limit)
 except subprocess.TimeoutExpired:
  timed_out=True;os.killpg(child.pid,signal.SIGKILL);code=child.wait()
r={'command':cmd,'cwd':str(root),'wall_timeout_s':limit,'wall_ns':time.monotonic_ns()-start,'exit_code':code,'timed_out':timed_out,'output':str(out),'report_template_sha256':hashlib.sha256(report).hexdigest(),'source':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'dirty_after':subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True).splitlines()}
with stem.with_suffix('.json').open('x') as f:json.dump(r,f,indent=2);f.write('\n')
print(json.dumps({k:r[k] for k in ['wall_ns','exit_code','timed_out','source','dirty_after']}),flush=True)
if (out/'receipt.json').exists():
 v=json.loads((out/'receipt.json').read_text());print(json.dumps({k:v.get(k) for k in ['case','arm','status','comparison_ns','command_wall_ns','verification_wall_ns','verification_status','cache_status','storage_bytes','cleanup','reason']}),flush=True)
else:print(stem.with_suffix('.stderr').read_text()[-2000:],flush=True)
sys.exit(1 if code or timed_out else 0)
