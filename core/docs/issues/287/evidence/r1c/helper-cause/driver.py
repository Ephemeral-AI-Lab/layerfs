import ctypes,hashlib,json,os,socket,struct,subprocess,sys
from pathlib import Path
root=Path(__file__).resolve().parent
helper=Path(sys.argv[1]).resolve()
library='/usr/lib/libsqlite3.dylib'
report={'kind':'LABELLED bounded helper cause diagnostic; no product/performance admission','helper':str(helper),'helper_sha256':hashlib.sha256(helper.read_bytes()).hexdigest(),'library':library,'stages':[]}
(root/'scratch_reader.source.py').write_bytes(helper.read_bytes())
lib=ctypes.CDLL(library)
lib.sqlite3_libversion.restype=ctypes.c_char_p
lib.sqlite3_sourceid.restype=ctypes.c_char_p
lib.sqlite3_open_v2.argtypes=[ctypes.c_char_p,ctypes.POINTER(ctypes.c_void_p),ctypes.c_int,ctypes.c_char_p]
lib.sqlite3_exec.argtypes=[ctypes.c_void_p,ctypes.c_char_p,ctypes.c_void_p,ctypes.c_void_p,ctypes.c_void_p]
lib.sqlite3_errmsg.argtypes=[ctypes.c_void_p]
lib.sqlite3_errmsg.restype=ctypes.c_char_p
lib.sqlite3_system_errno.argtypes=[ctypes.c_void_p]
lib.sqlite3_close.argtypes=[ctypes.c_void_p]
report.update(version=lib.sqlite3_libversion().decode(),source=lib.sqlite3_sourceid().decode())
database=root/'minimal-lfcs.sqlite'
fd=os.open(database,os.O_CREAT|os.O_EXCL|os.O_RDWR,0o600)
db=ctypes.c_void_p()
assert lib.sqlite3_open_v2(os.fsencode(database),ctypes.byref(db),2|0x01000000,None)==0
sql='PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF; PRAGMA application_id=1279673171; PRAGMA user_version=1; CREATE TABLE session_owner(id INTEGER PRIMARY KEY,records INTEGER NOT NULL,sealed INTEGER NOT NULL); INSERT INTO session_owner VALUES(1,0,0);'
rc=lib.sqlite3_exec(db,sql.encode(),None,None,None)
report['stages'].append({'stage':'independent minimal LFCS expected records0/sealed0','result':rc,'errno':lib.sqlite3_system_errno(db),'message':lib.sqlite3_errmsg(db).decode()})
assert rc==0
assert lib.sqlite3_close(db)==0
# A short unique owned socket path, not the potentially long worktree pathname.
socket_dir=Path('/tmp')/('lfcs-cause-'+str(os.getpid()))
socket_dir.mkdir(exist_ok=False)
socket_path=socket_dir/'r'
listener=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM)
listener.bind(str(socket_path));listener.listen(1);listener.settimeout(5)
child=subprocess.Popen([sys.executable,'-u',str(helper),library,'read',str(database),str(socket_path)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
control=None
try:
 control,_=listener.accept();control.setblocking(True);control.settimeout(5)
 report['stages'].append({'stage':'helper control connected'})
 ready=control.recv(1)
 report['stages'].append({'stage':'helper first acknowledgement','bytes_hex':ready.hex()})
 if ready==b'\x01':
  def exact(n):
   value=b''
   while len(value)<n:
    part=control.recv(n-len(value))
    assert part,'unexpected EOF in acknowledged frame'
    value+=part
   return value
  size=struct.unpack('>I',exact(4))[0];assert 0<size<=8192
  report['stages'].append({'stage':'helper provider frame','data':exact(size).decode()})
  control.sendall(b'\x02')
  report['stages'].append({'stage':'helper released','bytes_hex':exact(1).hex()})
except Exception as error:
 report['stages'].append({'stage':'control exception','type':type(error).__name__,'message':str(error),'errno':getattr(error,'errno',None)})
finally:
 if control is not None:control.close()
 listener.close()
 try:
  stdout,stderr=child.communicate(timeout=5)
 except subprocess.TimeoutExpired:
  child.kill();stdout,stderr=child.communicate(timeout=5)
  report['stages'].append({'stage':'owned bounded kill/reap'})
 report.update(exit=child.returncode,stdout=stdout.decode(errors='replace'),stderr=stderr.decode(errors='replace'),helper_errno='unavailable unless exception supplies it')
 os.close(fd)
 socket_path.unlink();socket_dir.rmdir()
text=json.dumps(report,indent=2)+'\n'
(root/'result.json').write_text(text)
print(text,end='')
