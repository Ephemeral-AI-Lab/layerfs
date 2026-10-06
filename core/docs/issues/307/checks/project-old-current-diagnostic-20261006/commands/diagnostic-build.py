from pathlib import Path
import subprocess,json,os,sys,time,signal
arm=sys.argv[1];root=Path('/Users/yifanxu/.codex/worktrees/'+('project-init-run-backed-diagnostic' if arm=='old' else 'namespace-init-benchmark')+'/layerfs')
out=Path('/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/docs/issues/307/checks/project-old-current-diagnostic-20261006')
command=['cargo','+1.85.1','build','--manifest-path','core/Cargo.toml','--release','--locked','--target-dir','core/target/stage-diagnostic' if arm=='old' else 'core/target','-p','layerfs-project','--example','project_init_stages']
start=time.monotonic_ns()
with (out/(arm+'-build.log')).open('x') as f:
 child=subprocess.Popen(command,cwd=root,env={**os.environ,'LAYERFS_CONSTRUCTION_WORKERS':'1'},stdout=f,stderr=subprocess.STDOUT,start_new_session=True)
 try:code=child.wait(timeout=60);expired=False
 except subprocess.TimeoutExpired:expired=True;os.killpg(child.pid,signal.SIGKILL);code=child.wait(timeout=5)
r={'command':command,'cwd':str(root),'timeout_s':60,'exit_code':code,'timed_out':expired,'wall_ns':time.monotonic_ns()-start}
with (out/(arm+'-build.json')).open('x') as f:json.dump(r,f,indent=2);f.write('\n')
print(r);print((out/(arm+'-build.log')).read_text()[-2200:]);raise SystemExit(code)
