"""Capability diagnostic only: inspect the exact linked native SQLite provider.
No speed sample, memory-mode change, payload workload or prior arm rerun.
"""
import ctypes, json
from pathlib import Path
lib=ctypes.CDLL('/usr/lib/libsqlite3.dylib')
lib.sqlite3_libversion.restype=ctypes.c_char_p
lib.sqlite3_compileoption_get.argtypes=[ctypes.c_int];lib.sqlite3_compileoption_get.restype=ctypes.c_char_p
lib.sqlite3_open.argtypes=[ctypes.c_char_p,ctypes.POINTER(ctypes.c_void_p)]
lib.sqlite3_close.argtypes=[ctypes.c_void_p]
lib.sqlite3_status64.argtypes=[ctypes.c_int,ctypes.POINTER(ctypes.c_longlong),ctypes.POINTER(ctypes.c_longlong),ctypes.c_int]
lib.sqlite3_db_status.argtypes=[ctypes.c_void_p,ctypes.c_int,ctypes.POINTER(ctypes.c_int),ctypes.POINTER(ctypes.c_int),ctypes.c_int]
lib.sqlite3_prepare_v2.argtypes=[ctypes.c_void_p,ctypes.c_char_p,ctypes.c_int,ctypes.POINTER(ctypes.c_void_p),ctypes.c_void_p]
lib.sqlite3_step.argtypes=[ctypes.c_void_p];lib.sqlite3_finalize.argtypes=[ctypes.c_void_p]
options=[]
for i in range(1000):
    value=lib.sqlite3_compileoption_get(i)
    if value is None:break
    options.append(value.decode())
path=Path('benchmark-results/phase6-metadata/provider-memory-diagnostic.sqlite')
assert not path.exists()
db=ctypes.c_void_p();assert lib.sqlite3_open(str(path).encode(),ctypes.byref(db))==0
stmt=ctypes.c_void_p();assert lib.sqlite3_prepare_v2(db,b'SELECT sqlite_version()',-1,ctypes.byref(stmt),None)==0
assert lib.sqlite3_step(stmt)==100
current=ctypes.c_longlong();high=ctypes.c_longlong()
assert lib.sqlite3_status64(0,ctypes.byref(current),ctypes.byref(high),0)==0
observations={}
for name,opcode in [('cache_used',1),('statement_used',3)]:
    c=ctypes.c_int();h=ctypes.c_int()
    rc=lib.sqlite3_db_status(db,opcode,ctypes.byref(c),ctypes.byref(h),0)
    observations[name]={'return_code':rc,'current':c.value,'high':h.value}
result={'diagnostic':'provider-memory-observation-v1','library':'/usr/lib/libsqlite3.dylib','version':lib.sqlite3_libversion().decode(),'compile_options':options,'raw_global_memory_current':current.value,'raw_global_memory_high':high.value,'db_status':observations,'performance_claim':False,'purpose':'explain all-zero allocation observations without repeating a measured arm'}
assert lib.sqlite3_finalize(stmt)==0;assert lib.sqlite3_close(db)==0
Path('benchmark-results/phase6-metadata/provider-memory-diagnostic.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:v for k,v in result.items() if k!='compile_options'}))
print('Relevant compile options:',[o for o in options if 'MEM' in o])
