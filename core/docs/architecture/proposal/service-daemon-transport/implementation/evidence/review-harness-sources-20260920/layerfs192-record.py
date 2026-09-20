import fcntl, hashlib, json, os, signal, subprocess, sys, time
from pathlib import Path
root=Path('/Users/yifanxu/.codex/worktrees/pair3-foundation/layerfs');os.chdir(root)
name=sys.argv[1];cmd=sys.argv[2:];locks=[]
lock_paths=[(Path(os.environ.get('TMPDIR','/tmp'))/'layerfs-infra-measurement.lock').resolve(),Path('/tmp/layerfs-infra-measurement.lock').resolve()]
for path in dict.fromkeys(lock_paths):
 f=open(path,'a');fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB);locks.append(f)
p=root/'core/docs/architecture/proposal/service-daemon-transport/implementation/evidence'/name;p.mkdir(exist_ok=False)
files=sorted(f for f in (root/'core/crates').rglob('*') if f.is_file()and(f.suffix in('.rs','.sql','.py')or f.name in('Cargo.toml','Dockerfile'))) + [root/'core/Cargo.toml',root/'core/Cargo.lock']
source={str(f.relative_to(root)):hashlib.sha256(f.read_bytes()).hexdigest()for f in files};(p/'source.json').write_text(json.dumps(source,indent=2)+'\n')
env=os.environ.copy();env.update(CARGO_TARGET_DIR=str(root/'core/target'),CARGO_BUILD_JOBS='2',LAYERFS_CONSTRUCTION_WORKERS='1',CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER='/Users/yifanxu/.rustup/toolchains/1.85.1-aarch64-apple-darwin/lib/rustlib/aarch64-apple-darwin/bin/rust-lld')
start=time.monotonic()
with(p/'command.log').open('w')as log:
 process=subprocess.Popen(cmd,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
 try:code=process.wait(timeout=60)
 except subprocess.TimeoutExpired:
  os.killpg(process.pid,signal.SIGTERM)
  try:process.wait(timeout=3)
  except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
  code=124
stable=all(hashlib.sha256((root/f).read_bytes()).hexdigest()==h for f,h in source.items())
result={'command':cmd,'status':'PASS'if code==0 and stable else'FAIL','returncode':code,'wall_seconds':time.monotonic()-start,'source_unchanged':stable,'head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'target':env['CARGO_TARGET_DIR'],'jobs':2}
(p/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result));print((p/'command.log').read_text()[-3500:]);sys.exit(code)
