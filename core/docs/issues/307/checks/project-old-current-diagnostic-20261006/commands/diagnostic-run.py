from pathlib import Path
import subprocess,json,hashlib,os,sys,time,signal
arm,profile=sys.argv[1:];label=arm+'-'+profile
root=Path('/Users/yifanxu/.codex/worktrees/'+('project-init-run-backed-diagnostic' if arm=='old' else 'namespace-init-benchmark')+'/layerfs')
checks=Path('/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/docs/issues/307/checks/project-old-current-diagnostic-20261006')
report=(root/'benchmark_agent_report.md').read_bytes()
folder=root/'benchmark-results/fs-bench-pro/project-stage-diagnostic-20261006'/label;folder.mkdir(parents=True,exist_ok=False);scratch=folder/'scratch';scratch.mkdir()
source=root/'benchmark-results/fs-bench-pro/sdk-prepared/namespace-1000-compact-v3-6e3f54c14f81c5e9/payload'
binary=root/('core/target/stage-diagnostic/release/examples/project_init_stages' if arm=='old' else 'core/target/release/examples/project_init_stages')
command=[str(binary),str(source),str(folder/'store.sqlite')]+([str(scratch)] if arm=='old' else [])+['stage-diagnostic',profile,'15']
start=time.monotonic_ns()
with (checks/(label+'.stdout')).open('x') as out,(checks/(label+'.stderr')).open('x') as err:
 child=subprocess.Popen(command,cwd=root,env={**os.environ,'LAYERFS_CONSTRUCTION_WORKERS':'1','TMPDIR':str(scratch)},stdout=out,stderr=err,start_new_session=True)
 try:code=child.wait(timeout=15);expired=False
 except subprocess.TimeoutExpired:expired=True;os.killpg(child.pid,signal.SIGKILL);code=child.wait(timeout=5)
r={'command':command,'cwd':str(root),'timeout_s':15,'exit_code':code,'timed_out':expired,'wall_ns':time.monotonic_ns()-start,'scope':'DIAGNOSTIC_ONLY; uncontrolled cache; no performance admission','head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'production_diff':subprocess.check_output(['git','diff','--','core/crates/*/src','core/crates/*/sql'],cwd=root,text=True),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'driver_sha256':hashlib.sha256((root/'core/crates/layerfs-project/examples/project_init_stages.rs').read_bytes()).hexdigest(),'report_template_sha256':hashlib.sha256(report).hexdigest(),'scratch_remaining':list(map(str,scratch.iterdir()))}
if (folder/'init-timing.json').exists():
 timing=json.loads((folder/'init-timing.json').read_text());r['timing']=timing
 (checks/(label+'-timing.json')).write_text(json.dumps(timing,indent=2)+'\n')
with (checks/(label+'.json')).open('x') as f:json.dump(r,f,indent=2);f.write('\n')
print({k:v for k,v in r.items() if k not in ['timing','production_diff']});print((checks/(label+'.stdout')).read_text())
print('\n'.join(line for line in (checks/(label+'.stderr')).read_text().splitlines() if 'INIT_SQL_DELTA' in line or 'DIAGNOSTIC_ONLY' in line))
if code:print((checks/(label+'.stderr')).read_text()[-4000:])
raise SystemExit(code)
