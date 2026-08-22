#!/usr/bin/env python3
"""Independent read-only verification of the post-v13 documentation closure."""

import hashlib
import json
import os
import re
import stat
import subprocess
from pathlib import Path


HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
BASE = HERE.parent
CLOSURE = BASE / "G3-POST-V13-DOCUMENTATION-CLOSURE-v1.json"
VERIFICATION = BASE / "G3-POST-V13-DOCUMENTATION-VERIFICATION-v1.txt"
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
DOC_HASHES = [
    "5748a36b9be0e2d21771483b1bc838804d47bc95801681df0863cb7c40caf462",
    "b94a638bc94be43f25d7e9b30248d93dcfc35d7170f6f85673389706f5695056",
    "9491bc1f9eeb2fb75bbb01d05bdb73adb28e2cf53363f49c0ccf3ee1b8aac96f",
    "03ca46e7772c63a9f39eaa50275edd82a0e5ece50fc1c0aff00b4a21bd8db304",
    "0cafb37d4d44659d226dae51d8ae7243612e628b4b3f943c540992393668d1de",
    "a5dc635898e53939e34e135471bffc22d6361babeb7d90a48e38678f4a67c830",
    "8ddb236ff7d3cfa03257c9006d8b6f219b151f7433a331b4f2b9ea900c0c30fb",
    "7854cd2c71d901e0990822c5be2e92cbaafd17023b16efad90c8a6370ed5cd25",
]
EXPECTED_SEALED = {
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


def mode(path):
    return f"{stat.S_IMODE(path.stat().st_mode):04o}"


def need(condition, label):
    if not condition:
        raise RuntimeError(f"independent verification failed: {label}")


def file_row(path, relative_to=REPO):
    return {
        "path": str(path.relative_to(relative_to)),
        "sha256": sha256(path),
        "size_bytes": path.stat().st_size,
        "mode": mode(path),
    }


def main():
    need(Path.cwd().resolve() == REPO, "cwd")
    need(CLOSURE.is_file() and not VERIFICATION.exists(), "closure-state")
    closure_bytes = CLOSURE.read_bytes()
    closure = json.loads(closure_bytes)
    closure_hash = hashlib.sha256(closure_bytes).hexdigest()
    need(closure["schema"] == "phase4-g3-post-v13-documentation-closure-v1", "schema")
    need(closure["status"] == "PASS", "closure-status")
    need(closure["branch"] == "codex/empty-worktree", "branch")
    need(closure["head"] == "d79f0e0e2582d1bc491410224fec2b6cef7482e9", "head")
    need(closure["artifact_outside_sealed_payload"] is True, "artifact-location")
    need(closure["sealed_payload_unchanged"] is True, "sealed-unchanged")
    need(closure["commands_planned"] == closure["commands_executed"] == 3, "command-count")
    need(closure["commands_rerun"] == 0, "command-rerun")
    need(not any(closure[key] for key in ["build_rerun", "tests_rerun", "campaign_rerun", "static_closure_rerun", "finalizer_rerun"]), "forbidden-reruns")

    expected_plan = [
        ["cargo", "fmt", "--all", "--", "--check"],
        ["git", "diff", "--check", "--", *DOCS],
        ["python3", str(HERE / "documentation_custody_status_v1.py")],
    ]
    stream_rows = []
    for sequence, (command, expected_argv) in enumerate(zip(closure["commands"], expected_plan), 1):
        need(command["sequence"] == sequence and command["argv"] == expected_argv, f"command-{sequence}-argv")
        need(command["exit_code"] == 0 and command["wall_ns"] > 0, f"command-{sequence}-result")
        for kind in ["stdout", "stderr"]:
            path = REPO / command[f"{kind}_path"]
            need(path.is_file(), f"command-{sequence}-{kind}-kind")
            need(sha256(path) == command[f"{kind}_sha256"], f"command-{sequence}-{kind}-hash")
            need(path.stat().st_size == command[f"{kind}_size_bytes"], f"command-{sequence}-{kind}-size")
            need(mode(path) == command[f"{kind}_mode"] == "0644", f"command-{sequence}-{kind}-mode")
            stream_rows.append(file_row(path))
    need(stream_rows == closure["command_streams"], "stream-rows")
    need(canonical_hash(stream_rows) == closure["command_streams_set_sha256"], "stream-set")
    need(all(row["size_bytes"] == 0 for row in stream_rows[:4]), "empty-command-streams")
    need(stream_rows[5]["size_bytes"] == 0, "custody-stderr")
    custody = json.loads((REPO / stream_rows[4]["path"]).read_text())
    need(custody["status"] == "PASS", "custody-status")

    script_rows = [file_row(REPO / row["path"]) for row in closure["input_scripts"]]
    need(script_rows == closure["input_scripts"], "script-rows")
    need(canonical_hash(script_rows) == closure["input_scripts_set_sha256"], "script-set")
    need(any(row["path"].endswith("verify_closure_v1.py") for row in script_rows), "independent-script-bound")

    doc_rows = []
    links = 0
    texts = []
    for name, expected in zip(DOCS, DOC_HASHES):
        path = REPO / name
        data = path.read_bytes()
        text = data.decode()
        texts.append(text)
        need(sha256(path) == expected and mode(path) == "0644", f"doc:{name}")
        need(data.endswith(b"\n") and b"\r" not in data, f"doc-newline:{name}")
        need(all(line == line.rstrip(" \t") for line in text.splitlines()), f"doc-whitespace:{name}")
        for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)", text):
            if target.startswith(("http://", "https://", "mailto:", "#")):
                continue
            links += 1
            clean = target.strip("<>").split("#", 1)[0]
            need((path.parent / clean).resolve().exists(), f"link:{name}:{target}")
        doc_rows.append(file_row(path))
    need(links == closure["links_checked"] == custody["links_checked"] == 127, "links")
    need(doc_rows == closure["docs"] == custody["docs"], "docs")
    need(canonical_hash(doc_rows) == closure["docs_set_sha256"] == custody["docs_set_sha256"], "docs-set")

    status_text = subprocess.check_output(["git", "status", "--short"], cwd=REPO, text=True)
    status = {
        "lines": status_text.splitlines(),
        "line_count": len(status_text.splitlines()),
        "sha256": hashlib.sha256(status_text.encode()).hexdigest(),
    }
    need(closure["pre_git_status"] == closure["post_git_status"] == status, "git-status")
    need(closure["pre_post_git_status_equal"] is True and closure["pre_post_docs_equal"] is True, "pre-post")

    sealed_files = sorted(path for path in SEALED.rglob("*") if path.is_file())
    sealed_rows = [file_row(path, SEALED_PARENT) for path in sealed_files]
    sealed_fingerprint = canonical_hash(sealed_rows)
    need(len(sealed_files) == 70 and all(mode(path) == "0444" for path in sealed_files), "sealed-files")
    sealed_dirs = [SEALED_PARENT, SEALED, *sorted(path for path in SEALED.rglob("*") if path.is_dir())]
    need(len(sealed_dirs) == 14 and all(mode(path) == "0555" for path in sealed_dirs), "sealed-dirs")
    need(not any(path.is_symlink() for path in SEALED_PARENT.rglob("*")), "sealed-symlinks")
    need(sealed_fingerprint == closure["sealed_root_fingerprint_sha256"] == custody["sealed_root"]["fingerprint_sha256"], "sealed-fingerprint")
    for name, expected in EXPECTED_SEALED.items():
        need(sha256(SEALED / name) == expected, f"sealed:{name}")
    need(closure["manifest_entries"] == 67, "manifest-count")
    need(closure["sealed_root"] == {"directories_0555": 14, "failure_absent": True, "files_0444": 70, "fingerprint_sha256": sealed_fingerprint, "lock_absent": True, "symlinks": 0}, "sealed-summary")

    terminal = json.loads((SEALED / "TERMINAL-v13.json").read_text())
    need(terminal["status"] == "PASS" and terminal["g4_eligible"] is True, "terminal")
    raw = [json.loads(line) for line in (SEALED / "rows-v13/G3-V13-RAW.jsonl").read_text().splitlines()]
    need(len(raw) == 9 and sum(row["operation_total_ns"] for row in raw) == 22948873, "raw")
    report = texts[0]
    for row in raw:
        need(str(row["operation_total_ns"]) in report, f"report-row:{row['sequence']}")
    for expected in [
        terminal["source_set_sha256"], terminal["methodology_set_sha256"], terminal["executable_sha256"],
        EXPECTED_SEALED["CAMPAIGN-v13.json"], EXPECTED_SEALED["rows-v13/G3-V13-RAW.jsonl"],
        EXPECTED_SEALED["G3-PRIMARY-ANALYSIS-v13.json"], EXPECTED_SEALED["G3-INDEPENDENT-RECOMPUTATION-v13.json"],
        terminal["normalized_ledger_sha256"], EXPECTED_SEALED["CLEANUP-v13.json"],
        EXPECTED_SEALED["ROW-CLEANUP-v13.jsonl"], EXPECTED_SEALED["STATIC-CLOSURE-v13.json"],
        EXPECTED_SEALED["PAYLOAD-MANIFEST-v13.tsv"], EXPECTED_SEALED["TERMINAL-v13.json"],
        EXPECTED_SEALED["TERMINAL-VERIFICATION-v13.txt"],
    ]:
        need(expected in report, f"report-hash:{expected}")
    need(closure["focused_tests"] == 15 and closure["workspace_tests"] == {"failed": 0, "ignored": 1, "passed": 157}, "tests")
    need(closure["history"]["v11"] == {"sealed_integrity": "PASS", "status": "HISTORICAL_REVISE"}, "v11-history")
    need(closure["history"]["v12"] == {"root_absent": True, "rows": 0, "status": "PREEXEC_REVISE"}, "v12-history")
    need(not (REPO / "target/phase4-g3-incremental-materialization-20260822-v12").exists(), "v12-root")
    need(closure["stage"] == {"g3": "PASS_SEALED", "g4": "READY_UNSTARTED", "g5": "PENDING", "g6": "PENDING", "phase4_complete": False, "platform_integrated": False, "production_integrated": False}, "stage")

    expected_verification = {
        "argv": ["python3", str(Path(__file__).resolve())],
        "stdout_path": str((HERE / "04-independent-verification.stdout").relative_to(REPO)),
        "stderr_path": str((HERE / "04-independent-verification.stderr").relative_to(REPO)),
    }
    need(closure["verification_command"] == expected_verification, "verification-command")
    need(closure["verification_path"] == str(VERIFICATION.relative_to(REPO)), "verification-path")

    input_rows = [*doc_rows, *script_rows, *stream_rows, *sealed_rows]
    output = {
        "schema": "phase4-g3-post-v13-documentation-verification-v1",
        "status": "PASS",
        "date": "2026-08-22",
        "closure_sha256": closure_hash,
        "docs_set_sha256": closure["docs_set_sha256"],
        "links_checked": 127,
        "broken_links": 0,
        "commands_total": 4,
        "commands_passed": 4,
        "first_three_command_streams_set_sha256": closure["command_streams_set_sha256"],
        "input_set_sha256": canonical_hash(input_rows),
        "sealed_root_fingerprint_sha256": sealed_fingerprint,
        "artifact_outside_sealed_payload": True,
        "sealed_payload_unchanged": True,
        "manifest_entries": 67,
        "sealed_files_0444": 70,
        "sealed_directories_0555": 14,
        "focused_tests": 15,
        "workspace_tests": {"passed": 157, "ignored": 1, "failed": 0},
        "history": closure["history"],
        "stage": closure["stage"],
        "limitations": closure["limitations"],
        "git_status_sha256": status["sha256"],
        "pre_post_git_status_equal": True,
    }
    print(json.dumps(output, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    main()
