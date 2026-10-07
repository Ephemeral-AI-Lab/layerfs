import ctypes as c, tempfile, pathlib
lib=c.CDLL('/usr/lib/libsqlite3.dylib')
lib.sqlite3_open_v2.argtypes=[c.c_char_p,c.POINTER(c.c_void_p),c.c_int,c.c_char_p]
lib.sqlite3_exec.argtypes=[c.c_void_p,c.c_char_p,c.c_void_p,c.c_void_p,c.c_void_p]
lib.sqlite3_file_control.argtypes=[c.c_void_p,c.c_char_p,c.c_int,c.c_void_p]
lib.sqlite3_close.argtypes=[c.c_void_p]
lib.sqlite3_errmsg.argtypes=[c.c_void_p];lib.sqlite3_errmsg.restype=c.c_char_p
lib.sqlite3_extended_errcode.argtypes=[c.c_void_p]
with tempfile.TemporaryDirectory(prefix='layerfs-readonly-cause-') as td:
    target=pathlib.Path(td)/'store.sqlite'; db=c.c_void_p()
    assert lib.sqlite3_open_v2(str(target).encode(),c.byref(db),2|4|0x8000,None)==0
    for sql in [b'PRAGMA journal_mode=wal',b'PRAGMA synchronous=OFF',b'CREATE TABLE item(x)',b'INSERT INTO item VALUES(1)',b'PRAGMA wal_checkpoint(TRUNCATE)']:
        assert lib.sqlite3_exec(db,sql,None,None,None)==0
    persist=c.c_int(0); assert lib.sqlite3_file_control(db,b'main',10,c.byref(persist))==0
    assert lib.sqlite3_close(db)==0
    print('sealed',[(p.name,p.stat().st_size) for p in pathlib.Path(td).iterdir()])
    db=c.c_void_p(); print('readonly_open',lib.sqlite3_open_v2(str(target).encode(),c.byref(db),1|0x8000,None))
    for sql in [b'PRAGMA journal_mode',b'SELECT * FROM item']:
        rc=lib.sqlite3_exec(db,sql,None,None,None)
        print(sql.decode(),'rc',rc,'extended',lib.sqlite3_extended_errcode(db),'message',lib.sqlite3_errmsg(db).decode())
    print('close',lib.sqlite3_close(db))
