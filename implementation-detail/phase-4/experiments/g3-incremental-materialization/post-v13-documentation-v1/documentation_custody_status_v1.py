#!/usr/bin/env python3
"""Read-only custody and status verification for the post-v13 documentation closure."""

import csv
import hashlib
import json
import os
import re
import stat
import subprocess
from pathlib import Path


REPO = Path(__file__).resolve().parents[5]
BASE = REPO / "implementation-detail/phase-4/experiments/g3-incremental-materialization"
SEALED_PARENT = REPO / "target/phase4-g3-incremental-materialization-20260822-v13"
SEALED = SEALED_PARENT / "results-v13"
DOCS = [
    "implementation-detail/phase-4/experiments/g3-incremental-materialization/G3-REPORT.md",
    "implementation-detail/phase-4/baseline/g3-incremental-materialization-baseline-v1.md",
    "implementation-detail/phase-4/baseline/index.md",
    "implementation-detail/phase-4/2026-08-21-phase-4-full-grind.md",
    "implementation-detail/phase-4/baseline/current-benchmark-scoreboard.md",
    "implementation-detail/phase-4/README.md",
    "research/phase-4/decision-map.md",
    "implementation-detail/phase-4/experiments/g3-incremental-materialization/execution-handoff.md",
]
DOC_HASHES = {
    DOCS[0]: "5748a36b9be0e2d21771483b1bc838804d47bc95801681df0863cb7c40caf462",
    DOCS[1]: "b94a638bc94be43f25d7e9b30248d93dcfc35d7170f6f85673389706f5695056",
    DOCS[2]: "9491bc1f9eeb2fb75bbb01d05bdb73adb28e2cf53363f49c0ccf3ee1b8aac96f",
    DOCS[3]: "03ca46e7772c63a9f39eaa50275edd82a0e5ece50fc1c0aff00b4a21bd8db304",
    DOCS[4]: "0cafb37d4d44659d226dae51d8ae7243612e628b4b3f943c540992393668d1de",
    DOCS[5]: "a5dc635898e53939e34e135471bffc22d6361babeb7d90a48e38678f4a67c830",
    DOCS[6]: "8ddb236ff7d3cfa03257c9006d8b6f219b151f7433a331b4f2b9ea900c0c30fb",
    DOCS[7]: "7854cd2c71d901e0990822c5be2e92cbaafd17023b16efad90c8a6370ed5cd25",
}
SEALED_HASHES = {
    "SOURCE-CUSTODY-v13.json": "348b6409a8d45a74d5a80808a95611ea8d79f67d882292b549a84fbf464c004c",
    "METHODOLOGY-CUSTODY-v13.json": "888213adc677a4634bbfd3b129b59f92ba8c13de447ee759907da0847e095849",
    "OPERAND-CUSTODY-v13.json": "58b652948950ed27e7ceb57c5b156705932e44e9d89724c63e8687f84b782d58",
    "CAMPAIGN-v13.json": "70be7a26ada3f0c378faed061819338620cc43708c3e5226aff3a360b5eb7e88",
    "rows-v13/G3-V13-RAW.jsonl": "3d2b40da82f612441cf1af88ee89f2d8c79b139c75818d6c7e2a5488cbad956c",
    "G3-PRIMARY-ANALYSIS-v13.json": "b28003f59dcf3fbfa6a585762d70cdc0beae0b4c81ec51904327d388452820d7",
    "G3-INDEPENDENT-RECOMPUTATION-v13.json": "2f137bb1116d1637656d1c89777dcb9e1291e04899f6710a000e5a6933419ace",
    "CLEANUP-v13.json": "ccb6edddfff96929e15e16b455a92df81314b7be3499143a8f92ebb27e87890e",
    "ROW-CLEANUP-v13.jsonl": "1b9e4fbdcb87c686dca9e6852fa535e6db68445114ef83c4e3c24017e172e506",
    "STATIC-CLOSURE-v13.json": "cbefce3c9ad384105acbf2c81e0a0d4304c8c7eb118d16d874ad6913de9e3531",
    "PAYLOAD-MANIFEST-v13.tsv": "1581f8f4b890237c6c04f17b79baf445067461767146c916b2d4df80c3030a49",
    "TERMINAL-v13.json": "1230187c702455eb3cf15aaa7d02197ebc5f60b196d08c072e524a87107a828e",
    "TERMINAL-VERIFICATION-v13.txt": "a9d06860828f14304b7f6fc1ef35146577e7ba770bacc4d4c428250d60169dd6",
}
IDENTITIES = {
    "source_set_sha256": "3a0330fc12cdc9b05b949a3f3f2b39f47e8d41d41234fffeedaa0ec65449058d",
    "methodology_set_sha256": "c6d04dd87b0cfc3794533e475be72e1564a87d142816c0360a6126179e0b6f5a",
    "executable_sha256": "535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e",
    "normalized_ledger_sha256": "19a3fd5ab1d5fb4dc00ffe396de1d118bfc38706d85c4009a974033d0a4010a1",
}
V11_ANCHORS = {
    "CAMPAIGN-v11.json": "9227c2eb31c8d897e163aceed0e2724c5d3d7617896fcb0069207ba061e7ef16",
    "rows-v11/G3-V11-RAW.jsonl": "47d979b9f687be75bfbc816608678b8ea1ef43e1317a3e7f9437abf7d5b93191",
    "G3-PRIMARY-ANALYSIS-v11.json": "0225e6e67411af363b8dcb1868d70572cf9dc8a4a9a76d295cd655ab29f8bbc3",
    "G3-INDEPENDENT-RECOMPUTATION-v11.json": "4f09586396e5bb35c5b758a7a91a3447283990f168edefca3d0006ecdbfb9366",
    "STATIC-CLOSURE-v11.json": "6de469522152ee2adf48c05e563fbf75d52cdbc312f4bc898e3d834e8b17c2ee",
    "PAYLOAD-MANIFEST-v11.tsv": "2950a6698983718e8c386a782b975e1ef807fa7a9ecf95cd59396d2473f3b27e",
    "TERMINAL-v11.json": "222bdc2abef4cd1435c6baec82a35bf05756e1aa385b10ae206bd27f9c6c351a",
    "TERMINAL-VERIFICATION-v11.txt": "995084a7ae284b940b951d9c67680d61d3ee56b350cac55df546dfcd883f99a8",
}
EXPECTED_STATUS = [
    " M Cargo.lock",
    " M crates/layerfs-engine/Cargo.toml",
    " M crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs",
    " M implementation-detail/phase-4/2026-08-21-phase-4-full-grind.md",
    " M implementation-detail/phase-4/README.md",
    " M implementation-detail/phase-4/baseline/current-benchmark-scoreboard.md",
    " M implementation-detail/phase-4/baseline/index.md",
    " M research/phase-4/decision-map.md",
    "?? crates/layerfs-engine/src/bin/phase4_g3_materialization.rs",
    "?? implementation-detail/phase-4/baseline/g3-incremental-materialization-baseline-v1.md",
    "?? implementation-detail/phase-4/experiments/g2-materialization-decomposition/",
    "?? implementation-detail/phase-4/experiments/g3-incremental-materialization/",
]


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1048576), b""):
            digest.update(block)
    return digest.hexdigest()


def canonical_hash(value):
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def need(condition, label):
    if not condition:
        raise RuntimeError(f"failed check: {label}")


def mode(path):
    return f"{stat.S_IMODE(path.stat().st_mode):04o}"


def git(*args):
    return subprocess.check_output(["git", *args], cwd=REPO, text=True).strip()


def status_lines():
    value = subprocess.check_output(
        ["git", "status", "--short"], cwd=REPO, text=True
    )
    return value, value.splitlines()


def rows_after(text, heading):
    tail = text.split(heading, 1)[1]
    lines = tail.splitlines()
    start = next(i for i, line in enumerate(lines) if line.startswith("| # |"))
    result = []
    for line in lines[start + 2 :]:
        if not line.startswith("|"):
            break
        result.append([cell.strip().replace("`", "") for cell in line.strip().strip("|").split("|")])
    return result


def main():
    need(Path.cwd().resolve() == REPO, "cwd")
    need(git("branch", "--show-current") == "codex/empty-worktree", "branch")
    need(git("rev-parse", "HEAD") == "d79f0e0e2582d1bc491410224fec2b6cef7482e9", "head")
    status, observed_status = status_lines()
    need(observed_status == EXPECTED_STATUS, "git-status")

    doc_rows = []
    texts = {}
    links = 0
    broken = []
    for name in DOCS:
        path = REPO / name
        data = path.read_bytes()
        text = data.decode("utf-8")
        texts[name] = text
        need(sha256(path) == DOC_HASHES[name], f"doc-hash:{name}")
        need(mode(path) == "0644", f"doc-mode:{name}")
        need(data.endswith(b"\n") and b"\r" not in data, f"doc-newline:{name}")
        need(
            all(line == line.rstrip(" \t") for line in text.splitlines()),
            f"doc-whitespace:{name}",
        )
        for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)", text):
            if target.startswith(("http://", "https://", "mailto:", "#")):
                continue
            links += 1
            clean = target.strip("<>").split("#", 1)[0]
            if not (path.parent / clean).resolve().exists():
                broken.append({"document": name, "target": target})
        doc_rows.append(
            {
                "path": name,
                "sha256": DOC_HASHES[name],
                "size_bytes": path.stat().st_size,
                "mode": mode(path),
            }
        )
    need(links == 127 and not broken, "document-links")

    all_text = "\n".join(texts.values())
    status_phrase = "G3 PASS / G4 READY — v13 STATICALLY CLOSED AND TERMINALLY SEALED"
    need(
        all(
            status_phrase
            in re.sub(r"\s+", " ", re.sub(r"(?m)^>\s?", "", texts[name]))
            for name in DOCS
        ),
        "controlling-status",
    )
    need(all("UNSTARTED" in texts[name].upper() for name in DOCS), "g4-unstarted")
    need("Phase 4 remains incomplete" in all_text or "Phase 4 is not complete" in all_text, "phase4-incomplete")
    need("G5 and G6 remain pending" in all_text or "G5/G6 remain pending" in all_text, "g5-g6")
    need("no persistent replayable destination receipt" in all_text, "no-persistent-receipt")
    need("malicious same-UID" in all_text, "same-uid-limit")
    need("Physical I/O" in all_text and "cache" in all_text and "stable-media" in all_text, "io-limitations")
    scoreboard = texts[DOCS[4]]
    need("**3.414166 ms**" in scoreboard and "not a median or acceptance result" in scoreboard, "scoreboard-screen")
    need("Proven cold native materialization | **Unavailable**" in scoreboard, "cold-unavailable")
    need("Trusted hot read | **Unavailable**" in scoreboard, "hot-unavailable")
    need("First edit after reopen | **154.019 ms**" in scoreboard, "reopen-edit")

    for name, expected in SEALED_HASHES.items():
        need(sha256(SEALED / name) == expected, f"sealed-hash:{name}")
    terminal = json.loads((SEALED / "TERMINAL-v13.json").read_text())
    verification = json.loads((SEALED / "TERMINAL-VERIFICATION-v13.txt").read_text())
    campaign = json.loads((SEALED / "CAMPAIGN-v13.json").read_text())
    primary = json.loads((SEALED / "G3-PRIMARY-ANALYSIS-v13.json").read_text())
    independent = json.loads((SEALED / "G3-INDEPENDENT-RECOMPUTATION-v13.json").read_text())
    static_closure = json.loads((SEALED / "STATIC-CLOSURE-v13.json").read_text())
    need(terminal["status"] == "PASS" and terminal["g4_eligible"] is True, "terminal-status")
    need(terminal["payload_manifest_entries"] == 67, "terminal-manifest-count")
    need(verification["status"] == "PASS" and verification["manifest_closure_exact"] is True, "terminal-verification")
    need(verification["payload_mismatches"] == 0, "terminal-payload")
    for key, expected in IDENTITIES.items():
        need(terminal[key] == expected, f"terminal-identity:{key}")
        if key in campaign:
            need(campaign[key] == expected, f"campaign-identity:{key}")
    need(primary["status"] == independent["status"] == "PASS", "analyses-status")
    need(primary["normalized_ledger"] == independent["normalized_ledger"], "ledger-equality")
    need(canonical_hash(primary["normalized_ledger"]) == IDENTITIES["normalized_ledger_sha256"], "ledger-hash")
    need(campaign["rows"] == 9 and campaign["rows_rerun"] == 0, "campaign-schedule")
    need(campaign["operation_total_ns"] == 22948873 and campaign["global_elapsed_ns"] == 17722050000, "campaign-timers")

    with (SEALED / "PAYLOAD-MANIFEST-v13.tsv").open(newline="") as handle:
        manifest = list(csv.DictReader(handle, delimiter="\t"))
    need(len(manifest) == 67, "manifest-entries")
    for row in manifest:
        path = SEALED / row["path"]
        need(path.is_file() and not path.is_symlink(), f"manifest-kind:{row['path']}")
        need(sha256(path) == row["sha256"], f"manifest-hash:{row['path']}")
        need(path.stat().st_size == int(row["size_bytes"]), f"manifest-size:{row['path']}")
        need(mode(path) == "0444", f"manifest-mode:{row['path']}")
    manifest_paths = {row["path"] for row in manifest}
    special = {"PAYLOAD-MANIFEST-v13.tsv", "TERMINAL-v13.json", "TERMINAL-VERIFICATION-v13.txt"}
    actual_payload = {
        str(path.relative_to(SEALED))
        for path in SEALED.rglob("*")
        if path.is_file() and str(path.relative_to(SEALED)) not in special
    }
    need(actual_payload == manifest_paths, "manifest-closure")
    files = sorted(path for path in SEALED.rglob("*") if path.is_file())
    directories = [SEALED_PARENT, SEALED, *sorted(path for path in SEALED.rglob("*") if path.is_dir())]
    need(len(files) == 70 and all(mode(path) == "0444" for path in files), "sealed-files")
    need(len(directories) == 14 and all(mode(path) == "0555" for path in directories), "sealed-directories")
    need(not any(path.is_symlink() for path in SEALED_PARENT.rglob("*")), "sealed-symlinks")
    need(not (REPO / "target/phase4-g3-incremental-materialization-20260822-v13.lock").exists(), "sealed-lock")
    need(not (SEALED / "FAILURE-v13.json").exists(), "sealed-failure")
    sealed_rows = [
        {
            "path": str(path.relative_to(SEALED_PARENT)),
            "sha256": sha256(path),
            "size_bytes": path.stat().st_size,
            "mode": mode(path),
        }
        for path in files
    ]
    sealed_fingerprint = canonical_hash(sealed_rows)

    raw = [json.loads(line) for line in (SEALED / "rows-v13/G3-V13-RAW.jsonl").read_text().splitlines()]
    need(len(raw) == 9 and [row["sequence"] for row in raw] == list(range(1, 10)), "raw-schedule")
    report = texts[DOCS[0]]
    direct = rows_after(report, "### Route and primary direct counters")
    expected_direct = []
    for row in raw:
        expected_direct.append(
            [
                str(row["sequence"]),
                row["scenario"],
                f"{row['route']} / {row['outcome']}",
                f"{row['authority_validation_successes']}/{row['authority_validation_failures']}",
                str(row["permit_consumptions"]),
                f"{row['payload_sql_queries']}/{row['payload_sql_rows']}/{row['canonical_blob_reads']}",
                str(row["canonical_bytes_authenticated"]),
                str(row["source_bytes_reconstructed"]),
                f"{row['clone_successes']}/{row['clone_failures']}",
                f"{row['changed_ranges']}/{row['changed_bytes']}",
                f"{row['patch_calls']}/{row['patch_bytes']}",
                f"{row['fallback_calls']}/{row['fallback_write_bytes']}",
            ]
        )
    need(direct == expected_direct, "report-direct-table")
    reconciliation = rows_after(report, "### Reconciliation and durability counters")
    expected_reconciliation = []
    for row in raw:
        expected_reconciliation.append(
            [
                str(row["sequence"]),
                row["reconciliation_outcome"],
                str(row["reconciliation_calls"]),
                f"{row['reconciliation_sql_queries']}/{row['reconciliation_sql_rows']}/{row['reconciliation_blob_reads']}",
                str(row["reconciliation_canonical_bytes_authenticated"]),
                str(row["reconciliation_source_bytes_compared"]),
                str(row["destination_bytes_read"]),
                f"{row['data_sync_calls']}/{row['metadata_sync_calls']}/{row['rename_calls']}/{row['directory_sync_calls']}",
                f"{row['temp_files_created']}/{row['temp_files_removed']}",
                f"{row['seed_files_created']}/{row['seed_files_removed']}",
                row["old_or_new"],
            ]
        )
    need(reconciliation == expected_reconciliation, "report-reconciliation-table")
    timer = rows_after(report, "### Exact timer equations")
    timer_fields = [
        "timer_preflight_ns", "timer_qualification_ns", "timer_payload_prepare_ns",
        "timer_data_sync_ns", "timer_metadata_ns", "timer_metadata_sync_ns",
        "timer_rename_ns", "timer_directory_sync_ns", "timer_reconciliation_ns",
        "timer_cleanup_ns", "attributed_wall_ns", "unattributed_wall_ns",
        "operation_total_ns",
    ]
    need(timer == [[str(row["sequence"]), *[str(row[key]) for key in timer_fields]] for row in raw], "report-timer-table")
    storage = json.loads((SEALED / "STORAGE-v13.json").read_text())
    allocation = {row["sequence"]: row["pre_delete_row"]["allocated_bytes"] for row in storage["samples"]}
    resources = rows_after(report, "### Exact RSS, Q, storage, and output gates")
    expected_resources = [
        [
            str(row["sequence"]), str(row["operation_total_ns"]), f"{row['external_real_seconds']:g}",
            str(row["maximum_resident_set_bytes"]), f"{row['q_high_water']}/{row['q_terminal']}",
            str(allocation[row["sequence"]]), f"{str(row['byte_exact']).lower()}/{str(row['mode_exact']).lower()}",
            f"{row['temp_residue_count']}/{row['seed_residue_count']}",
        ]
        for row in raw
    ]
    need(resources == expected_resources, "report-resource-table")

    need(static_closure["status"] == "PASS" and len(static_closure["focused_test_names"]) == 15, "focused-static")
    need(static_closure["workspace_aggregate"] == {"all_ok": True, "failed": 0, "ignored": 1, "passed": 157, "summary_lines": 11}, "workspace-static")
    focused_stdout = (SEALED / "static-v13/01-focused-g3-tests.stdout").read_text()
    observed_focused = re.findall(r"test (phase4_g3_materialization::tests::[^ ]+) \.\.\. ok", focused_stdout)
    need(sorted(observed_focused) == sorted(static_closure["focused_test_names"]), "focused-names")
    workspace_stdout = (SEALED / "static-v13/02-workspace-tests.stdout").read_text()
    counts = [tuple(map(int, match)) for match in re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured;", workspace_stdout)]
    need(tuple(sum(row[i] for row in counts) for i in range(4)) == (157, 0, 1, 0), "workspace-counts")

    v11_disposition = BASE / "G3-V11-POST-SEAL-REAUDIT-DISPOSITION-v1.md"
    need(sha256(v11_disposition) == "8226aacee217a58436b2c8405d953ee18882e5ad400662f1004368a91a26dae5", "v11-disposition-hash")
    need("G3-v11 historical REVISE; sealed evidence integrity PASS" in v11_disposition.read_text(), "v11-disposition")
    v11 = REPO / "target/phase4-g3-incremental-materialization-20260822-v11/results-v11"
    for name, expected in V11_ANCHORS.items():
        need(sha256(v11 / name) == expected, f"v11-anchor:{name}")
    v11_terminal = json.loads((v11 / "TERMINAL-v11.json").read_text())
    need(v11_terminal["source_set_sha256"] == "45a08ba60b02316bc803cca69871e773751009bb9f9196fb9c03e8c7ad705821", "v11-source-set")
    need(v11_terminal["methodology_set_sha256"] == "e7194aa398476a06a0706e93e1af70e06a15a9fb662be7d20014117f463856d8", "v11-method-set")
    need(v11_terminal["executable_sha256"] == "82136ed86f19e645cb5611b9b520fe0454b947188a824e6b7022491421b34cd3", "v11-executable")
    need("HISTORICAL REVISE" in report, "report-v11-revise")

    v12_disposition = BASE / "v12/V12-PREEXEC-REVISE.md"
    need(sha256(v12_disposition) == "13d7bd160b730285ba4457fcabc0107c8064ed6c63bdf9a1cfc84e275596e2c8", "v12-disposition-hash")
    need("zero v12 measured rows" in v12_disposition.read_text(), "v12-zero-rows")
    need(not (REPO / "target/phase4-g3-incremental-materialization-20260822-v12").exists(), "v12-root-absent")
    need(not (REPO / "target/phase4-g3-incremental-materialization-20260822-v12.lock").exists(), "v12-lock-absent")

    output = {
        "status": "PASS",
        "branch": "codex/empty-worktree",
        "head": "d79f0e0e2582d1bc491410224fec2b6cef7482e9",
        "docs": doc_rows,
        "docs_set_sha256": canonical_hash(doc_rows),
        "links_checked": links,
        "broken_links": broken,
        "git_status_lines": observed_status,
        "git_status_sha256": hashlib.sha256(status.encode()).hexdigest(),
        "sealed_hashes": SEALED_HASHES,
        "identities": IDENTITIES,
        "manifest_entries": 67,
        "sealed_root": {
            "files_0444": 70,
            "directories_0555": 14,
            "symlinks": 0,
            "lock_absent": True,
            "failure_absent": True,
            "fingerprint_sha256": sealed_fingerprint,
        },
        "rows_checked": 9,
        "report_tables_checked": ["direct", "reconciliation", "timers", "resources"],
        "focused_tests": 15,
        "workspace_tests": {"passed": 157, "ignored": 1, "failed": 0},
        "history": {
            "v11": {"status": "HISTORICAL_REVISE", "sealed_integrity": "PASS"},
            "v12": {"status": "PREEXEC_REVISE", "rows": 0, "root_absent": True},
            "v13": {"status": "PASS_SEALED", "rows": 9, "rows_reused": 0},
        },
        "stage": {
            "g3": "PASS_SEALED",
            "g4": "READY_UNSTARTED",
            "g5": "PENDING",
            "g6": "PENDING",
            "phase4_complete": False,
            "production_integrated": False,
            "platform_integrated": False,
        },
        "limitations": {
            "benchmark_private": True,
            "macos_apfs_process_custody": True,
            "persistent_replayable_destination_receipt": False,
            "malicious_same_uid_guarantee": False,
            "physical_io": "Unavailable",
            "cache_residency": "Unavailable",
            "stable_media": "Unavailable",
        },
    }
    print(json.dumps(output, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    main()
