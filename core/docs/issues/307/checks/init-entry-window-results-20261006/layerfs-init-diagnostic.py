import pathlib,subprocess,time,json,os,signal,hashlib,fcntl
root=pathlib.Path('/Users/yifanxu/.codex/worktrees/init-entry-performance/layerfs');p=pathlib.Path('/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/docs/issues/307/checks/init-entry-window-results-20261006/diagnostic');p.mkdir()
report=(root/'benchmark_agent_report.md').read_bytes()
locks=[]
for f in ['.run.lock','phase7-sqlite.lock']:
 h=(root/'benchmark-results/fs-bench-pro'/f).open('a+b');fcntl.flock(h,fcntl.LOCK_EX|fcntl.LOCK_NB);locks.append(h)
env={**os.environ,'CARGO_TARGET_DIR':str(root/'core/target'),'LAYERFS_CONSTRUCTION_WORKERS':'1'}
def run(name,cmd,cap):
 start=time.monotonic_ns()
 with (p/(name+'.stdout')).open('xb') as a,(p/(name+'.stderr')).open('xb') as b:
  child=subprocess.Popen(cmd,cwd=root,env=env,stdout=a,stderr=b,start_new_session=True);timeout=False
  try:code=child.wait(timeout=cap)
  except subprocess.TimeoutExpired:timeout=True;os.killpg(child.pid,signal.SIGKILL);code=child.wait()
 r={'command':cmd,'cwd':str(root),'wall_timeout_s':cap,'wall_ns':time.monotonic_ns()-start,'exit_code':code,'timed_out':timeout,'scope':'DIAGNOSTIC ONLY uncontrolled cache, counters not performance','report_template_sha256':hashlib.sha256(report).hexdigest()};(p/(name+'.json')).write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r));assert code==0 and not timeout
run('build',['cargo','+1.85.1','build','--manifest-path','core/Cargo.toml','--release','--locked','-p','layerfs-project','--example','acquisition_profile'],30)
binary=root/'core/target/release/examples/acquisition_profile';sha=hashlib.sha256(binary.read_bytes()).hexdigest();out=root/'benchmark-results/fs-bench-pro/entry-window-count-diagnostic-20261006';out.mkdir();source=root/'benchmark-results/fs-bench-pro/sdk-prepared/namespace-1000-compact-v3-6e3f54c14f81c5e9/payload'
run('disposable',[str(binary),str(source),str(out/'store.sqlite'),'disposable'],15)
assert sha==hashlib.sha256(binary.read_bytes()).hexdigest();(p/'identity.json').write_text(json.dumps({'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'source_tree':subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=root,text=True).strip(),'binary_sha256_pre_post':sha,'observer_sha256':hashlib.sha256((root/'core/crates/layerfs-project/examples/acquisition_profile.rs').read_bytes()).hexdigest(),'closed_store':str(out/'store.sqlite')},indent=2)+'\n');print((p/'disposable.stdout').read_text());print((p/'disposable.stderr').read_text()[:650])
