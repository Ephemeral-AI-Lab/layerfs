import os,sys,time,json,signal,subprocess
from pathlib import Path
folder=Path(__file__).resolve().parent
label=sys.argv[1];limit=int(sys.argv[2]);command=sys.argv[3:]
assert 0<limit<=120
record={'command':command,'cwd':os.getcwd(),'wall_timeout_s':limit,'source':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'dirty':subprocess.check_output(['git','status','--porcelain'],text=True).splitlines()}
start=time.monotonic_ns()
with (folder/(label+'.stdout')).open('xb') as out,(folder/(label+'.stderr')).open('xb') as err:
 child=subprocess.Popen(command,stdout=out,stderr=err,start_new_session=True,env={**os.environ,'CARGO_TARGET_DIR':str(Path.cwd()/'core/target'),'LAYERFS_CONSTRUCTION_WORKERS':'1'})
 try:record['exit_code']=child.wait(timeout=limit);record['timed_out']=False
 except subprocess.TimeoutExpired:
  os.killpg(child.pid,signal.SIGKILL);record['exit_code']=child.wait();record['timed_out']=True
record['wall_ns']=time.monotonic_ns()-start
with (folder/(label+'.json')).open('x') as f:json.dump(record,f,indent=2);f.write('\n')
print(json.dumps(record));print((folder/(label+'.stderr')).read_text()[-4000:]);print((folder/(label+'.stdout')).read_text()[-2000:]);sys.exit(1 if record['timed_out'] or record['exit_code'] else 0)
