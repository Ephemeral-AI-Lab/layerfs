#!/usr/bin/env python3
"""Read-only semantic inventory, root and physical comparison of two arms."""
import hashlib, json, sqlite3
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent


def roots(run):
    found = {}
    for line in (run / "trace-perf.jsonl").read_text().splitlines():
        row = json.loads(line)
        key = row.get("key", "")
        if row["kind"] == "counter" and key.startswith("history.state.") and key.endswith(".root"):
            found[int(key.split(".")[2])] = str(row["value"])
    return found


def main():
    result = {}
    for case in ("history-stride10", "history-stride3"):
        arms = {}
        for arm in ("baseline2", "candidate2"):
            run = CAMPAIGN / "runs" / f"{arm}-{case}"
            if not (run / "verify-receipt.json").exists():
                continue
            path = run / "raw/sample.sqlite"
            db = sqlite3.connect(path.as_uri() + "?mode=ro", uri=True)
            rows = list(db.execute(
                "SELECT hex(object_id), object_role, canonical_length FROM objects ORDER BY object_id"))
            serial = json.dumps(rows, separators=(",", ":")).encode()
            groups = list(db.execute(
                "SELECT first_ordinal, count, pack_id, group_number, hex(digest) "
                "FROM metadata_value_groups ORDER BY first_ordinal"))
            arms[arm] = {
                "state_roots": roots(run),
                "canonical_object_count": len(rows),
                "canonical_bytes": sum(r[2] for r in rows),
                "canonical_inventory_sha256": hashlib.sha256(serial).hexdigest(),
                "value_groups": len(groups),
                "value_group_inventory_sha256": hashlib.sha256(
                    json.dumps(groups, separators=(",", ":")).encode()).hexdigest(),
                "packs": db.execute("SELECT count(*), sum(length(data)) FROM object_packs").fetchone(),
                "quick_check": db.execute("PRAGMA quick_check").fetchall(),
                "store_bytes": path.stat().st_size,
                "store_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            }
            db.close()
        if len(arms) == 2:
            left, right = arms["baseline2"], arms["candidate2"]
            result[case] = {
                "arms": arms,
                "state_roots_match": left["state_roots"] == right["state_roots"],
                "states_compared": len(left["state_roots"]),
                "canonical_inventory_match": left["canonical_inventory_sha256"]
                == right["canonical_inventory_sha256"],
                "value_group_inventory_match": left["value_group_inventory_sha256"]
                == right["value_group_inventory_sha256"],
                "store_byte_identical": left["store_sha256"] == right["store_sha256"],
            }
    out = CAMPAIGN / "store-comparison-v2.json"
    assert not out.exists(), "refusing to overwrite the retained comparison"
    # The empty `store-comparison.json` is a retained first attempt whose arm name
    # was wrong; it is not overwritten and holds no comparison.
    out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({case: {k: v for k, v in body.items() if k != "arms"}
                      for case, body in result.items()}, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
