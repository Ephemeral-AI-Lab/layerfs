#!/usr/bin/env python3
"""Independent read-only verification of the post-v13 v2 closure."""

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
CLOSURE = BASE / "G3-POST-V13-DOCUMENTATION-CLOSURE-v2.json"
VERIFICATION = BASE / "G3-POST-V13-DOCUMENTATION-VERIFICATION-v2.txt"
V1_CLOSURE = BASE / "G3-POST-V13-DOCUMENTATION-CLOSURE-v1.json"
V1_VERIFICATION = BASE / "G3-POST-V13-DOCUMENTATION-VERIFICATION-v1.txt"
V1_DIR = BASE / "post-v13-documentation-v1"
SEALED_PARENT = REPO / "target/phase4-g3-incremental-materialization-20260822-v13"
SEALED = SEALED_PARENT / "results-v13"
V1_CLOSURE_SHA256 = "5a20c669cba588de9dc08343fdc3432441cb6c8765c81e70a9d9179399165dd8"
V1_VERIFICATION_SHA256 = "6e8e97934a099d96ded2cffc5bf42eaa619a00feba89b6fc4e1b096e74f3a77a"
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
SEALED_HASHES = {
    "CAMPAIGN-v13.json": "70be7a26ada3f0c378faed061819338620cc43708c3e5226aff3a360b5eb7e88",
    "rows-v13/G3-V13-RAW.jsonl": "3d2b40da82f612441cf1af88ee89f2d8c79b139c75818d6c7e2a5488cbad956c",
    "G3-PRIMARY-ANALYSIS-v13.json": "b28003f59dcf3fbfa6a585762d70cdc0beae0b4c81ec51904327d388452820d7",
    "G3-INDEPENDENT-RECOMPUTATION-v13.json": "2f137bb1116d1637656d1c89777dcb9e1291e04899f6710a000e5a6933419ace",
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
        raise RuntimeError(f"v2 independent verification failed: {label}")


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
    need(closure["schema"] == "phase4-g3-post-v13-documentation-closure-v2", "schema")
    need(closure["status"] == "PASS", "status")
    need(closure["artifact_outside_sealed_payload"] is True, "artifact-location")
    need(closure["sealed_payload_unchanged"] is True, "sealed-unchanged")
    need(closure["commands_planned"] == 3 and closure["commands_reused"] == 2, "command-plan")
    need(closure["commands_fresh_executed"] == 1 and closure["commands_rerun"] == 0, "fresh-plan")
    need(not any(closure[key] for key in ["build_rerun", "tests_rerun", "campaign_rerun", "static_closure_rerun", "finalizer_rerun"]), "forbidden-reruns")

    need(sha256(V1_CLOSURE) == V1_CLOSURE_SHA256, "v1-closure-hash")
    need(sha256(V1_VERIFICATION) == V1_VERIFICATION_SHA256, "v1-verification-hash")
    v1 = json.loads(V1_CLOSURE.read_text())
    v1_verification = json.loads(V1_VERIFICATION.read_text())
    need(v1["status"] == v1_verification["status"] == "REVISE", "v1-status")
    need(v1_verification["failure_class"] == "documentation-verifier-newline-normalization-defect", "v1-defect")
    need(closure["v1_revise"] == {"closure_sha256": V1_CLOSURE_SHA256, "commands_rerun": 0, "failure_class": "documentation-verifier-newline-normalization-defect", "status": "REVISE", "verification_sha256": V1_VERIFICATION_SHA256}, "v1-binding")

    expected_reused = [
        ["cargo", "fmt", "--all", "--", "--check"],
        ["git", "diff", "--check", "--", *DOCS],
    ]
    reused_streams = []
    for command, expected in zip(closure["reused_commands"], expected_reused):
        need(command["argv"] == expected and command["exit_code"] == 0, "reused-command")
        need(command["executed_in_v2"] is False, "reused-not-executed")
        for kind in ["stdout", "stderr"]:
            path = REPO / command[f"{kind}_path"]
            need(sha256(path) == command[f"{kind}_sha256"] and path.stat().st_size == 0, "reused-stream")
            reused_streams.append(file_row(path))
    need(reused_streams == closure["reused_command_streams"], "reused-stream-rows")
    need(canonical_hash(reused_streams) == closure["reused_command_streams_set_sha256"], "reused-stream-set")

    need(len(closure["fresh_commands"]) == 1, "fresh-command-count")
    command = closure["fresh_commands"][0]
    need(command["argv"] == ["python3", str(HERE / "documentation_custody_status_v2.py")], "custody-argv")
    need(command["exit_code"] == 0 and command["executed_in_v2"] is True, "custody-result")
    fresh_streams = []
    for kind in ["stdout", "stderr"]:
        path = REPO / command[f"{kind}_path"]
        need(sha256(path) == command[f"{kind}_sha256"], f"custody-{kind}-hash")
        need(path.stat().st_size == command[f"{kind}_size_bytes"] and mode(path) == "0644", f"custody-{kind}-custody")
        fresh_streams.append(file_row(path))
    need(fresh_streams == closure["fresh_command_streams"], "fresh-stream-rows")
    need(canonical_hash(fresh_streams) == closure["fresh_command_streams_set_sha256"], "fresh-stream-set")
    need(fresh_streams[1]["size_bytes"] == 0, "custody-stderr")
    custody = json.loads((REPO / fresh_streams[0]["path"]).read_text())
    need(custody["status"] == "PASS", "custody-status")

    wrapper = (HERE / "documentation_custody_status_v2.py").read_text()
    need('text.count(OLD) != 1 or NEW in text' in wrapper, "single-repair-guard")
    need('re.sub(r"\\\\s+", " ", all_text)' in wrapper, "whitespace-repair")
    need(closure["inherited_v1_custody_verifier_sha256"] == "a124dd9e761efe3fa01bb537ba1f5f0970750b994a360eabc5674ab6c5d131ca", "inherited-verifier")
    script_rows = [file_row(REPO / row["path"]) for row in closure["input_scripts"]]
    need(script_rows == closure["input_scripts"], "scripts")
    need(canonical_hash(script_rows) == closure["input_scripts_set_sha256"], "script-set")

    doc_rows = []
    links = 0
    for name, expected in zip(DOCS, DOC_HASHES):
        path = REPO / name
        data = path.read_bytes()
        text = data.decode()
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
    need(doc_rows == closure["docs"] == custody["docs"] == v1["docs"], "docs")
    need(canonical_hash(doc_rows) == closure["docs_set_sha256"] == custody["docs_set_sha256"], "docs-set")

    status_text = subprocess.check_output(["git", "status", "--short"], cwd=REPO, text=True)
    status = {"lines": status_text.splitlines(), "line_count": len(status_text.splitlines()), "sha256": hashlib.sha256(status_text.encode()).hexdigest()}
    need(status == closure["pre_git_status"] == closure["post_git_status"] == v1["pre_git_status"] == v1["post_git_status"], "git-status")
    need(closure["pre_post_git_status_equal"] is True and closure["pre_post_docs_equal"] is True, "pre-post")

    sealed_files = sorted(path for path in SEALED.rglob("*") if path.is_file())
    sealed_rows = [file_row(path, SEALED_PARENT) for path in sealed_files]
    sealed_fingerprint = canonical_hash(sealed_rows)
    need(len(sealed_files) == 70 and all(mode(path) == "0444" for path in sealed_files), "sealed-files")
    sealed_dirs = [SEALED_PARENT, SEALED, *sorted(path for path in SEALED.rglob("*") if path.is_dir())]
    need(len(sealed_dirs) == 14 and all(mode(path) == "0555" for path in sealed_dirs), "sealed-dirs")
    need(sealed_fingerprint == closure["sealed_root_fingerprint_sha256"] == custody["sealed_root"]["fingerprint_sha256"] == v1["sealed_root_fingerprint_sha256"], "sealed-fingerprint")
    for name, expected in SEALED_HASHES.items():
        need(sha256(SEALED / name) == expected, f"sealed:{name}")
    need(closure["manifest_entries"] == 67 and closure["rows_checked"] == 9, "sealed-counts")
    need(closure["focused_tests"] == 15 and closure["workspace_tests"] == {"failed": 0, "ignored": 1, "passed": 157}, "tests")
    need(closure["history"] == custody["history"] and closure["stage"] == custody["stage"], "status-output")

    expected_verification = {
        "argv": ["python3", str(Path(__file__).resolve())],
        "stdout_path": str((HERE / "04-independent-verification.stdout").relative_to(REPO)),
        "stderr_path": str((HERE / "04-independent-verification.stderr").relative_to(REPO)),
    }
    need(closure["verification_command"] == expected_verification, "verification-command")
    need(closure["verification_path"] == str(VERIFICATION.relative_to(REPO)), "verification-path")

    input_rows = [*doc_rows, *script_rows, *reused_streams, *fresh_streams, *sealed_rows]
    output = {
        "schema": "phase4-g3-post-v13-documentation-verification-v2",
        "status": "PASS",
        "date": "2026-08-22",
        "closure_sha256": hashlib.sha256(closure_bytes).hexdigest(),
        "docs_set_sha256": closure["docs_set_sha256"],
        "links_checked": 127,
        "broken_links": 0,
        "commands_total": 4,
        "commands_passed": 4,
        "commands_reused": 2,
        "commands_fresh": 2,
        "commands_rerun": 0,
        "reused_command_streams_set_sha256": closure["reused_command_streams_set_sha256"],
        "fresh_custody_streams_set_sha256": closure["fresh_command_streams_set_sha256"],
        "input_set_sha256": canonical_hash(input_rows),
        "v1_revise": closure["v1_revise"],
        "only_v2_change": closure["only_v2_change"],
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
