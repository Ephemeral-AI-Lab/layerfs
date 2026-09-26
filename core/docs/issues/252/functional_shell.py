#!/usr/bin/env python3
"""Append-only public Exec/FUSE/Commit regressions on release binaries."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / "core/benchmark/fs-bench-pro"))
import shell_package as shell  # noqa: E402

PREPEND = """set -eu
truncate -s 10489856 large.bin
i=79
while [ "$i" -ge 0 ]; do
  dd if=large.bin of=large.bin bs=131072 skip="$i" seek="$((i*131072+4096))" count=1 oflag=seek_bytes conv=notrunc 2>/dev/null
  i=$((i-1))
done
dd if=/fixtures/v2/patch-4k.bin of=large.bin bs=4096 count=1 conv=notrunc 2>/dev/null
"""

EXTRA = (
    ("append-10mib-v1", "large", "printf APPEND >> large.bin", 1),
    ("resize-10mib-v1", "large", "truncate -s 5242880 large.bin; truncate -s 10485761 large.bin", 0),
    ("temp-replace-v1", "large", "cp /fixtures/v2/patch-4k.bin large.bin.next; mv -f large.bin.next large.bin", 1),
    ("prepend-4k-10mib-v2", "large", PREPEND, 1),
)


def seal(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def cases():
    inherited = shell.registry()["cases"]
    added = [{"scenario_id": name, "shape": shape, "command": command,
              "expected_write_min": writes, "expected_failure": False,
              "commit": True, "complete_command_limit_s": 15}
             for name, shape, command, writes in EXTRA]
    return inherited + added


def expected(name, shapes):
    files = dict(shapes["large"])
    old = files["large.bin"]
    if name == "append-10mib-v1":
        files["large.bin"] = old + b"APPEND"
    elif name == "resize-10mib-v1":
        files["large.bin"] = old[:5 << 20] + bytes((5 << 20) + 1)
    elif name == "temp-replace-v1":
        files["large.bin"] = b"P" * 4096
    elif name == "prepend-4k-10mib-v2":
        files["large.bin"] = b"P" * 4096 + old
    else:
        raise ValueError(name)
    return files


def prepare(output, reuse=None):
    output.mkdir(parents=True, exist_ok=False)
    if reuse:
        data = json.loads(reuse.read_text())
        current = shell.identities()
        if (current["source_dirty"] or data["source"]["product_seal"] != current["product_seal"]
                or data["source"]["harness_seal"] != current["harness_seal"]):
            raise ValueError("reuse requires the same sealed product and benchmark harness")
        for master in data["masters"].values():
            for kind in ("store", "history"):
                if shell.digest(Path(master["path"]) / f"{kind}.sqlite") != master[f"{kind}_sha256"]:
                    raise ValueError(f"closed master {kind} changed")
        source = reuse.parent / "functional"
        functional = output / "functional"
        shutil.copytree(source, functional)
        data["source"] = current
        data["prepared_master_reuse"] = {"source": str(reuse), "method": "closed sealed master byte copy"}
    else:
        shell.prepare(output / "base")
        data = json.loads((output / "base/prepared.json").read_text())
        functional = output / "functional"
        shutil.copytree(output / "base/masters", functional / "masters")
        shutil.copytree(output / "base/fixture", functional / "fixture")
    shapes, _ = shell.recipe()
    for name, _, _, _ in EXTRA:
        (functional / "fixture" / f"{name}.tsv").write_text(shell.manifest(expected(name, shapes)))
    for shape, master in data["masters"].items():
        master["path"] = str((functional / "masters" / shape).resolve())
    data["clone_method"] = "independent writable byte copy of a closed validated master"
    data["functional_contract"] = "issue252-ordinary-shell-functional-v1"
    data["functional_script_sha256"] = seal(__file__)
    data["functional_manifests"] = {p.name: seal(p) for p in (functional / "fixture").glob("*.tsv")}
    shell.json_file(output / "functional-prepared.json", data)
    print(json.dumps({"prepared": str(output / "functional-prepared.json"),
                      "cases": [row["scenario_id"] for row in cases()]}, sort_keys=True))


def run(prepared_path, output, selection):
    data = json.loads(prepared_path.read_text())
    current = shell.identities()
    if (current["source_dirty"] or data["source"]["source_commit"] != current["source_commit"]
            or data["source"]["product_seal"] != current["product_seal"]
            or data["source"]["harness_seal"] != current["harness_seal"]
            or data["functional_script_sha256"] != seal(__file__)):
        raise ValueError("functional source or harness changed since preparation")
    for name, wanted in data["functional_manifests"].items():
        if seal(prepared_path.parent / "functional/fixture" / name) != wanted:
            raise ValueError(f"manifest changed: {name}")
    rows = cases()
    if selection:
        rows = [row for row in rows if row["scenario_id"] == selection]
        if not rows:
            raise ValueError("unknown selection")
    output.mkdir(parents=True, exist_ok=False)
    results = []
    stopped = None
    for row in rows:
        if stopped:
            receipt = {"scenario_id": row["scenario_id"], "sample_count": 0,
                       "row_status": "NOT_RUN", "reason": stopped}
            folder = output / row["scenario_id"]
            folder.mkdir()
            shell.json_file(folder / "receipt.json", receipt)
        else:
            receipt = shell.run_case(row, data, output)
            if receipt["cleanup_status"] != "PASS":
                stopped = "previous cleanup failed or unknown"
        results.append({"scenario_id": row["scenario_id"],
                        "functional_status": receipt.get("functional_status", "NOT_RUN"),
                        "cleanup_status": receipt.get("cleanup_status", "NOT_RUN")})
    shell.json_file(output / "report.json", {"schema": "issue252-ordinary-shell-functional-v1",
                                           "source": data["source"], "prepared": str(prepared_path),
                                           "cases": results, "functional_script_sha256": seal(__file__)})
    print(json.dumps(results, sort_keys=True))
    if any(row["functional_status"] != "PASS" or row["cleanup_status"] != "PASS" for row in results):
        raise SystemExit(1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    p = commands.add_parser("prepare")
    p.add_argument("--output", required=True, type=Path)
    p.add_argument("--reuse", type=Path)
    p = commands.add_parser("run")
    p.add_argument("--prepared", required=True, type=Path)
    p.add_argument("--output", required=True, type=Path)
    p.add_argument("--case")
    args = parser.parse_args()
    if args.action == "prepare":
        prepare(args.output, args.reuse)
    else:
        run(args.prepared, args.output, args.case)


if __name__ == "__main__":
    main()
