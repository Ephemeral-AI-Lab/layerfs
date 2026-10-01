import os,stat,hashlib,sqlite3,json,time,shutil,ctypes
from pathlib import Path
source=Path('/Users/yifanxu/Ephemeral-AI-Lab/deepseek-harness')
root=Path('benchmark-results/storage-probes/deepseek-full-master-v2');root.mkdir(exist_ok=False)
tree=root/'tree';tree.mkdir()
db=sqlite3.connect(root/'manifest.sqlite');db.execute('PRAGMA journal_mode=MEMORY');db.execute('PRAGMA synchronous=OFF');db.executescript("CREATE TABLE entries(path BLOB PRIMARY KEY,kind TEXT,mode INTEGER,mtime INTEGER,size INTEGER,dev INTEGER,ino INTEGER,uid INTEGER,gid INTEGER,flags INTEGER,link BLOB,sha256 TEXT); CREATE TABLE xattrs(path BLOB,name BLOB,value BLOB,PRIMARY KEY(path,name));")
started=time.monotonic_ns();counts={'files':0,'directories':0,'symlinks':0,'bytes':0,'hardlink_aliases':0,'xattrs':0};links={}
libc=ctypes.CDLL(None,use_errno=True)
libc.listxattr.argtypes=[ctypes.c_char_p,ctypes.c_void_p,ctypes.c_size_t,ctypes.c_int];libc.listxattr.restype=ctypes.c_ssize_t
libc.getxattr.argtypes=[ctypes.c_char_p,ctypes.c_char_p,ctypes.c_void_p,ctypes.c_size_t,ctypes.c_uint32,ctypes.c_int];libc.getxattr.restype=ctypes.c_ssize_t
def attrs(path):
 raw=os.fsencode(path);length=libc.listxattr(raw,None,0,1)
 if length<0:raise OSError(ctypes.get_errno(),'native listxattr failed')
 if length==0:return []
 buffer=ctypes.create_string_buffer(length)
 actual=libc.listxattr(raw,buffer,length,1)
 if actual!=length:raise OSError(ctypes.get_errno(),'source xattr list changed')
 result=[]
 for name in buffer.raw[:actual].split(b'\0'):
  if not name:continue
  size=libc.getxattr(raw,name,None,0,0,1)
  if size<0:raise OSError(ctypes.get_errno(),'native getxattr failed')
  value=ctypes.create_string_buffer(max(1,size));got=libc.getxattr(raw,name,value,size,0,1)
  if got!=size:raise OSError(ctypes.get_errno(),'source xattr value changed')
  result.append((name,value.raw[:size]))
 return result
def capture(path,target):
 before=path.lstat();relative=os.fsencode(str(path.relative_to(source)))
 if stat.S_ISDIR(before.st_mode):
  target.mkdir(exist_ok=True);kind='directory';size=0;link=None;digest=None;counts['directories']+=1
 elif stat.S_ISLNK(before.st_mode):
  link=os.readlink(os.fsencode(path));os.symlink(link,os.fsencode(target));kind='symlink';size=before.st_size;digest=None;counts['symlinks']+=1
 elif stat.S_ISREG(before.st_mode):
  kind='file';size=before.st_size;link=None;key=(before.st_dev,before.st_ino)
  if before.st_nlink>1 and key in links:
   prior,digest=links[key];os.link(prior,target);counts['hardlink_aliases']+=1
  else:
   hasher=hashlib.sha256()
   fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW)
   with os.fdopen(fd,'rb') as inp,target.open('xb') as out:
    while data:=inp.read(65536):out.write(data);hasher.update(data)
    after=os.fstat(inp.fileno())
    assert (before.st_dev,before.st_ino,before.st_size,before.st_mtime_ns)==(after.st_dev,after.st_ino,after.st_size,after.st_mtime_ns),f'source changed during acquisition: {relative!r}'
   digest=hasher.hexdigest()
   if before.st_nlink>1:links[key]=(target,digest)
  counts['files']+=1;counts['bytes']+=size
 else:raise RuntimeError(f'unsupported live filesystem entry: {relative!r}')
 db.execute('INSERT INTO entries VALUES(?,?,?,?,?,?,?,?,?,?,?,?)',(relative,kind,stat.S_IMODE(before.st_mode),before.st_mtime_ns,size,before.st_dev,before.st_ino,before.st_uid,before.st_gid,getattr(before,'st_flags',0),link,digest))
 for name,value in attrs(path):
  db.execute('INSERT INTO xattrs VALUES(?,?,?)',(relative,name,value));counts['xattrs']+=1
 if kind=='file':os.chmod(target,stat.S_IMODE(before.st_mode));os.utime(target,ns=(before.st_atime_ns,before.st_mtime_ns))
 if sum(counts[k] for k in ['files','directories','symlinks'])%128==0:db.commit()
capture(source,tree)
for parent,dirs,files in os.walk(source,followlinks=False):
 dirs.sort();files.sort()
 for name in list(dirs):
  path=Path(parent)/name;target=tree/path.relative_to(source);capture(path,target)
  if path.is_symlink():dirs.remove(name)
 for name in files:
  path=Path(parent)/name;capture(path,tree/path.relative_to(source))
for rel,mode,mtime in db.execute("SELECT path,mode,mtime FROM entries WHERE kind='directory' ORDER BY length(path) DESC"):
 target=tree/os.fsdecode(rel);os.chmod(target,mode);os.utime(target,ns=(mtime,mtime))
db.commit();assert db.execute('PRAGMA integrity_check').fetchone()==('ok',)
transcript=hashlib.sha256()
for row in db.execute('SELECT path,kind,mode,mtime,size,dev,ino,uid,gid,flags,link,sha256 FROM entries ORDER BY path'):
 data=json.dumps([x.hex() if isinstance(x,bytes) else x for x in row],separators=(',',':')).encode();transcript.update(len(data).to_bytes(8,'big'));transcript.update(data)
for row in db.execute('SELECT path,name,value FROM xattrs ORDER BY path,name'):
 data=b''.join(len(x).to_bytes(8,'big')+x for x in row);transcript.update(data)
db.close();seal={'source':str(source),'scope':'all paths including .git and ignored/generated entries; no symlink following','counts':counts,'manifest_sha256':hashlib.sha256((root/'manifest.sqlite').read_bytes()).hexdigest(),'transcript_sha256':transcript.hexdigest(),'wall_ns':time.monotonic_ns()-started,'copy':'independent byte copies; internal hardlink aliases preserved; symlink targets copied','capture_scope':'stable per-file bytes checked during read; no atomic multi-file live-repository snapshot claim; metadata/xattrs recorded in closed manifest'}
(root/'seal.json').write_text(json.dumps(seal,indent=2)+'\n');print(seal,flush=True)
