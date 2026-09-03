#!/usr/bin/env python3
"""Generate unpublished SDK-edit claims only from three sealed eligible families."""
import argparse
import importlib.util
import json
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("sdk_edit_report", REPO / "benchmark/fs-bench-pro/generate-sdk-edit-report.py")
reporter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reporter)
custody = reporter.custody
EXPECTED = {"edit_length_preserving":12, "edit_length_changing":32, "edit_canonical_chunk_count":12}
REPOSITORY_COMMANDS = [
    ["cargo","fmt","--all","--","--check"],
    ["cargo","test","--workspace","--all-targets","--all-features","--locked"],
    ["cargo","clippy","--workspace","--all-targets","--all-features","--locked","--","-D","warnings"],
    ["git","diff","--check"],
]


def path_for(value):
    path = Path(value)
    return path if path.is_absolute() else REPO / path


def cell(value, divisor):
    return f"{value['median']/divisor:.3f} ({value['min']/divisor:.3f}–{value['max']/divisor:.3f})"


def terminal(manifest_path, require_documentation=False):
    manifest = json.loads(manifest_path.read_text())
    assert manifest["schema"] == "fs-bench-pro-sdk-edit-release-inputs-v1"
    inputs = manifest["families"]
    assert len(inputs) == 3 and {item["family_id"] for item in inputs} == set(EXPECTED)
    families, all_rows, all_ids, receipt_count, subproof_count = {}, [], set(), 0, 0
    common_source = common_environment = common_prepared = None
    for item in inputs:
        root = path_for(item["path"]).resolve()
        assert custody.sha(root / "evidence.sha256") == item["manifest_sha256"], "family manifest identity"
        custody.verify_manifest(root)
        status = json.loads((root / "run-status.json").read_text())
        assert status["status"] == "pass" and status["admission_eligible"] is True, "family is not eligible"
        source, fixtures, _ = reporter.custody_validation(root, require_ending=True)
        family, registry, rows, failures, summary = reporter.performance_validation(root, write_summary=False)
        assert family == item["family_id"] and len(registry) == EXPECTED[family]
        assert len(rows) == EXPECTED[family] * 10
        receipts, verification = reporter.verification_validation(root, family, registry, rows, failures, write=False)
        assert not failures, "\n".join(failures)
        assert summary == json.loads((root / "performance/summary.json").read_text()), "derived performance summary changed"
        assert verification == json.loads((root / "verification/summary.json").read_text()), "derived verification summary changed"
        assert not (all_ids & registry.keys()), "cross-family scenario collision"
        all_ids.update(registry)
        all_rows.extend(rows)
        receipt_count += len(receipts)
        subproof_count += verification["source_subproofs"]
        identity = {arm:{key:source[arm][key] for key in (
            "revision","tree","source_seal","product_seal","harness_seal","binary_sha256","image_id",
            "workload_sha256","report_generator_sha256","custody_helper_sha256","release_generator_sha256","contract_sha256","build_configuration",
            "preparation_compatibility_sha256")} for arm in ("baseline","candidate")}
        environment = json.loads((root / "environment/host-runtime.json").read_text())
        prepared = {fixture["fixture_bytes"]:json.loads((root / f"environment/prepared-cache-{fixture['fixture_bytes']}.json").read_text())["store_sha256"] for fixture in fixtures}
        if common_source is None:
            common_source, common_environment, common_prepared = identity, environment, prepared
        else:
            assert identity == common_source, "families use different source/build/image identities"
            assert environment == common_environment, "families use different controlled environments"
            assert prepared == common_prepared, "families use different prepared input artifacts"
        assert identity["candidate"]["release_generator_sha256"] == custody.sha(Path(__file__)), "release generator custody"
        families[family] = {"root":root, "input":item, "summary":summary, "rows":rows, "receipts":receipts}
    assert len(all_ids) == 56 and len(all_rows) == 560 and len({row["row_id"] for row in all_rows}) == 560
    assert {arm:sum(row["source_arm"]==arm for row in all_rows) for arm in ("baseline","candidate")} == {"baseline":280,"candidate":280}
    assert receipt_count == 56 and subproof_count == 112
    if "repository_gates" not in manifest:
        assert not require_documentation, "final repository and documentation gates required"
        return families, common_source, common_environment, manifest
    gates_input = manifest["repository_gates"]
    gates_root = path_for(gates_input["path"])
    assert custody.sha(gates_root / "evidence.sha256") == gates_input["manifest_sha256"]
    custody.verify_manifest(gates_root)
    gates = json.loads((gates_root / "run-status.json").read_text())
    assert gates["schema"] == "fs-bench-pro-sdk-edit-repository-gates-v1" and gates["status"] == "pass"
    assert gates["measured_source"]["revision"] == common_source["candidate"]["revision"]
    assert all(gates["measured_source"][key] == common_source["candidate"][key] for key in ("tree","source_seal","product_seal","harness_seal"))
    assert gates["documentation_bridge"] == custody.documentation_bridge(common_source["candidate"]["revision"], gates["source"]["revision"])
    commands = json.loads((gates_root / "commands.json").read_text())
    assert [command["argv"] for command in commands] == REPOSITORY_COMMANDS and all(command["exit_code"]==0 for command in commands)
    if "documentation_gates" in manifest:
        document_input = manifest["documentation_gates"]
        document_root = path_for(document_input["path"])
        assert custody.sha(document_root / "evidence.sha256") == document_input["manifest_sha256"]
        custody.verify_manifest(document_root)
        document = json.loads((document_root / "run-status.json").read_text())
        assert document["schema"] == "fs-bench-pro-sdk-edit-repository-gates-v1" and document["status"] == "pass"
        assert document["measured_source"]["revision"] == common_source["candidate"]["revision"]
        assert document["documentation_bridge"] == custody.documentation_bridge(common_source["candidate"]["revision"], document["source"]["revision"])
        document_commands = json.loads((document_root / "commands.json").read_text())
        assert [command["argv"] for command in document_commands] == REPOSITORY_COMMANDS and all(command["exit_code"]==0 for command in document_commands)
        custody.documentation_bridge(document["source"]["revision"], custody.output("git","rev-parse","HEAD"), evidence_only=True)
    else:
        assert not require_documentation, "documentation-complete repository gates required"
    return families, common_source, common_environment, manifest


def render(families, source, environment):
    lines = ["# LayerFS 0.1.2 SDK-only edit candidate evidence", "",
             "> Unreleased candidate evidence. Final issue closure requires the separate documentation-complete custody gate; this report does not tag or publish v0.1.2 or close #12.", "",
             f"Measured candidate: {source['candidate']['revision']}. Authentic baseline: {source['baseline']['revision']}.",
             "", "All three families pass as one 56-ID matrix: 280 baseline + 280 candidate performance rows, 56 aggregate verifier receipts, and 112 source-arm subproofs.",
             "", "Candidate medians satisfy the user-approved accepted 20/20/30 ms edit/Commit/combined ceilings, size parity, matched-operation parity, no-amplification, memory, correctness, cleanup, and custody gates. Nominal targets remain 10/10/20 ms; each family report distinguishes nominal-pass from accepted-with-tolerance. Combined latency is independently capped at 30 ms. Baseline latency is diagnostic; baseline correctness and resources remain binding.",
             "", "Measurements cover exact 1/10/100/500 MiB tiers on the recorded host/Docker Desktop/FUSE environment. No empirical claim extends above 500 MiB.",
             "", "Only pristine input Stores are reused. Every sample receives a fresh writable clone, worker, Workspace, and container. Clone/hash conditioning is not a cold-cache claim. The user-approved ack-window-v1 profile brackets cgroup observation by acknowledgments before Edit and after Commit; exact cgroup phase attribution is unavailable. Native whole-worker/container high-water marks conservatively bound total peaks; category maxima and transient swap checks are sampled observations, not continuous proofs. Actual windows and sampling gaps are retained without clock-precision abort gates. Process RSS, cgroup memory, and spool disk remain separate.",
             "", "## Evidence manifests", "", "| Family | IDs | Rows | Aggregate proofs | Manifest SHA-256 |", "| --- | ---: | ---: | ---: | --- |"]
    for family in EXPECTED:
        item = families[family]
        relative = item["root"].relative_to(REPO).as_posix()
        lines.append(f"| [{family}](../../{relative}/report.md) | {EXPECTED[family]} | {len(item['rows'])} | {len(item['receipts'])} | {item['input']['manifest_sha256']} |")
    for family in EXPECTED:
        item = families[family]
        relative = item["root"].relative_to(REPO).as_posix()
        lines += ["", f"## {family}", "", f"Raw: [performance](../../{relative}/performance/raw.jsonl), [verification aggregates](../../{relative}/verification/raw.jsonl), [source subproofs](../../{relative}/verification/subproofs.jsonl).",
                  "", "| Operation | MiB | Arm | n | Edit ms median (min–max) | Commit ms median (min–max) | Combined ms median (min–max) |",
                  "| --- | ---: | --- | ---: | ---: | ---: | ---: |"]
        for scenario in item["summary"]["scenarios"]:
            for arm in ("baseline","candidate"):
                metrics = scenario[arm]
                lines.append(f"| {scenario['operation_key']} | {scenario['fixture_bytes']//reporter.MIB} | {arm} | 5 | {cell(metrics['edit_call_ns'],1e6)} | {cell(metrics['commit_call_ns'],1e6)} | {cell(metrics['edit_commit_ns'],1e6)} |")
        lines += ["", "| Operation | MiB | Arm | RSS phase MiB median (min–max) | RSS incremental MiB median (min–max) | Cgroup phase MiB median (min–max) | Cgroup incremental MiB median (min–max) |",
                  "| --- | ---: | --- | ---: | ---: | ---: | ---: |"]
        for scenario in item["summary"]["scenarios"]:
            for arm in ("baseline","candidate"):
                metrics = scenario[arm]
                lines.append(f"| {scenario['operation_key']} | {scenario['fixture_bytes']//reporter.MIB} | {arm} | {cell(metrics['rss_phase_peak_bytes'],reporter.MIB)} | {cell(metrics['rss_incremental_peak_bytes'],reporter.MIB)} | {cell(metrics['cgroup_window_peak_bytes'],reporter.MIB)} | {cell(metrics['cgroup_window_incremental_peak_bytes'],reporter.MIB)} |")
    lines += ["", "## Broad one-operation diagnostic", "", "Nonbinding target: combined-median spread ≤5 ms; alert above 7 ms. This does not replace byte-matched parity gates.",
              "", "| MiB | Minimum operation | Maximum operation | Combined spread ms | Diagnostic |", "| ---: | --- | --- | ---: | --- |"]
    for size in reporter.SIZES:
        cells = [scenario for item in families.values() for scenario in item["summary"]["scenarios"] if scenario["fixture_bytes"]==size]
        ordered = sorted(cells,key=lambda row:row["candidate"]["edit_commit_ns"]["median"])
        low,high = ordered[0],ordered[-1]
        spread = high["candidate"]["edit_commit_ns"]["median"]-low["candidate"]["edit_commit_ns"]["median"]
        label = "target-pass" if spread<=5_000_000 else "alert" if spread>7_000_000 else "diagnostic-target-miss"
        lines.append(f"| {size//reporter.MIB} | {low['operation_key']} | {high['operation_key']} | {spread/1e6:.3f} | {label} |")
        if spread>7_000_000:
            edit_delta=high["candidate"]["edit_call_ns"]["median"]-low["candidate"]["edit_call_ns"]["median"]
            commit_delta=high["candidate"]["commit_call_ns"]["median"]-low["candidate"]["commit_call_ns"]["median"]
            lines += ["", f"At {size//reporter.MIB} MiB, the broad spread consists of {edit_delta/1e6:.3f} ms edit-phase and {commit_delta/1e6:.3f} ms Commit-phase median differences. This diagnostic mixes 0/2/4/64 KiB replacement work; the separate matched-work cohorts remain binding.", ""]
    lines += ["", "## Custody and archival disposition", "",
              f"Candidate product seal: {source['candidate']['product_seal']}.",
              f"Common harness seal: {source['candidate']['harness_seal']}.",
              f"Host: {environment['cpu']} / {environment['os']}; Docker server {environment['docker_server_version']}.",
              "", "Final admission requires the separately sealed documentation-complete repository gate, tied to the exact measured candidate through the unchanged-source bridge. This draft alone is not issue-closure evidence. Earlier POSIX/FUSE same-count/count-changing rows remain immutable archival evidence only; they are not members, baselines, comparators, or sources of these SDK claims.",
              "", "Universal edit-engine and Store-footprint history remains supporting context, not a substitute for this complete proof. v0.1.2 remains untagged/unpublished; #12 remains open for a later release-finalization decision."]
    draft = ["# LayerFS 0.1.2 — unpublished evidence draft", "",
             "SDK-only edit candidate evidence is recorded in [benchmark-results.md](benchmark-results.md); final issue closure also requires documentation-complete custody.",
             "", "This is not a GitHub Release announcement. No v0.1.2 tag or release is created, and parent #12 remains open.",
             "", "The exact 56-ID, 560-row, 56-aggregate/112-subproof evidence covers 1/10/100/500 MiB only, with singular public SDK edit → Commit-return timing and strict latency/parity/resource/correctness/custody gates.",
             "", "Historical POSIX/FUSE edit evidence is archival and is not an SDK performance claim."]
    return "\n".join(lines)+"\n", "\n".join(draft)+"\n"


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--manifest",type=Path,default=HERE/"sdk-edit-evidence.json")
    parser.add_argument("--write",action="store_true")
    parser.add_argument("--check",action="store_true")
    parser.add_argument("--verify-all",action="store_true",help="all artifacts are always verified")
    parser.add_argument("--terminal-dir",type=Path)
    args=parser.parse_args()
    assert not(args.write and args.check)
    if args.check:
        custody.require_clean()
    families,source,environment,manifest=terminal(args.manifest,require_documentation=args.check or bool(args.terminal_dir))
    report,draft=render(families,source,environment)
    outputs={HERE/"benchmark-results.md":report,HERE/"github-release.md":draft}
    if args.write:
        for path,text in outputs.items(): path.write_text(text)
    elif args.check:
        for path,text in outputs.items(): assert path.read_text()==text,f"stale generated report: {path}"
    else:
        print(report,end="")
    if args.terminal_dir:
        assert not args.terminal_dir.exists(),"terminal directory exists"
        args.terminal_dir.mkdir(parents=True)
        custody.write_json(args.terminal_dir/"inputs.json",manifest)
        custody.write_json(args.terminal_dir/"run-status.json",{
            "schema":"fs-bench-pro-sdk-edit-terminal-v1","status":"pass","admission_eligible":True,
            "registered_ids":56,"baseline_rows":280,"candidate_rows":280,"performance_rows":560,
            "aggregate_verifier_receipts":56,"source_arm_subproofs":112,"source":source,
            "repository_gates_status":"pass","documentation_gates_status":"pass","publication_action":"none"})
        (args.terminal_dir/"report.md").write_text(report)
        custody.seal(args.terminal_dir)
        custody.verify_manifest(args.terminal_dir)


if __name__=="__main__":
    main()
