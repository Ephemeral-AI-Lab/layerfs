import ctypes as c, tempfile, pathlib
lib=c.CDLL('/usr/lib/libsqlite3.dylib')
lib.sqlite3_libversion.restype=c.c_char_p
lib.sqlite3_open_v2.argtypes=[c.c_char_p,c.POINTER(c.c_void_p),c.c_int,c.c_char_p]
lib.sqlite3_exec.argtypes=[c.c_void_p,c.c_char_p,c.c_void_p,c.c_void_p,c.c_void_p]
lib.sqlite3_file_control.argtypes=[c.c_void_p,c.c_char_p,c.c_int,c.c_void_p]
lib.sqlite3_close.argtypes=[c.c_void_p]
print('system_sqlite',lib.sqlite3_libversion().decode())
with tempfile.TemporaryDirectory(prefix='layerfs-seal-cause-') as td:
    db=c.c_void_p(); target=pathlib.Path(td)/'store.sqlite'
    assert lib.sqlite3_open_v2(str(target).encode(),c.byref(db),2|4|0x8000,None)==0
    persistent=c.c_int(-1)
    print('persist_wal_rc',lib.sqlite3_file_control(db,b'main',10,c.byref(persistent)),'value',persistent.value)
    for sql in [b'PRAGMA journal_mode=wal',b'PRAGMA synchronous=OFF',b'CREATE TABLE item(x)',b'INSERT INTO item VALUES(1)',b'PRAGMA wal_checkpoint(TRUNCATE)']:
        print(sql.decode(),'rc',lib.sqlite3_exec(db,sql,None,None,None))
    print('before_close',[(p.name,p.stat().st_size) for p in pathlib.Path(td).iterdir()])
    print('close_rc',lib.sqlite3_close(db))
    print('after_close',[(p.name,p.stat().st_size) for p in pathlib.Path(td).iterdir()])
