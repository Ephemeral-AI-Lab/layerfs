import ctypes,json,os
p="/work/core/target/cluster2-307/e04-unsupported-filesystem-proof-20261007/fresh-descriptor-cause.bin"
libc=ctypes.CDLL(None,use_errno=True)
rows=[]
fresh=os.open(p,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600)
for label,fd in [("original-create",fresh),("reopened-read",os.open(p,os.O_RDONLY)),("reopened-write",os.open(p,os.O_WRONLY))]:
 buf=ctypes.create_string_buffer(256)
 result=libc.fstatfs(fd,buf)
 st=os.fstat(fd)
 rows.append(dict(label=label,result=result,errno=ctypes.get_errno(),magic=hex(ctypes.c_long.from_buffer(buf).value),device=st.st_dev,inode=st.st_ino,size=st.st_size,allocated=st.st_blocks*512))
 os.close(fd)
print(json.dumps(rows,indent=2))
