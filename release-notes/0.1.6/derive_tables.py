#!/usr/bin/env python3
"""Derive the v0.1.6 release CSVs (report-only) from the two committed seed-1
matrices.

Reads `docs/roadmap/0.1/0.1.6/evidence/issue154/final-complete-matrix.json` (33
regular cases) and `final-complete-extended-matrix.json` (3 extensions), verifies
every receipt path it names against the local evidence tree, and writes
`release-notes/0.1.6/benchmark-performance.csv` and `benchmark-verification.csv`.
No benchmark, proof or test is executed, and no claim is invented: the tables carry
the measured complete-command wall, the declared target, the declared exception
(where one applies) and the gate verdict exactly as the driver recorded them.
"""
import csv
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
EVIDENCE = ROOT / "docs/roadmap/0.1/0.1.6/evidence/issue154"
OUT = ROOT / "release-notes" / "0.1.6"
TARGET_SECONDS = 15.0
EXCEPTION_SECONDS = 25.0
# The one declared verification exception (owner ruling on #154, ledger L12).
VERIFICATION_EXCEPTIONS = {"v016-branch-mixed-500mb-30000-k100-v1": 30.0}


def load(name):
    return json.loads((EVIDENCE / name).read_text())


def verify_receipt(path):
    if not path:
        return "no-receipt"
    root = ROOT / path
    return "present" if root.exists() else "MISSING"


def identities(row):
    """The identity chain of one row: the driver's requested block when it carries
    one, otherwise the receipt's own record (a verify-only case binds the sealed
    producer identity instead of a performance receipt)."""
    candidates = [(row.get("requested") or {}).get("identities") or {}]
    for mode in ("performance", "verification"):
        block = row.get(mode) or {}
        receipt = block.get("receipt") or {}
        candidates.append(receipt.get("identities") or {})
    # A verify-only case has no performance receipt and the report's row keeps no
    # identity block for it, so the raw verification receipt is read as the last
    # resort — the same receipt the evidence column names.
    for mode in ("verification", "performance"):
        evidence = ((row.get(mode) or {}).get("driver") or {}).get("receipt")
        if not evidence:
            continue
        root = ROOT / evidence
        if (root / "verification.json").is_file():
            receipt = json.loads((root / "verification.json").read_text())
            candidates.append({k: receipt.get(k) for k in ("source_identity", "host_executor", "product_identity") if receipt.get(k)})
        header = root / "perf.jsonl"
        if header.is_file():
            for line in header.read_text().splitlines():
                if line.strip().startswith("{"):
                    parsed = json.loads(line)
                    if parsed.get("kind") == "header":
                        candidates.append(parsed.get("identities") or {})
                    break
    source, product = "", ""
    for ids in candidates:
        source = source or ids.get("source_identity") or ids.get("image_source_identity") or ""
        executor = ids.get("host_executor") or {}
        if isinstance(executor, dict):
            product = product or executor.get("LAYERFS_PRODUCT_SEAL", "")
        product = product or ids.get("product_identity") or ""
    return source, product


def performance_row(row, tag):
    driver = row["performance"].get("driver", row["performance"])
    source, product = identities(row)
    declared = (
        VERIFICATION_EXCEPTIONS.get(row["case"], EXCEPTION_SECONDS)
        if row["family"] == "historical_access"
        else EXCEPTION_SECONDS
    )
    if driver.get("status") == "N/A":
        declared = ""
    return {
        "family": row["family"],
        "case": row["case"],
        "kind": "performance",
        "status": driver.get("status"),
        "gate": driver.get("gate") or "",
        "complete_wall_seconds": driver.get("complete_wall_seconds") or "",
        "declared_target_seconds": TARGET_SECONDS,
        "declared_exception_seconds": declared,
        "created_commits": (row["performance"].get("receipt") or {}).get("created_commit_count") or "",
        "phase": (row["performance"].get("receipt") or {}).get("phase") or "",
        "cleanup": ((driver.get("cleanup") or {}).get("status")) or "",
        "source_seal": source,
        "product_seal": product,
        "image": row["image"],
        "evidence": driver.get("receipt") or "",
        "receipt_state": verify_receipt(driver.get("receipt")),
        "tag": tag,
    }


def verification_row(row, tag):
    driver = row["verification"].get("driver", row["verification"])
    source, product = identities(row)
    return {
        "family": row["family"],
        "case": row["case"],
        "selection_kind": "performance" if row["performance"].get("driver", {}).get("status") != "N/A" else "verify-only",
        "proof_status": driver.get("status"),
        "gate": driver.get("gate") or "",
        "proof_wall_seconds": driver.get("complete_wall_seconds") or "",
        "declared_target_seconds": TARGET_SECONDS,
        "declared_exception_seconds": VERIFICATION_EXCEPTIONS.get(row["case"], EXCEPTION_SECONDS),
        "omissions": "; ".join(driver.get("omissions") or []),
        "cleanup": ((driver.get("cleanup") or {}).get("status")) or "",
        "source_seal": source,
        "product_seal": product,
        "image": row["image"],
        "evidence": driver.get("receipt") or "",
        "receipt_state": verify_receipt(driver.get("receipt")),
        "tag": tag,
    }


def main():
    rows = []
    for name, tag in (
        ("final-complete-matrix.json", "final3-seed1"),
        ("final-complete-extended-matrix.json", "final3-ext"),
    ):
        matrix = load(name)
        if matrix.get("tag") != tag:
            raise SystemExit(f"{name}: expected tag {tag}, found {matrix.get('tag')}")
        rows.extend((row, tag) for row in matrix["rows"])
    OUT.mkdir(parents=True, exist_ok=True)
    for name, builder in (
        ("benchmark-performance.csv", performance_row),
        ("benchmark-verification.csv", verification_row),
    ):
        table = [builder(row, tag) for row, tag in rows]
        missing = [entry for entry in table if entry["receipt_state"] == "MISSING"]
        if missing:
            raise SystemExit(f"{name}: missing receipts: {[e['case'] for e in missing]}")
        with (OUT / name).open("w", newline="") as stream:
            writer = csv.DictWriter(stream, fieldnames=list(table[0].keys()))
            writer.writeheader()
            writer.writerows(table)
        print(f"wrote {OUT / name} ({len(table)} rows)")


if __name__ == "__main__":
    main()
