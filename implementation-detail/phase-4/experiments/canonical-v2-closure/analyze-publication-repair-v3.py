#!/usr/bin/env python3
"""Fail-closed v3 analyzer: audited v2 gates plus authority mode custody."""

import csv
import hashlib
import importlib.util
import json
import stat
import sys
from pathlib import Path

sys.dont_write_bytecode = True

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
V2_ANALYZER = HERE / "analyze-publication-repair-v2.py"
V2_RUNNER = HERE / "run-publication-repair-v2.py"
V2 = REPO / "target/phase4-canonical-v2-publication-repair-20260821-v2/results-v1"
V2_MANIFEST = V2 / "TERMINAL-MANIFEST-v1.tsv"
V1_AUTHORITY = REPO / (
    "target/phase4-canonical-v2-publication-repair-20260821-v1/results-v1/"
    "work-v1/masters/one-byte-middle-100-B/"
    "db-K64-F64-104857600-one-byte-middle-970021.sqlite.authority")
SOURCE = REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs"

V2_MANIFEST_SHA = "1e94b51bbc46524ad164aa3db836026d4a79e200f8bf3bd1cb7ba5c176b35131"
V2_VERIFICATION_SHA = "02a7d0f17c8e80658dbacbc5d52d17a97aee542dccd189a3617132dfc47ef5e0"
V2_ANALYSIS_SHA = "576d53c6ae3d1b4104fbf28763065e53f23c6ad65a916d465f04d22f4d3264bf"
V2_DISPOSITION_SHA = "035b2f46db17b748cb4cb2284940b7607bbad4efec60ad5d86b53ba95d60dc02"
V2_STDERR_SHA = "f665ab00c6a188b15810f6c01f152a5941021e698ed13e11cad7c62416d56679"
V2_INVOCATIONS_SHA = "9b81ce6fd1fcfcb3ee74894d98d67d13c4666f5bec19bc8b10bf232a61d4bc37"
V2_INPUT_CUSTODY_SHA = "65a8884ffdbee2fc6520f21dd7354b24e28d5e64c4ebbf8c180ceaf4483be307"
EMPTY_SHA = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
AUTHORITY_SHA = "abac9762e55b20e4a7db6b42bfaa435fb9af8e3a0a79d061f4dd05ee63ef6f12"


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


v2 = load(V2_ANALYZER, "canonical_v2_publication_repair_v2_analyzer_core")


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def tsv(path):
    return list(csv.DictReader(path.open(), delimiter="\t"))


def verify_v2_history(reasons):
    anchors = {
        V2_MANIFEST: V2_MANIFEST_SHA,
        V2 / "TERMINAL-MANIFEST-VERIFICATION-v1.txt": V2_VERIFICATION_SHA,
        V2 / "ANALYSIS-v1.json": V2_ANALYSIS_SHA,
        V2 / "DISPOSITION-v1.txt": V2_DISPOSITION_SHA,
        V2 / "ACTUAL-INVOCATIONS-v1.tsv": V2_INVOCATIONS_SHA,
        V2 / "INPUT-CUSTODY-v1.tsv": V2_INPUT_CUSTODY_SHA,
        V2 / "logs-v1/row-01-fresh-one-byte-middle-B.stderr": V2_STDERR_SHA,
        V2 / "logs-v1/row-01-fresh-one-byte-middle-B.stdout": EMPTY_SHA,
        V2 / "RAW-v1.jsonl": EMPTY_SHA,
    }
    for path, expected in anchors.items():
        if not path.is_file() or sha(path) != expected:
            reasons.append(f"v2-anchor:{path.name}")
    manifest = tsv(V2_MANIFEST) if V2_MANIFEST.is_file() else []
    mismatches = []
    for row in manifest:
        path = REPO / row["path"]
        if (not path.is_file() or sha(path) != row["sha256"]
                or path.stat().st_size != int(row["size_bytes"])):
            mismatches.append(row["path"])
    recorded = {(REPO / row["path"]).resolve() for row in manifest}
    verification = V2 / "TERMINAL-MANIFEST-VERIFICATION-v1.txt"
    actual = {path.resolve() for path in V2.rglob("*") if path.is_file()}
    if (len(manifest) != 20 or mismatches
            or actual != recorded | {V2_MANIFEST.resolve(), verification.resolve()}
            or len(actual) != 22):
        reasons.append("v2-manifest-closure")
    text = verification.read_text() if verification.is_file() else ""
    if ("status=PASS\nentries=20\nmismatches=0\n" not in text
            or f"manifest_sha256={V2_MANIFEST_SHA}\n" not in text
            or "wall_ceiling_seconds=59\nchild_ceiling_seconds=15\n" not in text):
        reasons.append("v2-manifest-verification")
    analysis = json.loads((V2 / "ANALYSIS-v1.json").read_text())
    if analysis != {
        "disposition": "CANONICAL-V2 PUBLICATION-REPAIR-v2 REVISE",
        "reasons": ["RuntimeError: row-01-fresh-one-byte-middle-B exited 1"],
        "status": "REVISE",
    }:
        reasons.append("v2-historical-disposition")
    stderr = V2 / "logs-v1/row-01-fresh-one-byte-middle-B.stderr"
    if not stderr.is_file() or stderr.read_text() != "Error: ValidationAuthorityUnavailable\n":
        reasons.append("v2-failure-text")
    invocations = tsv(V2 / "ACTUAL-INVOCATIONS-v1.tsv")
    expected_command = " ".join(map(str, [
        V2 / "operands-v1/phase4_create_edit_benchmark-canonical-v2-publication-repair",
        "--fast-row", V2 / "work-v1/rows/01-fresh-one-byte-middle-B",
        "104857600", "edit-one-byte-middle", "990001", "false", "capture-only",
    ]))
    if (len(invocations) != 2
            or [(row.get("event"), row.get("exit")) for row in invocations]
                != [("started", "-"), ("completed", "1")]
            or any(row.get("command") != expected_command for row in invocations)):
        reasons.append("v2-invocation")
    if (V2 / "RAW-v1.jsonl").stat().st_size != 0:
        reasons.append("v2-row-count")
    if (not V1_AUTHORITY.is_file() or V1_AUTHORITY.is_symlink()
            or stat.S_IMODE(V1_AUTHORITY.lstat().st_mode) != 0o444
            or V1_AUTHORITY.stat().st_size != 32 or sha(V1_AUTHORITY) != AUTHORITY_SHA):
        reasons.append("v2-authority-source")
    source_text = SOURCE.read_text()
    runner_text = V2_RUNNER.read_text()
    if ("metadata.permissions().mode() & 0o777 != 0o600" not in source_text
            or 'destination.open("xb")' not in runner_text
            or "destination.chmod(0o600)" in runner_text):
        reasons.append("v2-causal-chain")
    residue = [str(path.relative_to(V2)) for path in V2.rglob("*")
               if path.name.endswith(("-journal", "-wal", "-shm"))]
    if residue:
        reasons.append("v2-residue")
    return {
        "disposition": "CANONICAL-V2 PUBLICATION-REPAIR-v2 REVISE",
        "reason": "ValidationAuthorityUnavailable before JSON row",
        "manifest_sha256": V2_MANIFEST_SHA,
        "manifest_entries": 20,
        "root_files": 22,
        "raw_rows": 0,
        "relabelled": False,
    }


def verify_authority_mode(root, reasons):
    custody_path = root / "AUTHORITY-MODE-CUSTODY-v1.tsv"
    custody = tsv(custody_path) if custody_path.is_file() else []
    target = root / (
        "work-v1/rows/01-fresh-one-byte-middle-B/"
        "db-K64-F64-104857600-one-byte-middle-990001.sqlite.authority")
    if len(custody) != 1:
        reasons.append("v3-authority-mode-custody-count")
        return
    row = custody[0]
    try:
        source_stat, target_stat = V1_AUTHORITY.lstat(), target.lstat()
        expected = {
            "source_path": str(V1_AUTHORITY.relative_to(REPO)),
            "source_device": str(source_stat.st_dev),
            "source_inode": str(source_stat.st_ino),
            "source_mode_octal": "0444",
            "target_path": str(target.relative_to(REPO)),
            "target_device": str(target_stat.st_dev),
            "target_inode": str(target_stat.st_ino),
            "target_mode_copied_octal": "0644",
            "target_mode_runtime_octal": "0600",
            "sha256_before_chmod": AUTHORITY_SHA,
            "sha256_after_chmod": AUTHORITY_SHA,
            "change_scope": "target-only",
        }
        if any(row.get(key) != value for key, value in expected.items()):
            reasons.append("v3-authority-mode-custody")
        if (V1_AUTHORITY.is_symlink() or target.is_symlink()
                or not stat.S_ISREG(source_stat.st_mode) or not stat.S_ISREG(target_stat.st_mode)
                or source_stat.st_size != target_stat.st_size or target_stat.st_size != 32
                or (source_stat.st_dev, source_stat.st_ino)
                    == (target_stat.st_dev, target_stat.st_ino)
                or stat.S_IMODE(source_stat.st_mode) != 0o444
                or stat.S_IMODE(target_stat.st_mode) != 0o600
                or sha(V1_AUTHORITY) != AUTHORITY_SHA or sha(target) != AUTHORITY_SHA):
            reasons.append("v3-authority-live-mode-or-hash")
    except (KeyError, OSError, TypeError, ValueError):
        reasons.append("v3-authority-mode-custody")


def analyze(root):
    base = v2.analyze(root)
    reasons = list(base.get("reasons", []))
    sealed_v2 = verify_v2_history(reasons)
    verify_authority_mode(root, reasons)
    status = "PASS" if not reasons else "REVISE"
    base.update({
        "status": status,
        "disposition": ("CANONICAL-V2 PUBLICATION-REPAIR-v3 PASS / SCREEN CLOSED"
                        if status == "PASS" else
                        "CANONICAL-V2 PUBLICATION-REPAIR-v3 REVISE"),
        "reasons": reasons,
        "fresh_row_timing_claim": "none",
        "authority_mode_change": "copied-target-only:0644-to-0600; source-remains-0444",
        "authority_mode_observation_boundary": "post-child-pre-terminal-seal",
        "terminal_sealed_mode_expected": "0444",
        "sealed_v2": sealed_v2,
        "screen_closed": status == "PASS",
        "eligible_for_complete_canonical_v2_validation": status == "PASS",
        "promotion_authorized": False,
        "limitations": [
            "The fresh row is a deterministic semantic closure; it makes no timing claim.",
            "The two full-create improvements are composed unchanged from sealed v1 raw rows.",
            "Sealed v2 remains historical REVISE and is not relabelled.",
            "Runtime target mode 0600 is observed post-child before the inherited terminal seal intentionally makes the artifact copy read-only 0444; post-seal audit uses the manifested custody/analyzer proof chain.",
            "PASS closes only this compact screen and does not promote or integrate canonical-v2.",
        ],
    })
    return base


def write_outputs(root, result):
    (root / "ANALYSIS-v1.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    report = [
        "# Canonical-v2 publication repair authority-mode closure v3\n",
        f"Disposition: **{result['disposition']}**\n",
        "Sealed v1 and v2 remain historical **REVISE** and are not relabelled.\n",
        "\nHistorical sealed-v1 full-create comparisons recomputed from raw rows:\n",
    ]
    for pair in result.get("sealed_v1", {}).get("full_create_pairs", []):
        report.append(
            f"- pair {pair['pair']} {pair['order']}: control {pair['control_ns']/1e6:.3f} ms; "
            f"candidate {pair['candidate_ns']/1e6:.3f} ms; improvement "
            f"{pair['improvement_percent']:.3f}%.\n")
    report.extend([
        "\nSealed v2: REVISE before a JSON row, exact error "
        "`ValidationAuthorityUnavailable`; manifest 20 entries/zero mismatches and "
        "22 complete root files.\n",
        "\nV3 one change: the independent target authority copy is hash-verified, changed "
        "from copied mode `0644` to runtime mode `0600`, and rehash-verified; the retained "
        "source remains `0444`.\n",
        "\nFresh evidence is exactly one candidate-only `one-byte-middle` semantic row. "
        "Its elapsed values make no performance claim.\n",
        "\nHard-gate reasons: "
        + (", ".join(result["reasons"]) if result["reasons"] else "none") + ".\n",
        "\nLimitations:\n" + "".join(f"- {item}\n" for item in result["limitations"]),
    ])
    (root / "REPORT-v1.md").write_text("\n".join(report))
    disposition = result["disposition"] + "\n"
    disposition += "Sealed v1/v2 remain historical REVISE; CP-0009 remains accepted.\n"
    disposition += "Reasons: " + (", ".join(result["reasons"]) if result["reasons"] else "none") + "\n"
    disposition += ("Eligible only for complete canonical-v2 validation; no promotion, integration, commit, or later optimization is authorized.\n"
                    if result["status"] == "PASS" else
                    "Not eligible; no promotion, integration, commit, or later optimization is authorized.\n")
    (root / "DISPOSITION-v1.txt").write_text(disposition)


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: analyze-publication-repair-v3.py RESULT_ROOT")
    root = Path(sys.argv[1]).resolve()
    try:
        result = analyze(root)
    except Exception as error:
        result = {
            "status": "REVISE",
            "disposition": "CANONICAL-V2 PUBLICATION-REPAIR-v3 REVISE",
            "reasons": [f"analyzer:{type(error).__name__}:{error}"],
            "sealed_v1": {"full_create_pairs": []},
            "sealed_v2": {},
            "screen_closed": False,
            "eligible_for_complete_canonical_v2_validation": False,
            "promotion_authorized": False,
            "limitations": [],
        }
    write_outputs(root, result)
    print(json.dumps(result, sort_keys=True))
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
