#!/usr/bin/env python3
"""Stream rows of a SQLite query into a PG table with COPY (text format, bytea as hex).

usage: pg_load.py SQLITE_DB "SELECT ..." PG_TABLE
Writes COPY text to stdout; pipe into: docker exec -i lfs420-format-pg psql -U postgres -d fmt -c "COPY t FROM STDIN"
"""
import sqlite3
import sys

db, sql, table = sys.argv[1], sys.argv[2], sys.argv[3]
con = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
w = sys.stdout.write
for row in con.execute(sql):
    out = []
    for v in row:
        if v is None:
            out.append("\\N")
        elif isinstance(v, bytes):
            out.append("\\\\x" + v.hex())
        elif isinstance(v, str):
            out.append(v.replace("\\", "\\\\").replace("\t", "\\t").replace("\n", "\\n").replace("\r", "\\r"))
        else:
            out.append(str(v))
    w("\t".join(out) + "\n")
