#!/usr/bin/env python3
"""Read-only fixed profile8 image using the exact library selected by this test."""
import ctypes
import os
import sys

library, database = sys.argv[1:]
sqlite = ctypes.CDLL(library)
db = ctypes.c_void_p()
sqlite.sqlite3_open_v2.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p), ctypes.c_int, ctypes.c_char_p]
sqlite.sqlite3_open_v2.restype = ctypes.c_int
sqlite.sqlite3_exec.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p]
sqlite.sqlite3_exec.restype = ctypes.c_int
sqlite.sqlite3_errmsg.argtypes = [ctypes.c_void_p]
sqlite.sqlite3_errmsg.restype = ctypes.c_char_p
sqlite.sqlite3_busy_timeout.argtypes = [ctypes.c_void_p, ctypes.c_int]
sqlite.sqlite3_close.argtypes = [ctypes.c_void_p]
sqlite.sqlite3_extended_errcode.argtypes=[ctypes.c_void_p]
sqlite.sqlite3_system_errno.argtypes=[ctypes.c_void_p]
opened=sqlite.sqlite3_open_v2(os.fsencode(database),ctypes.byref(db),1|0x01000000,None)
if opened!=0:
    raise RuntimeError({"operation":"sqlite3_open_v2","result":opened,"extended_error":sqlite.sqlite3_extended_errcode(db),"system_errno":sqlite.sqlite3_system_errno(db),"message":sqlite.sqlite3_errmsg(db).decode(),"path":database,"library":library,"flags":"READ_ONLY|NOFOLLOW"})
assert sqlite.sqlite3_busy_timeout(db, 0) == 0
callback_type = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_int, ctypes.POINTER(ctypes.c_char_p), ctypes.POINTER(ctypes.c_char_p))
rows = []
@callback_type
def collect(_context, count, values, _names):
    rows.append(["NULL" if values[i] is None else values[i].decode() for i in range(count)])
    return 0
sql = """SELECT hex(header),hex((SELECT scope FROM site_owner WHERE id=1)),
 hex((SELECT scope FROM graph_owner WHERE id=1)),hex((SELECT scope FROM solver_owner WHERE id=1)),
 declared_roots,declared_sites,selected_bytes,sealed,records,
 (SELECT stage FROM site_owner),(SELECT stage FROM graph_owner),
 (SELECT stage FROM alias_owner),(SELECT SUM(stage) FROM fact_owner),
 (SELECT stage FROM count_owner),(SELECT stage FROM release_owner)
 FROM session_owner WHERE id=1"""
result = sqlite.sqlite3_exec(db, sql.encode(), collect, None, None)
assert result == 0, (result, sqlite.sqlite3_errmsg(db).decode(), database, library)
assert len(rows) == 1 and len(rows[0]) == 15
print("\n".join(rows[0]))
rows.clear()
empty = "+".join("(SELECT COUNT(*) FROM " + table + ")" for table in [
    "binding_sites", "directory_roots", "graph_nodes", "graph_edges", "alias_facts", "alias_jobs",
    "base_facts", "parent_eligibility", "canonical_counts", "zero_seeds", "release_jobs", "release_frames"])
fixed = "+".join("(SELECT COUNT(*) FROM " + table + ")" for table in [
    "session_owner", "site_owner", "graph_owner", "solver_owner", "alias_owner", "fact_owner", "count_owner", "release_owner"])
exact = """(SELECT COUNT(*) FROM session_owner WHERE id=1 AND length(header)=386 AND scope IS NULL AND sealed=0 AND records=0 AND record_bytes=0 AND digest IS NULL)+
(SELECT COUNT(*) FROM site_owner WHERE id=1 AND length(scope)=89 AND stage=0 AND records=0 AND remaining=0 AND birth_digest IS NULL AND birth_max IS NULL AND final_digest IS NULL AND final_max IS NULL AND after_key IS NULL)+
(SELECT COUNT(*) FROM graph_owner WHERE id=1 AND length(scope)=188 AND stage=0 AND nodes=0 AND edges=0 AND record_bytes=0 AND source_multiplicity=zeroblob(8) AND remaining_nodes=0 AND remaining_edges=0 AND adjacency_seal IS NULL AND proof_seal IS NULL AND max_node IS NULL AND max_edge IS NULL AND after_node IS NULL AND after_edge IS NULL AND seed_count=0)+
(SELECT COUNT(*) FROM solver_owner WHERE id=1 AND scope=(SELECT scope FROM graph_owner WHERE id=1) AND current_serial=0 AND dfs_root_serial=0 AND next_discovery=1 AND scc_root_serial=0 AND scc_boundary=0 AND cumulative_pop_count=0 AND last_stack_discovery=0 AND any_seed=0 AND singleton_self_loop=0)+
(SELECT COUNT(*) FROM alias_owner WHERE id=1 AND scope=(SELECT scope FROM site_owner WHERE id=1) AND stage=0 AND members IS NULL AND sequence=0 AND facts=0 AND jobs=0 AND expanded=0 AND current_serial IS NULL AND current_sequence IS NULL AND progress=zeroblob(264) AND remaining=0 AND after_serial IS NULL)+
(SELECT COUNT(*) FROM fact_owner WHERE table_id IN(17,18) AND scope IS NULL AND stage=0 AND records=0 AND bound=0 AND record_bytes=0 AND remaining=0 AND after_serial IS NULL AND maximum IS NULL AND digest IS NULL)+
(SELECT COUNT(*) FROM count_owner WHERE id=1 AND scope IS NULL AND stage=0 AND records=0 AND touched=0 AND maximum IS NULL AND zeros=0 AND zero_maximum IS NULL AND count_remaining=0 AND zero_remaining=0 AND after_count IS NULL AND after_zero IS NULL AND effects IS NULL AND final_seal IS NULL AND seeds IS NULL)+
(SELECT COUNT(*) FROM release_owner WHERE id=1 AND scope IS NULL AND stage=0 AND seeds IS NULL AND seeded=0 AND seed_after IS NULL AND sequence=0 AND pending=0 AND current_key IS NULL AND current_value IS NULL AND frames=0 AND completed=0 AND directories=0 AND maximum_depth=0)"""
result=sqlite.sqlite3_exec(db,("SELECT ("+empty+"),("+fixed+"),("+exact+")").encode(),collect,None,None)
assert result==0,(result,sqlite.sqlite3_errmsg(db).decode())
assert rows==[["0","9","9"]],rows
print("\n".join(rows[0]))
assert sqlite.sqlite3_close(db) == 0
