#!/usr/bin/env python3
"""#420 format study: commit + commit_lineage rows, today vs integer references.

usage: sim_commit.py SRC_DB OUT_DIR VARIANT   (base | slim)
base: fresh copy of commit (+commit_identity) and commit_lineage (+commit_runs, run_merges).
slim: one commit table with an integer row id; parent, base layer, second parent, branch,
      merged-from, layer stack and lineage run become integers; lineage folded into the row;
      commit_identity dropped (its only role is a composite FK target); commit_id kept UNIQUE.
"""
import json
import os
import sqlite3
import sys
import time


def main():
    src, outdir, variant = sys.argv[1], sys.argv[2], sys.argv[3]
    t0 = time.time()
    path = os.path.join(outdir, f"cm-{variant}.sqlite")
    if os.path.exists(path):
        os.remove(path)
    dst = sqlite3.connect(f"file:{path}", uri=True)
    for p in ("PRAGMA page_size=4096", "PRAGMA journal_mode=OFF", "PRAGMA synchronous=OFF"):
        dst.execute(p)
    dst.execute(f"ATTACH 'file:{src}?mode=ro' AS s")
    if variant == "base":
        for (sql,) in dst.execute("SELECT sql FROM s.sqlite_schema WHERE name IN ('commit','commit_identity','commit_lineage','commit_runs','run_merges') ORDER BY type DESC").fetchall():
            dst.execute(sql.replace("CREATE TABLE IF NOT EXISTS", "CREATE TABLE"))
        dst.execute('INSERT INTO main."commit" SELECT * FROM s."commit" ORDER BY commit_id')
        dst.execute("INSERT INTO main.commit_lineage SELECT * FROM s.commit_lineage ORDER BY commit_id")
    else:
        dst.execute("""CREATE TABLE ids(id BLOB PRIMARY KEY, n INTEGER NOT NULL) WITHOUT ROWID""")
        # number commits by creation order; layers, branches and the stack get their own small numbers
        dst.execute("""CREATE TEMP TABLE cn AS SELECT commit_id, row_number() OVER (ORDER BY created_ms, commit_id) n FROM s."commit" """)
        dst.execute("""CREATE TEMP TABLE ln AS SELECT layer_id, row_number() OVER (ORDER BY layer_id) n FROM s.layer""")
        dst.execute("""CREATE TEMP TABLE bn AS SELECT branch_id, row_number() OVER (ORDER BY branch_id) n FROM s.branch""")
        dst.execute("CREATE INDEX temp.cn_i ON cn(commit_id)")
        dst.execute("CREATE INDEX temp.ln_i ON ln(layer_id)")
        dst.execute("CREATE INDEX temp.bn_i ON bn(branch_id)")
        dst.execute("DROP TABLE ids")
        dst.execute("""CREATE TABLE "commit"(n INTEGER PRIMARY KEY, commit_id BLOB NOT NULL UNIQUE, stack INTEGER NOT NULL,
            root_id BLOB NOT NULL, parent INTEGER, base_layer INTEGER NOT NULL, second_parent INTEGER, branch INTEGER,
            via INTEGER NOT NULL, message TEXT, merged_from INTEGER, created_ms INTEGER NOT NULL, logical INTEGER NOT NULL,
            new_canonical INTEGER NOT NULL, new_stored INTEGER NOT NULL, new_metadata INTEGER NOT NULL, lww BLOB NOT NULL,
            displaced INTEGER, run INTEGER NOT NULL, depth INTEGER NOT NULL, newest INTEGER NOT NULL, merged INTEGER) STRICT""")
        # second parent / merged-from / merged may be commit (0x12) or layer (0x32) ids: tag the kind in the low bit
        def ref(col):
            return f"""(CASE WHEN {col} IS NULL THEN NULL WHEN substr({col},1,1)=X'12'
                        THEN (SELECT n FROM cn WHERE commit_id={col})*2 ELSE (SELECT n FROM ln WHERE layer_id={col})*2+1 END)"""
        dst.execute(f"""INSERT INTO main."commit" SELECT cn.n, c.commit_id, 1, c.root_id,
            (SELECT n FROM cn p WHERE p.commit_id=c.parent_commit_id), (SELECT n FROM ln WHERE layer_id=c.base_layer_id),
            {ref('c.second_parent_id')}, (SELECT n FROM bn WHERE branch_id=c.branch_id), c.via, c.message,
            {ref('c.merged_from_id')}, c.created_ms, c.logical, c.new_canonical, c.new_stored, c.new_metadata, c.lww,
            {ref('c.displaced_id')}, (SELECT n FROM cn r WHERE r.commit_id=l.run), l.depth, l.newest, {ref('l.merged')}
            FROM s."commit" c JOIN cn ON cn.commit_id=c.commit_id JOIN s.commit_lineage l ON l.commit_id=c.commit_id
            ORDER BY cn.n""")
        dst.execute('CREATE UNIQUE INDEX commit_runs ON "commit"(run, depth)')
        dst.execute('CREATE INDEX run_merges ON "commit"(run, depth) WHERE merged IS NOT NULL')
        nulls = dst.execute('SELECT sum(run IS NULL), sum(parent IS NULL) FROM main."commit"').fetchone()
    dst.commit()
    rows = dst.execute('SELECT count(*) FROM main."commit"').fetchone()[0]
    dst.execute("DETACH s")
    dst.execute("VACUUM")
    out = {"variant": variant, "rows": rows, "bytes": dst.execute("PRAGMA page_count").fetchone()[0] * 4096,
           "seconds": round(time.time() - t0, 1)}
    if variant == "slim":
        out["null_run_parent"] = nulls
    print(json.dumps(out))


if __name__ == "__main__":
    main()
