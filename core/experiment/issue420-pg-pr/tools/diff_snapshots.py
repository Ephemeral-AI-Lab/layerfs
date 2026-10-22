#!/usr/bin/env python3
"""Row-level diff of two Store snapshots: added, removed and changed rows per table.

    python3 -I diff_snapshots.py BEFORE.sqlite AFTER.sqlite [--json OUT]

Rows are keyed by the table's primary key (rowid for rowid tables); blobs are
summarized by length. Read-only (immutable URI)."""
import json, sqlite3, sys


def open_ro(path):
    return sqlite3.connect(f"file:{path}?mode=ro&immutable=1", uri=True)


def key_columns(db, table):
    cols = db.execute(f'PRAGMA table_info("{table}")').fetchall()
    pk = [c[1] for c in sorted(cols, key=lambda c: c[5]) if c[5] > 0]
    without_rowid = "WITHOUT ROWID" in (db.execute("SELECT sql FROM sqlite_master WHERE name=?", (table,)).fetchone()[0] or "").upper()
    return [c[1] for c in cols], (pk if pk else (["rowid"] if not without_rowid else [c[1] for c in cols]))


def rows(db, table, columns, key):
    select = ", ".join(f'"{c}"' if c != "rowid" else "rowid" for c in key + columns)
    out = {}
    for r in db.execute(f'SELECT {select} FROM "{table}"'):
        out[tuple(r[: len(key)])] = r[len(key):]
    return out


def show(v):
    return f"<{len(v)} B>" if isinstance(v, bytes) and len(v) > 40 else (v.hex() if isinstance(v, bytes) else v)


def diff(before, after):
    a, b = open_ro(before), open_ro(after)
    tables = [t for (t,) in b.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
    report = {}
    for t in tables:
        columns, key = key_columns(b, t)
        old, new = rows(a, t, columns, key), rows(b, t, columns, key)
        added = [k for k in new if k not in old]
        removed = [k for k in old if k not in new]
        changed = [k for k in new if k in old and new[k] != old[k]]
        if added or removed or changed:
            report[t] = {
                "key": key, "columns": columns,
                "added": len(added), "removed": len(removed), "changed": len(changed),
                "added_bytes": sum(sum(len(x) if isinstance(x, (bytes, str)) else 8 for x in new[k]) for k in added),
                "changes": [{"key": [show(x) for x in k], "from": [show(x) for x in old[k]], "to": [show(x) for x in new[k]]} for k in changed][:20],
            }
    return report


if __name__ == "__main__":
    result = diff(sys.argv[1], sys.argv[2])
    text = json.dumps(result, indent=1, default=str)
    if "--json" in sys.argv:
        open(sys.argv[sys.argv.index("--json") + 1], "w").write(text + "\n")
    for t, r in result.items():
        print(f"{t}: +{r['added']} -{r['removed']} ~{r['changed']} ({r['added_bytes']} B added)")
        for c in r["changes"][:6]:
            print("   ~", c["key"], c["from"], "->", c["to"])
