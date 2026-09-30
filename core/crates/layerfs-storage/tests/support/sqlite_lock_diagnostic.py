#!/usr/bin/env python3
"""One bounded native-lock cause diagnostic; no product/performance admission.

Uses the same Darwin system library as the captured Rust executable. Each of
three prospective topology cases gets its own fresh file and one attempted
writer transaction. The native file descriptor remains open through SQL work.
All results, including failures, are emitted; no retries or provider fallback.
"""

import ctypes
import json
import os
from pathlib import Path
import subprocess
import sys


LIBRARY = "/usr/lib/libsqlite3.dylib"
DB = ctypes.c_void_p
SQLITE = ctypes.CDLL(LIBRARY)
SQLITE.sqlite3_open_v2.argtypes = [ctypes.c_char_p, ctypes.POINTER(DB), ctypes.c_int, ctypes.c_char_p]
SQLITE.sqlite3_open_v2.restype = ctypes.c_int
SQLITE.sqlite3_exec.argtypes = [DB, ctypes.c_char_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p]
SQLITE.sqlite3_exec.restype = ctypes.c_int
SQLITE.sqlite3_extended_result_codes.argtypes = [DB, ctypes.c_int]
SQLITE.sqlite3_busy_timeout.argtypes = [DB, ctypes.c_int]
SQLITE.sqlite3_system_errno.argtypes = [DB]
SQLITE.sqlite3_system_errno.restype = ctypes.c_int
SQLITE.sqlite3_errmsg.argtypes = [DB]
SQLITE.sqlite3_errmsg.restype = ctypes.c_char_p
SQLITE.sqlite3_close.argtypes = [DB]
SQLITE.sqlite3_close.restype = ctypes.c_int
SQLITE.sqlite3_libversion.restype = ctypes.c_char_p
SQLITE.sqlite3_sourceid.restype = ctypes.c_char_p


def open_db(path, readonly=False):
    db = DB()
    # Match actual access/no-follow flags. The fixture already owns this file.
    result = SQLITE.sqlite3_open_v2(os.fsencode(path), ctypes.byref(db), (1 if readonly else 2) | 0x01000000, None)
    assert result == 0, result
    assert SQLITE.sqlite3_extended_result_codes(db, 1) == 0
    assert SQLITE.sqlite3_busy_timeout(db, 0) == 0
    return db


def execute(db, sql):
    result = SQLITE.sqlite3_exec(db, sql.encode(), None, None, None)
    return {"statement": sql, "result": result, "system_errno": SQLITE.sqlite3_system_errno(db),
            "message": SQLITE.sqlite3_errmsg(db).decode()}


def required(db, sql):
    observed = execute(db, sql)
    assert observed["result"] == 0, observed


def close(db):
    result = SQLITE.sqlite3_close(db)
    assert result == 0, result


if len(sys.argv) > 1 and sys.argv[1] == "--reader":
    reader = open_db(sys.argv[2], readonly=True)
    required(reader, "BEGIN")
    required(reader, "SELECT records FROM session_owner WHERE id=1")
    print("READ_LOCKED", flush=True)
    assert sys.stdin.readline() == "RELEASE\n"
    required(reader, "ROLLBACK")
    close(reader)
    print("READ_RELEASED", flush=True)
    sys.exit(0)


output = Path(sys.argv[1])
output.mkdir(exist_ok=False)
schema = Path(__file__).resolve().parents[2] / "sql" / "construction_scratch.sql"
report = {"kind": "bounded cause diagnostic; no speed/resource/Unknown admission",
          "library": LIBRARY, "version": SQLITE.sqlite3_libversion().decode(),
          "source_id": SQLITE.sqlite3_sourceid().decode(), "cases": []}
for topology in ["same_process_readonly", "same_process_readwrite", "separate_process_readonly"]:
    path = output / (topology + ".sqlite")
    native = os.open(path, os.O_RDWR | os.O_CREAT | os.O_EXCL, 0o600)
    writer = open_db(path)
    required(writer, "PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF; PRAGMA temp_store=MEMORY; PRAGMA foreign_keys=ON; PRAGMA cache_size=-512; PRAGMA mmap_size=0;")
    required(writer, schema.read_text())
    required(writer, "INSERT INTO session_owner VALUES(1,zeroblob(192),NULL,0,0,0,NULL)")
    child = None
    reader = None
    if topology == "separate_process_readonly":
        child = subprocess.Popen([sys.executable, __file__, "--reader", str(path)],
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        assert child.stdout.readline() == "READ_LOCKED\n"
    else:
        reader = open_db(path, readonly=topology == "same_process_readonly")
        required(reader, "BEGIN")
        required(reader, "SELECT records FROM session_owner WHERE id=1")
    phases = []
    for sql in ["SELECT header FROM session_owner WHERE id=1", "BEGIN IMMEDIATE",
                "INSERT INTO directory_roots VALUES(zeroblob(25),1,zeroblob(32))", "COMMIT"]:
        observed = execute(writer, sql)
        phases.append(observed)
        if observed["result"] != 0:
            break
    # Explicit diagnostic-fixture teardown, never product Unknown cleanup.
    if child:
        child.stdin.write("RELEASE\n")
        child.stdin.flush()
        assert child.stdout.readline() == "READ_RELEASED\n"
        assert child.wait(timeout=5) == 0
    else:
        required(reader, "ROLLBACK")
        close(reader)
    rollback = execute(writer, "ROLLBACK")
    assert rollback["result"] == 0 or rollback["message"] == "cannot rollback - no transaction is active", rollback
    close(writer)
    os.close(native)
    report["cases"].append({"topology": topology, "native_fd_held_during_sql": True,
                            "writer_phases": phases, "fixture_rollback": rollback})
serialized = json.dumps(report, indent=2) + "\n"
(output / "result.json").write_text(serialized)
print(serialized, end="")
