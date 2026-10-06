"""Untimed, recorded source preconditioning before a history arm; never a sample.

The sealed cold helper evicts resident corpus pages by opening each resident file
writable. Spotlight then re-reads those files and races the in-run attestation.
Evict here, outside every run, wait until no Spotlight worker has a corpus file
open, and repeat until one helper pass finds nothing left to invalidate. The arm
that follows still performs and must pass its own in-run attestation.
"""
import json,subprocess,sys,time,hashlib
from pathlib import Path
helper=Path(sys.argv[1]);label=sys.argv[2];rounds=int(sys.argv[3]) if len(sys.argv)>3 else 6
corpus=Path('/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data')
log=Path(__file__).resolve().parent/'precondition.jsonl'
def spotlight():
    out=subprocess.run(['lsof','-c','mdworker','-c','mds'],capture_output=True,text=True).stdout
    # An idle worker may keep the Finder metadata file open; the helper pass decides residency.
    return sum('deepseek-history-data' in line and not line.rstrip().endswith('/.DS_Store') for line in out.splitlines())
def settle(limit_s=900):
    start=time.monotonic();quiet=0
    while quiet<4:
        if time.monotonic()-start>limit_s:return None
        quiet=quiet+1 if spotlight()==0 else 0;time.sleep(5)
    return round(time.monotonic()-start,1)
ready=False
for index in range(rounds):
    waited=settle()
    if waited is None:row={'label':label,'round':index,'status':'SPOTLIGHT_BUSY'};break
    r=subprocess.run([str(helper),'--paths',str(corpus/'checkpoint-manifest.json'),str(corpus/'inputs'),str(corpus/'oracles')],capture_output=True,text=True,timeout=120)
    child=json.loads(r.stdout);row={'label':label,'round':index,'settle_s':waited,'helper_sha256':hashlib.sha256(helper.read_bytes()).hexdigest(),'exit_code':r.returncode,
        **{k:child[k] for k in ('files','total_pages','resident_first','resident_after','invalidated_files','first_pass_ns','attestation_ns')},'time':time.strftime('%Y-%m-%dT%H:%M:%S%z')}
    ready=child['resident_after']==0 and child['invalidated_files']<=16
    row['status']='READY' if ready else 'REPEAT'
    with log.open('a') as f:f.write(json.dumps(row)+'\n')
    print(json.dumps(row),flush=True)
    if ready:break
else:row={'label':label,'status':'NOT_CONVERGED'}
if not ready:
    with log.open('a') as f:f.write(json.dumps(row)+'\n')
    print(json.dumps(row),flush=True);sys.exit(1)
final=settle()
print(json.dumps({'label':label,'final_settle_s':final,'status':'READY' if final is not None else 'SPOTLIGHT_BUSY'}),flush=True);sys.exit(0 if final is not None else 1)
