#!/usr/bin/env python3
"""One finite acknowledged real Store SHARED owner at prepared-body EOF."""
import ctypes
import json
import os
import socket
import struct
import sys
library, database, socket_path = sys.argv[1:]
control=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM)
control.settimeout(5)
control.connect(socket_path)
sqlite=ctypes.CDLL(library)
db=ctypes.c_void_p()
sqlite.sqlite3_open_v2.argtypes=[ctypes.c_char_p,ctypes.POINTER(ctypes.c_void_p),ctypes.c_int,ctypes.c_char_p]
sqlite.sqlite3_open_v2.restype=ctypes.c_int
sqlite.sqlite3_exec.argtypes=[ctypes.c_void_p,ctypes.c_char_p,ctypes.c_void_p,ctypes.c_void_p,ctypes.c_void_p]
sqlite.sqlite3_exec.restype=ctypes.c_int
sqlite.sqlite3_errmsg.argtypes=[ctypes.c_void_p]
sqlite.sqlite3_errmsg.restype=ctypes.c_char_p
sqlite.sqlite3_busy_timeout.argtypes=[ctypes.c_void_p,ctypes.c_int]
sqlite.sqlite3_close.argtypes=[ctypes.c_void_p]
sqlite.sqlite3_extended_errcode.argtypes=[ctypes.c_void_p]
sqlite.sqlite3_system_errno.argtypes=[ctypes.c_void_p]
opened=sqlite.sqlite3_open_v2(os.fsencode(database),ctypes.byref(db),1|0x01000000,None)
if opened!=0:
    raise RuntimeError({"operation":"sqlite3_open_v2","result":opened,"extended_error":sqlite.sqlite3_extended_errcode(db),"system_errno":sqlite.sqlite3_system_errno(db),"message":sqlite.sqlite3_errmsg(db).decode(),"path":database,"library":library,"flags":"READ_ONLY|NOFOLLOW"})
assert sqlite.sqlite3_busy_timeout(db,0)==0
callback_type=ctypes.CFUNCTYPE(ctypes.c_int,ctypes.c_void_p,ctypes.c_int,ctypes.POINTER(ctypes.c_char_p),ctypes.POINTER(ctypes.c_char_p))
def query(sql):
    rows=[]
    @callback_type
    def collect(_context,count,values,_names):
        rows.append([None if values[i] is None else values[i].decode() for i in range(count)])
        return 0
    result=sqlite.sqlite3_exec(db,sql.encode(),collect,None,None)
    assert result==0,(result,sql,sqlite.sqlite3_errmsg(db).decode())
    return rows
query("BEGIN")
assert query("SELECT count(*) FROM saves WHERE active_slot IS NOT NULL")==[["1"]]
info=json.dumps({"library":library,"barrier":"real Store BEGIN+SELECT activeSave=1 SHARED held","access":"READ_ONLY|NOFOLLOW"},sort_keys=True).encode()
control.sendall(b"\x01"+struct.pack(">I",len(info))+info)
assert control.recv(1)==b"\x02"
query("ROLLBACK")
assert sqlite.sqlite3_close(db)==0
control.sendall(b"\x03")
control.close()
