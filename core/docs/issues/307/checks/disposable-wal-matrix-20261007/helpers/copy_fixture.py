from pathlib import Path
import fcntl,hashlib,json,os,stat,sys,time
sys.path.insert(0,'core/benchmark/fs-bench-pro')
from families import init_namespace
import runner
source_home=runner.RESULTS/'sdk-prepared/namespace-100000-b69e710dfd0474a5'
root=runner.RESULTS/'disposable-wal-prepared.noindex'
with (runner.RESULTS/'phase7-sqlite.lock').open('a+b') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
 started=time.monotonic_ns()
 assert not root.exists()
 root.mkdir()
 target_home=root/source_home.name
 partial=root/('.'+source_home.name+'.partial')
 partial.mkdir();(partial/'payload').mkdir()
 raw=(source_home/'manifest.tsv').read_bytes()
 directories=[];files=total=0
 for line in raw.decode().splitlines():
  relative,kind,mode,mtime,size,sha=line.split('\t')
  source=source_home/'payload'/relative;target=partial/'payload'/relative
  mode,mtime,size=int(mode),int(mtime),int(size)
  st=source.lstat()
  assert stat.S_IMODE(st.st_mode)==mode and st.st_mtime_ns==mtime
  if kind=='d':
   if relative!='.':target.mkdir()
   directories.append((target,mode,mtime));continue
  assert kind=='f' and stat.S_ISREG(st.st_mode) and st.st_size==size
  h=hashlib.sha256();copied=0
  with source.open('rb') as inp,target.open('xb') as out:
   while block:=inp.read(131072):out.write(block);h.update(block);copied+=len(block)
  assert copied==size and h.hexdigest()==sha
  os.chmod(target,mode);os.utime(target,ns=(mtime,mtime))
  assert target.stat().st_ino!=st.st_ino and target.stat().st_nlink==1
  files+=1;total+=copied
 for target,mode,mtime in reversed(directories):
  os.chmod(target,mode);os.utime(target,ns=(mtime,mtime))
 (partial/'manifest.tsv').write_bytes(raw)
 (partial/'fixture.json').write_bytes((source_home/'fixture.json').read_bytes())
 partial.rename(target_home)
 receipt=init_namespace.prepare(init_namespace.CASES['namespace-100000'],root)
 assert receipt['reused'] and receipt['manifest_sha256']==hashlib.sha256(raw).hexdigest()
 record={'source':str(source_home),'destination':str(target_home),'files':files,'bytes':total,'directory_count':len(directories),'window_bytes':131072,'method':'independent read/write byte stream; each file SHA256 checked against original manifest; no APFS clone or hard link','preparation_ns':time.monotonic_ns()-started,'fixture':receipt,'cold_claim':False,'original_fixture_preserved':True}
 with Path('core/docs/issues/307/checks/disposable-wal-matrix-20261007/21-independent-fixture-copy.json').open('x') as f:json.dump(record,f,indent=2);f.write('\n')
 print(json.dumps(record))
