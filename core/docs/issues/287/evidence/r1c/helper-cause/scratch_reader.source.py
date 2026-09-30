#!/usr/bin/env python3
"""One real selected-library SQLite lock owner, with finite acknowledged control.

This external correctness helper loads the exact linked library path supplied by
the owned Server test executable. It creates/adopts no database and switches no
provider. Mode read holds BEGIN+SELECT on LFCS; mode write holds BEGIN IMMEDIATE
on the already-owned Store. No timer, retry, fallback, fault hook or benchmark.
"""

import ctypes
import json
import os
import socket
import struct
import sys


library, mode, database, socket_path = sys.argv[1:]
assert mode in ("read", "write")
control = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
control.settimeout(5)
control.connect(socket_path)
sqlite = ctypes.CDLL(library)
db = ctypes.c_void_p()
sqlite.sqlite3_open_v2.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p), ctypes.c_int, ctypes.c_char_p]
sqlite.sqlite3_open_v2.restype = ctypes.c_int
sqlite.sqlite3_exec.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p]
sqlite.sqlite3_exec.restype = ctypes.c_int
sqlite.sqlite3_extended_result_codes.argtypes = [ctypes.c_void_p, ctypes.c_int]
sqlite.sqlite3_busy_timeout.argtypes = [ctypes.c_void_p, ctypes.c_int]
sqlite.sqlite3_errmsg.argtypes = [ctypes.c_void_p]
sqlite.sqlite3_errmsg.restype = ctypes.c_char_p
sqlite.sqlite3_close.argtypes = [ctypes.c_void_p]
sqlite.sqlite3_close.restype = ctypes.c_int
sqlite.sqlite3_libversion.restype = ctypes.c_char_p
sqlite.sqlite3_sourceid.restype = ctypes.c_char_p
access = 1 if mode == "read" else 2
assert sqlite.sqlite3_open_v2(os.fsencode(database), ctypes.byref(db), access | 0x01000000, None) == 0
assert sqlite.sqlite3_extended_result_codes(db, 1) == 0
assert sqlite.sqlite3_busy_timeout(db, 0) == 0
callback_type = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_int,
                                ctypes.POINTER(ctypes.c_char_p), ctypes.POINTER(ctypes.c_char_p))


def execute(sql):
    rows = []

    @callback_type
    def row(_context, count, values, _names):
        rows.append([None if values[i] is None else values[i].decode() for i in range(count)])
        return 0

    result = sqlite.sqlite3_exec(db, sql.encode(), row, None, None)
    assert result == 0, (sql, result, sqlite.sqlite3_errmsg(db).decode())
    return rows


if mode == "read":
    execute("BEGIN")
    assert execute("SELECT records,sealed FROM session_owner WHERE id=1") == [["0", "0"]]
    barrier = "BEGIN+SELECT records=0/sealed=0, SHARED held"
else:
    execute("BEGIN IMMEDIATE")
    assert execute("SELECT count(*) FROM saves WHERE active_slot IS NOT NULL") == [["1"]]
    barrier = "BEGIN IMMEDIATE+SELECT activeSave=1, RESERVED held"
info = {"library": library, "version": sqlite.sqlite3_libversion().decode(),
        "source": sqlite.sqlite3_sourceid().decode(), "mode": mode, "barrier": barrier,
        "access": "READ_ONLY|NOFOLLOW" if mode == "read" else "READ_WRITE|NOFOLLOW", "busy_ms": 0}
encoded = json.dumps(info, sort_keys=True).encode()
assert len(encoded) <= 8192
control.sendall(b"\x01" + struct.pack(">I", len(encoded)) + encoded)
assert control.recv(1) == b"\x02"
execute("ROLLBACK")
assert sqlite.sqlite3_close(db) == 0
control.sendall(b"\x03")
control.close()
print(json.dumps({"completion": "ROLLBACK acknowledged, exact selected connection closed", **info}, sort_keys=True))
