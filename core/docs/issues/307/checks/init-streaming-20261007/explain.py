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


shapes = {'jobs': [1,-1,512], 'job_sizes': [1,-1,512], 'job': [1,1,1]}
plans = [record(name, 'native-position-range-or-owned-point-with-alias-count', values) for name,values in shapes.items()]
for plan in plans:
    details = [row['detail'] for row in plan['query_plan']]
    assert not any('SCAN' in line or 'TEMP B-TREE' in line for line in details), details
    if plan['name']=='job':
        assert details[0]=='SEARCH owner USING INTEGER PRIMARY KEY (rowid=?)',details

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
print(json.dumps({"engine": result["engine"], "plan_shapes": len(plans), "changed_queries": len(shapes), "status": "PASS"}))
