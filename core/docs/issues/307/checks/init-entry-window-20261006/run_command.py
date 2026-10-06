import os,sys,time,json,subprocess,signal,pathlib
stem=sys.argv[1]; cap=float(sys.argv[2]); cmd=sys.argv[3:]
p=pathlib.Path('core/docs/issues/307/checks/init-entry-window-20261006')/stem
assert not p.with_suffix('.json').exists() and not p.with_suffix('.log').exists()
env=dict(os.environ, CARGO_TARGET_DIR=str(pathlib.Path('core/target/init-acquisition-fix-20261006').resolve()),LAYERFS_CONSTRUCTION_WORKERS='1')
start=time.monotonic_ns()
with p.with_suffix('.log').open('xb') as log:
 child=subprocess.Popen(cmd,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
 timeout=False
 try: code=child.wait(timeout=cap)
 except subprocess.TimeoutExpired:
  timeout=True; os.killpg(child.pid,signal.SIGKILL); code=child.wait()
r={'command':cmd,'wall_timeout_s':cap,'wall_ns':time.monotonic_ns()-start,'exit_code':code,'timeout':timeout,'target':env['CARGO_TARGET_DIR'],'construction_workers':1}
p.with_suffix('.json').write_text(json.dumps(r,indent=2)+'\n')
print(json.dumps(r)); print(p.with_suffix('.log').read_text()[-10000:]);sys.exit(code if code>=0 else 1)
