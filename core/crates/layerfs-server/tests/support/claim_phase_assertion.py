#!/usr/bin/env python3
"""External assertions on the exact SQLite library selected by the Server test.

This is a direct-library correctness proof, not an executable hardcap proof.
The install mode changes only the owned test scratch schema at an acknowledged
prepared-body EOF: a fixed root column requires site retirement stage4.
Foreign-key checking uses no trigger program; SQLITE_LIMIT_TRIGGER_DEPTH stays0.
The owning provider separately proves stage4 requires exact empty sites/indexes.
No provider substitution, retry, sleep, product hook, or performance measurement.
"""

import ctypes
import os
import sys


library, mode, database = sys.argv[1:4]
assert mode in ("number", "install")
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
sqlite.sqlite3_limit.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
sqlite.sqlite3_get_autocommit.argtypes = [ctypes.c_void_p]
sqlite.sqlite3_close.argtypes = [ctypes.c_void_p]
sqlite.sqlite3_libversion.restype = ctypes.c_char_p
sqlite.sqlite3_sourceid.restype = ctypes.c_char_p
access = 1 if mode == "number" else 2
opened = sqlite.sqlite3_open_v2(os.fsencode(database), ctypes.byref(db), access | 0x01000000, None)
assert opened == 0, (opened, database, library)
assert sqlite.sqlite3_extended_result_codes(db, 1) == 0
assert sqlite.sqlite3_busy_timeout(db, 0) == 0
callback_type = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_int,
                                ctypes.POINTER(ctypes.c_char_p), ctypes.POINTER(ctypes.c_char_p))


def statement(sql, expected=0):
    rows = []

    @callback_type
    def row(_context, count, values, _names):
        rows.append([None if values[i] is None else values[i].decode() for i in range(count)])
        return 0

    result = sqlite.sqlite3_exec(db, sql.encode(), row, None, None)
    assert result == expected, (sql, result, expected, sqlite.sqlite3_errmsg(db).decode())
    return rows


if mode == "number":
    assert len(sys.argv) == 5
    result = statement(sys.argv[4])
    assert len(result) == 1 and len(result[0]) == 1
    print(int(result[0][0]))
else:
    assert len(sys.argv) == 4
    assert statement("PRAGMA application_id") == [["1279673171"]]
    assert statement("PRAGMA user_version") == [["3"]]
    assert statement("SELECT hex(substr(header,1,8)),length(header),records,sealed FROM session_owner WHERE id=1") == [
        ["4C4643534F574E33", "200", "0", "0"]
    ]
    assert statement("SELECT length(scope),stage,records,remaining FROM site_owner WHERE id=1") == [["89", "0", "0", "0"]]
    assert len(statement("PRAGMA table_info(session_owner)")) == 9
    assert len(statement("PRAGMA table_info(site_owner)")) == 10
    assert len(statement("PRAGMA table_info(directory_roots)")) == 3
    assert statement("SELECT count(*) FROM binding_sites") == [["0"]]
    assert statement("SELECT count(*) FROM directory_roots") == [["0"]]
    # These settings belong only to this acknowledged external test connection.
    # The production connection remains foreign_keys1/triggerdepth0 unchanged.
    assert statement("PRAGMA journal_mode=MEMORY") == [["memory"]]
    statement("PRAGMA synchronous=OFF")
    statement("PRAGMA foreign_keys=OFF")
    statement("BEGIN IMMEDIATE")
    statement("CREATE UNIQUE INDEX test_required_site_phase ON site_owner(stage)")
    statement("ALTER TABLE directory_roots ADD COLUMN required_site_stage INTEGER NOT NULL DEFAULT 4 REFERENCES site_owner(stage)")
    statement("COMMIT")
    statement("PRAGMA foreign_keys=ON")
    assert statement("PRAGMA foreign_keys") == [["1"]]
    sqlite.sqlite3_limit(db, 10, 0)
    assert sqlite.sqlite3_limit(db, 10, -1) == 0
    assert len(statement("PRAGMA table_info(session_owner)")) == 9
    assert len(statement("PRAGMA table_info(directory_roots)")) == 4
    # The assertion has a negative control on the actual selected engine. This
    # otherwise width-valid row must fail while the owner is still phase0.
    statement("INSERT INTO directory_roots(key,ordinal,root) VALUES(zeroblob(25),1,zeroblob(32))", 787)
    assert sqlite.sqlite3_get_autocommit(db) == 1
    assert statement("SELECT count(*) FROM directory_roots") == [["0"]]
    assert statement("SELECT stage FROM site_owner WHERE id=1") == [["0"]]
    print("installed stage4 assertion; owner_columns9 site_columns10 root_columns4 trigger_depth0 foreign_keys1 pre_phase0 constraint787")

assert sqlite.sqlite3_close(db) == 0
print("selected SQLite: library={} version={} source={}".format(
    library, sqlite.sqlite3_libversion().decode(), sqlite.sqlite3_sourceid().decode()), file=sys.stderr)
