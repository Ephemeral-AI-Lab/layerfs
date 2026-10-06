"""Prospective Apple SQLite plans of changed queries; no mutation or timing arm."""
import hashlib
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[6]
OUT = Path(__file__).resolve().parent
QUERIES = ROOT / "core/crates/layerfs-persistence/sql/sqlite/acquisition/queries"
DATABASE = ROOT / "benchmark-results/fs-bench-pro/cluster-one-regression-retained-20261006/phase7-sqlite-init-100000-cluster-one-regression-v1/store.sqlite"
CLI = "/usr/bin/sqlite3"


def query(sql, values=()):
    setup = ".explain off\n.mode json\n.parameter init\n"
    setup += ".output /dev/null\nPRAGMA foreign_keys=ON; PRAGMA temp_store=MEMORY; PRAGMA mmap_size=0; PRAGMA cache_size=-2048;\n.output\n"
    for index, value in enumerate(values, 1):
        literal = "NULL" if value is None else (
            "X'" + value.hex() + "'" if isinstance(value, bytes) else str(value)
        )
        setup += f".parameter set ?{index} {literal}\n"
    completed = subprocess.run(
        [CLI, "-readonly", str(DATABASE)], input=setup + sql + ";\n",
        text=True, capture_output=True, timeout=10, check=True,
    )
    return json.loads(completed.stdout)


def record(name, shape, values):
    path = QUERIES / (name + ".sql")
    sql = path.read_text()
    parameters = max(map(int, re.findall(r"\?(\d+)", sql)))
    assert len(values) == parameters, (name, len(values), parameters)
    return {
        "name": name, "shape": shape, "sql_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "parameters": parameters,
        "binding_bytes": sum(len(v) if isinstance(v, bytes) else 0 if v is None else 8 for v in values),
        "query_plan": query("EXPLAIN QUERY PLAN " + sql, values),
        "program": query("EXPLAIN " + sql, values),
    }


owned = {
    "entries_first": [1, 1, 512],
    "entries_next": [1, 1, 0, b"f0000", 512],
    "bindings_first": [1, 1, 512],
    "bindings_next": [1, 1, 0, b"f0000", 512],
    "file_roots": [1, 1, -1, 512],
    "unplaced_first": [1, 1, 0, 512],
    "unplaced_next": [1, 1, 0, b"f0000", 512],
    "job": [1, 1, 1],
    "directory_path": [1, 1, 0],
}
plans = [record(name, "owned-keyset-or-point", values) for name, values in owned.items()]
for plan in plans:
    details = [row["detail"] for row in plan["query_plan"]]
    assert details[0] == "SEARCH owner USING INTEGER PRIMARY KEY (rowid=?)", details
    assert not any("SCAN" in line or "TEMP B-TREE" in line for line in details), details

for count in (32, 7):
    rows = []
    for index in range(32):
        if index < count:
            position = index + 1
            native = (1).to_bytes(8, "big") + position.to_bytes(8, "big") + bytes([7]) * 44
            rows.append((0, f"f{position:04}".encode(), position, 1, bytes([3]) * 32, None, f"/source/f{position:04}".encode(), native))
        else:
            rows.append((None,) * 8)
    dependencies, natives, entries = [1], [1], [1]
    for parent, name, position, kind, metadata, target, path, native in rows:
        dependencies.extend([parent, name, position, kind, native[:16] if native else None])
        natives.extend([position, native, path])
        entries.extend([parent, name, position, kind, metadata, target, None])
    for name, values in [("entry_dependencies", dependencies), ("put_native", natives), ("put_entries", entries)]:
        plans.append(record(name, f"regular-{count}-of-32", values))

result = {
    "scope": "Read-only EXPLAIN/EQP of exact changed SQL at dirty implementation hashes before runtime; no mutation or timing sample. Runtime profiles are separate.",
    "database": str(DATABASE),
    "parent_source": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
    "sqlite_cli": CLI,
    "engine": query("SELECT sqlite_version() AS version, sqlite_source_id() AS source_id"),
    "compile_options": query("PRAGMA compile_options"),
    "schema": query("SELECT type,name,sql FROM sqlite_schema WHERE name LIKE 'init_%' ORDER BY name"),
    "plans": plans,
    "report_template_sha256": hashlib.sha256((ROOT / "benchmark_agent_report.md").read_bytes()).hexdigest(),
}
with (OUT / "prospective-sql-plans.json").open("x") as handle:
    json.dump(result, handle, indent=2)
    handle.write("\n")
print(json.dumps({"engine": result["engine"], "plan_shapes": len(plans), "owned_queries": len(owned), "status": "PASS"}))
