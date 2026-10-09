"""Explicit dispositions for the removed host-mediated benchmark mechanism.

Compatible historical checkouts keep their original routes. Current checkouts
must not build removed packages or consume an old cached host-mediated binary.
"""
import json
from pathlib import Path
import tomllib

SCHEMA = "core-host-mediated-retirement-v1"
REASON = "mechanism removed"


def compatible(root):
    manifest = Path(root) / "core/crates/layerfs-server/Cargo.toml"
    if not manifest.is_file():
        return False
    # Presence only decides whether the frozen route may attempt its ordinary
    # checks/build; it does not qualify a historical source or its topology.
    document = tomllib.loads(manifest.read_text())
    return document.get("package", {}).get("name") == "layerfs-server"


def require_compatible(root):
    if not compatible(root):
        raise RuntimeError("NOT_RUN — " + REASON + "; host-mediated layerfs-server route retired")


def record(output, selection, cases, identity):
    output = Path(output)
    output.mkdir(parents=True, exist_ok=False)
    rows = []
    for name in cases:
        row = {"case": name, "status": "NOT_RUN", "sample_count": 0,
               "reason": REASON, "functional_status": "NOT_RUN",
               "performance_status": "NOT_RUN", "cleanup_status": "NOT_RUN"}
        folder = output / name
        folder.mkdir()
        (folder / "receipt.json").write_text(json.dumps(row, sort_keys=True, indent=2) + "\n")
        rows.append(row)
    summary = {"schema": SCHEMA, "selection": selection, "identity": identity,
               "status": "NOT_RUN", "reason": REASON, "sample_count": 0,
               "rows": rows, "route": "retired-host-mediated"}
    (output / "run.json").write_text(json.dumps(summary, sort_keys=True, indent=2) + "\n")
    return output


def verify(output):
    output = Path(output)
    summary = json.loads((output / "run.json").read_text())
    if (summary.get("schema") != SCHEMA or summary.get("status") != "NOT_RUN"
            or summary.get("reason") != REASON or type(summary.get("sample_count")) is not int
            or summary["sample_count"] != 0 or not summary.get("rows")):
        raise ValueError("invalid removed-mechanism disposition")
    names = set()
    for row in summary["rows"]:
        name = row.get("case")
        if (not isinstance(name, str) or name in names or row.get("status") != "NOT_RUN"
                or row.get("reason") != REASON or type(row.get("sample_count")) is not int
                or row["sample_count"] != 0
                or json.loads((output / name / "receipt.json").read_text()) != row):
            raise ValueError("removed-mechanism row/custody mismatch")
        names.add(name)
    return summary
