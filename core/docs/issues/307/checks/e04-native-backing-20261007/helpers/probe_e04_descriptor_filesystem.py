import ctypes,json,os
p="/work/core/target/cluster2-307/e04-unsupported-filesystem-proof-20261007"
libc=ctypes.CDLL(None,use_errno=True)
rows=[]
for path,flags in [(p,os.O_RDONLY|os.O_DIRECTORY),(p+"/overlay.sqlite",os.O_RDONLY),(p+"/overlay.sqlite",os.O_WRONLY)]:
 fd=os.open(path,flags)
 buf=ctypes.create_string_buffer(256)
 result=libc.fstatfs(fd,buf)
 st=os.fstat(fd)
 rows.append(dict(path=path,flags=flags,result=result,errno=ctypes.get_errno(),magic=hex(ctypes.c_long.from_buffer(buf).value),device=st.st_dev,inode=st.st_ino,size=st.st_size,allocated=st.st_blocks*512))
 os.close(fd)
print(json.dumps(rows,indent=2))
