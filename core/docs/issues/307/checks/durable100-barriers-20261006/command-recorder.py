import pathlib,sys,os,time,subprocess,signal,json,hashlib
p=pathlib.Path('core/docs/issues/307/checks/durable100-barriers-20261006')/sys.argv[1];cap=float(sys.argv[2]);cmd=sys.argv[3:]
assert not p.with_suffix('.json').exists();start=time.monotonic_ns();env={**os.environ,'LAYERFS_CONSTRUCTION_WORKERS':'1','CARGO_TARGET_DIR':os.environ.get('CARGO_TARGET_DIR',str(pathlib.Path('core/target/cluster2-runtime-tests').resolve()))}
with p.with_suffix('.stdout').open('xb') as a,p.with_suffix('.stderr').open('xb') as b:
 child=subprocess.Popen(cmd,stdout=a,stderr=b,env=env,start_new_session=True);timeout=False
 try:code=child.wait(timeout=cap)
 except subprocess.TimeoutExpired:timeout=True;os.killpg(child.pid,signal.SIGKILL);code=child.wait()
r={'command':cmd,'cwd':str(pathlib.Path.cwd()),'env_target':env['CARGO_TARGET_DIR'],'wall_timeout_s':cap,'wall_ns':time.monotonic_ns()-start,'exit_code':code,'timeout':timeout,'report_template_sha256':hashlib.sha256(pathlib.Path('benchmark_agent_report.md').read_bytes()).hexdigest()};p.with_suffix('.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r));print(p.with_suffix('.stdout').read_text()[-6500:]);print(p.with_suffix('.stderr').read_text()[-2500:]);sys.exit(code if code>=0 else 1)
