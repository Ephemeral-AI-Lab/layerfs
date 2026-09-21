#!/usr/bin/env python3
"""Exact, disjoint history attribution; raw integers only, never pool samples."""
import argparse
import json
from collections import defaultdict
from pathlib import Path


def exclusive(node, prefix=""):
    """Flatten leaves plus each parent's residual without double counting."""
    name = prefix + node["name"]
    children = node.get("children", [])
    remaining = node["elapsed_ns"] - sum(c["elapsed_ns"] for c in children)
    if remaining < 0:
        raise ValueError(f"overlapping or invalid children at {name}")
    result = {name + (".residual" if children else ""): remaining}
    for child in children:
        for key, value in exclusive(child, name + "/").items():
            if key in result:
                raise ValueError(f"duplicate span {key}")
            result[key] = value
    assert sum(result.values()) == node["elapsed_ns"]
    return result


def analyze(directory):
    timing = json.loads((directory / "timing.json").read_text())
    trace = {}
    for line in (directory / "trace.jsonl").read_text().splitlines():
        row = json.loads(line)
        if not row["key"].startswith("history.state."):
            continue
        if row["key"] in trace:
            raise ValueError(f"duplicate trace key {row['key']}")
        trace[row["key"]] = row["value"]
    rows, totals = [], defaultdict(int)
    for state in timing["children"]:
        children = {c["name"]: c for c in state["children"]}
        if "filesystem" in children:
            parts = {}
            for child in state["children"]:
                parts.update(exclusive(child))
            parts["state.residual"] = state["elapsed_ns"] - sum(parts.values())
        else:
            parts = {k: children[k]["elapsed_ns"] for k in ("content", "store.create") if k in children}
            for key in ("input", "build", "save"):
                parts[key] = int(trace[state["name"] + "." + key + "_ns"])
            parts["state.residual"] = state["elapsed_ns"] - sum(parts.values())
        if any(value < 0 for value in parts.values()):
            raise ValueError(f"negative residual at {state['name']}")
        assert sum(parts.values()) == state["elapsed_ns"]
        for key, value in parts.items():
            totals[key] += value
        rows.append({"state": state["name"], "operation_ns": state["elapsed_ns"], "parts_ns": parts})
    operation = sum(row["operation_ns"] for row in rows)
    assert operation == sum(totals.values())
    phases_path = directory / "phases-perf.json"
    if phases_path.exists():
        assert json.loads(phases_path.read_text())["operation_ns"] == operation
    return {"source": str(directory), "operation_ns": operation, "root_ns": timing["elapsed_ns"],
            "root_outside_states_ns": timing["elapsed_ns"] - operation,
            "parts_ns": dict(totals), "states": rows}


def self_test():
    tree = {"name": "fs", "elapsed_ns": 100, "children": [
        {"name": "validate", "elapsed_ns": 60, "children": [
            {"name": "read", "elapsed_ns": 40, "children": []}]},
        {"name": "inodes", "elapsed_ns": 30, "children": []}]}
    assert exclusive(tree) == {"fs.residual": 10, "fs/validate.residual": 20,
                               "fs/validate/read": 40, "fs/inodes": 30}
    tree["elapsed_ns"] = 80
    try:
        exclusive(tree)
    except ValueError:
        pass
    else:
        raise AssertionError("overlapping children accepted")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", nargs="?", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("PASS: nested children counted once; impossible residual rejected")
    elif args.directory:
        print(json.dumps(analyze(args.directory), indent=2))
    else:
        parser.error("directory or --self-test is required")
