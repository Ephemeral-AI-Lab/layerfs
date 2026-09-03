#!/usr/bin/env python3
"""Final, separately identified consumer of immutable SDK-edit collections.

The frozen producer/reader are unchanged. This consumer corrects one unavailable-
attribution field alias in memory and applies explicitly recorded acceptance
amendments. It never modifies or recollects source rows.
"""
import argparse
import copy
import importlib.util
import json
import sys
from pathlib import Path

sys.dont_write_bytecode = True

REPO = Path(__file__).resolve().parents[4]
HERE = Path(__file__).resolve().parent
BASELINE = "dc7aeff9a7e4f9e849a48022142f86801273f0bd"
CANDIDATE = "3337728e9846a200d7a5cc08d076de18f1d5436c"
HARNESS = "5d2d2995aca098e1e3c8878b2e45d5cd460cdc8b6dfff8681e6cc0df93561ec4"
REPORTER = "da606681f5c4222e724eb6273c2417f7ec3960cd15d12a25082228550f25eb19"
EXPECTED = {"edit_length_preserving":12,"edit_length_changing":32,"edit_canonical_chunk_count":12}
REVIEWED = {
    "candidate delete-middle-4k edit_call_ns size parity": [5221333,2715208,2698209,2649375],
    "candidate replace-shrink-middle-4k-to-2k edit_call_ns size parity": [1589125,1649209,2605292,3700208],
    "candidate delete 1048576 edit_call_ns": [5221333,2736875],
}
spec = importlib.util.spec_from_file_location("frozen_sdk_report", REPO/"benchmark/fs-bench-pro/generate-sdk-edit-report.py")
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)
custody = report.custody
frozen_clock_validation = report.clock_validation


def alias_validation(row, failures, label):
    old, new = "exact_cgroup_window_attribution", "exact_cgroup_phase_attribution"
    if old in row:
        assert row["resource_observation_profile"] == "ack-window-v1"
        assert row["category_peak_scope"] == "sampled-window-not-continuous"
        assert row[old] == "unavailable"
        assert new not in row or row[new] == row[old], "conflicting attribution fields"
        row = {**row, new:row[old]}
    frozen_clock_validation(row, failures, label)


def load(path):
    return json.loads(Path(path).read_text())


def classify(inputs, verification):
    assert custody.sha(Path(report.__file__)) == REPORTER
    assert inputs["candidate"] == CANDIDATE and inputs["baseline"] == BASELINE
    assert inputs["policy"]["edit_size_parity_binding"] is True
    assert inputs["policy"]["commit_combined_size_parity"] == "diagnostic-user-approved"
    assert len(inputs["families"]) == 3 and {item["family_id"] for item in inputs["families"]} == set(EXPECTED)
    results, all_ids, total_rows, total_proofs = {}, set(), 0, 0
    common_source = common_host = common_prepared = None
    for item in inputs["families"]:
        root = REPO/item["path"]
        assert custody.sha(root/"evidence.sha256") == item["manifest_sha256"]
        custody.verify_manifest(root)
        assert custody.sha(root/"performance/raw.jsonl") == item["performance_raw_sha256"]
        source, fixtures, _ = report.custody_validation(root, require_ending=True)
        assert source["baseline"]["revision"] == BASELINE and source["candidate"]["revision"] == CANDIDATE
        assert source["baseline"]["harness_seal"] == source["candidate"]["harness_seal"] == HARNESS
        identity = {arm:source[arm] for arm in ("baseline","candidate")}
        host = load(root/"environment/host-runtime.json")
        prepared = {entry["fixture_bytes"]:load(root/f"environment/prepared-cache-{entry['fixture_bytes']}.json")["store_sha256"] for entry in fixtures}
        if common_source is None:
            common_source,common_host,common_prepared = identity,host,prepared
        else:
            assert (identity,host,prepared) == (common_source,common_host,common_prepared)
        report.clock_validation = frozen_clock_validation
        family, registry, rows, original_failures, original_summary = report.performance_validation(root, write_summary=False)
        assert original_summary == load(root/"performance/summary.json")
        report.clock_validation = alias_validation
        _, registry2, rows2, corrected_failures, corrected_summary = report.performance_validation(root, write_summary=False)
        assert registry == registry2 and rows == rows2
        for key in ("scenarios","size_parity","matched_operation_parity","paired_controls","latency_policy"):
            assert original_summary[key] == corrected_summary[key], f"numeric change: {key}"
        allowed_alias = {f"{row['row_id']} observation scope" for row in rows if "exact_cgroup_window_attribution" in row}
        assert set(original_failures)-set(corrected_failures) <= allowed_alias
        assert not (set(corrected_failures)-set(original_failures))
        diagnostic = {f"candidate {entry['operation_key']} {entry['metric']} size parity" for entry in corrected_summary["size_parity"] if entry["source_arm"]=="candidate" and entry["metric"] in ("commit_call_ns","edit_commit_ns")}
        assert inputs["policy"]["matched_commit_combined_parity"] == "diagnostic-user-approved"
        diagnostic.update(f"candidate {entry['cohort']} {entry['fixture_bytes']} {entry['metric']}" for entry in corrected_summary["matched_operation_parity"] if entry["metric"] in ("commit_call_ns","edit_commit_ns"))
        # Every further exception must be explicitly named in the approved policy;
        # never suppress unknown numerical/resource/correctness findings.
        exceptions = set(inputs["policy"].get("reviewed_failure_exceptions", []))
        assert exceptions == set(REVIEWED), "only the three explicitly reviewed discrepancies are authorized"
        if family == "edit_length_changing":
            observed = {f"candidate {entry['operation_key']} {entry['metric']} size parity":entry["medians"] for entry in corrected_summary["size_parity"] if entry["source_arm"]=="candidate"}
            observed.update({f"candidate {entry['cohort']} {entry['fixture_bytes']} {entry['metric']}":entry["medians"] for entry in corrected_summary["matched_operation_parity"]})
            assert all(observed[name] == values for name,values in REVIEWED.items()), "reviewed data changed"
        final_failures = [failure for failure in corrected_failures if failure not in diagnostic and failure not in exceptions]
        assert family == item["family_id"] and len(registry) == EXPECTED[family]
        assert len(rows) == EXPECTED[family]*10 and not (all_ids & set(registry))
        all_ids.update(registry); total_rows += len(rows)
        proof_summary, aggregates, verification_removed = None, [], []
        if verification:
            assert custody.sha(root/"verification/subproofs.jsonl") == item["verification_subproofs_sha256"]
            report.clock_validation = frozen_clock_validation
            original_v = list(original_failures)
            original_aggregates, original_v_summary = report.verification_validation(root,family,registry,rows,original_v,write=False)
            assert original_v_summary == load(root/"verification/summary.json")
            report.clock_validation = alias_validation
            corrected_v = list(original_failures)
            aggregates, proof_summary = report.verification_validation(root,family,registry,rows,corrected_v,write=False)
            assert original_aggregates == aggregates, "source proof or aggregate changed"
            aliases = {f"verification {scenario} {arm} observation scope" for scenario in registry for arm in ("baseline","candidate")}
            verification_removed = sorted(set(original_v)-set(corrected_v))
            assert set(verification_removed) <= aliases and not (set(corrected_v)-set(original_v))
            assert corrected_v[:len(original_failures)] == original_failures
            final_failures.extend(corrected_v[len(original_failures):])
            assert len(aggregates)==EXPECTED[family] and proof_summary["source_subproofs"]==EXPECTED[family]*2
            total_proofs += proof_summary["source_subproofs"]
            aggregates = copy.deepcopy(aggregates)
            for aggregate in aggregates:
                aggregate["status"] = "pass" if not final_failures else "fail"
                aggregate["classification_consumer"] = "frozen-validator-plus-explicit-policy-and-attribution-alias-v1"
        results[family] = {"source_run":str(root.relative_to(REPO)),"performance_rows":len(rows),"verification_complete":verification,
            "original_failures":original_failures,"alias_findings_removed":sorted(set(original_failures)-set(corrected_failures)),
            "commit_combined_size_findings_now_diagnostic":sorted(set(corrected_failures)&diagnostic),
            "explicitly_reviewed_exceptions":sorted(set(corrected_failures)&exceptions),
            "verification_alias_findings_removed":verification_removed,"remaining_failures":final_failures,
            "status":"pass" if not final_failures else "fail","statistics":corrected_summary,"derived_verification_aggregates":aggregates}
    assert len(all_ids)==56 and total_rows==560
    assert not verification or total_proofs==112
    return {"schema":"layerfs-sdk-edit-final-consumer-v1","consumer_sha256":custody.sha(__file__),
            "producer_reporter_sha256":REPORTER,"producer_harness_sha256":HARNESS,"policy":inputs["policy"],
            "source":common_source,"host":common_host,"registered_ids":56,"performance_rows":total_rows,
            "source_subproofs":total_proofs,"verification_complete":verification,"families":results,
            "status":"pass" if all(value["status"]=="pass" for value in results.values()) else "fail"}


def render(result):
    lines=["# LayerFS SDK-edit final classification", "", f"Status: **{result['status']}**; verification complete: **{result['verification_complete']}**.", "",
        "This is a separately identified consumer of unchanged raw collections. It recognizes the producer's unavailable-attribution field alias in memory and applies the explicitly approved policy. Original producer classifications remain retained, not rewritten.", "",
        "Nominal Edit/Commit/combined targets: 10/10/20 ms. Accepted ceilings: 20/20/30 ms. Edit-only size and matched-operation parity remain binding except for the three explicitly reviewed LC discrepancies below. Commit/combined size and matched-operation spreads are diagnostic. No size-independent Commit claim is made.", "",
        "Reviewed exceptions (strict results retained): delete-middle Edit cross-size spread 2.571958 ms; replace-shrink Edit cross-size spread 2.111083 ms; delete versus truncate at 1 MiB Edit spread 2.484458 ms. These are accepted by explicit user review, not represented as passing the original 2 ms rule. No arbitrary future exception is authorized.", "",
        "Memory uses ack-window-v1: native whole-worker/container peaks are conservative lifetime bounds; category maxima and transient swap observations are sampled, not continuous proofs. Exact cgroup edit-phase attribution is unavailable. No old failed/incomplete campaign is pooled here."]
    lines += ["", "Admission eligibility (including repository gates): **"+str(result.get("admission_eligible",False))+"**.",
              "", "Raw bundles, source identities, and SHA-256 manifests are pinned in [inputs.json](inputs.json). Full machine-readable statistics and original findings are in [classification.json](classification.json).",
              "", "Performance collection finished for all three families before verification. One baseline 10 MiB zero-extension verifier returned InvalidRequest; its container exited 0 without OOM. The failed attempt is retained, six missing proofs passed on retry, and the original 58-proof prefix and all performance bytes were preserved. Root cause of that isolated control error remains unproven."]
    for family,value in result["families"].items():
        lines += ["",f"## {family}","",f"Performance rows: {value['performance_rows']}; final classification: {value['status']}.","",
            f"[Raw performance](../../{family.replace('_','-')}/terminal-3337728e/performance/raw.jsonl) · [Verification subproofs](../../{family.replace('_','-')}/terminal-3337728e/verification/subproofs.jsonl) · [Manifest](../../{family.replace('_','-')}/terminal-3337728e/evidence.sha256)", "",
            "| Operation | MiB | Arm | N | Edit median (min–max) ms | Commit median (min–max) ms | Combined median (min–max) ms | Latency status |","| --- | ---: | --- | ---: | ---: | ---: | ---: | --- |"]
        for scenario in value["statistics"]["scenarios"]:
            for arm in ("baseline","candidate"):
                metrics=scenario[arm]
                cells=[f"{metrics[key]['median']/1e6:.3f} ({metrics[key]['min']/1e6:.3f}–{metrics[key]['max']/1e6:.3f})" for key in report.METRICS]
                status=scenario['candidate_latency_status'] if arm=='candidate' else 'directional comparator'
                lines.append(f"| {scenario['operation_key']} | {scenario['fixture_bytes']//report.MIB} | {arm} | {metrics['edit_call_ns']['samples']} | {' | '.join(cells)} | {status} |")
        lines += ["","Memory cells are median (min–max), MiB, N=5 performance workers per arm. Native peaks bound whole lifetimes; window/category observations are sampled, not exact-phase or continuous maxima.","",
                  "| Operation | MiB | Arm | Native RSS lifetime peak | Native cgroup lifetime peak | Sampled cgroup window peak | Sampled cgroup window increment | Sampled RSS increment | Sampled dirty/writeback increment |",
                  "| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |"]
        for scenario in value["statistics"]["scenarios"]:
            for arm in ('baseline','candidate'):
                metrics=scenario[arm]
                cells=[f"{metrics[key]['median']/report.MIB:.3f} ({metrics[key]['min']/report.MIB:.3f}–{metrics[key]['max']/report.MIB:.3f})" for key in ("process_lifetime_peak_rss_bytes","cgroup_lifetime_peak_bytes","cgroup_window_peak_bytes","cgroup_window_incremental_peak_bytes","rss_incremental_peak_bytes","dirty_writeback_incremental_peak_bytes")]
                lines.append(f"| {scenario['operation_key']} | {scenario['fixture_bytes']//report.MIB} | {arm} | {' | '.join(cells)} |")
        lines += ["","Remaining findings:",""] + ([f"- {failure}" for failure in value["remaining_failures"]] or ["- None under the explicitly recorded final policy."])
    return "\n".join(lines)+"\n"


def validate_final_gate(inputs, result):
    assert result["status"]=="pass" and result["verification_complete"] and result["source_subproofs"]==112
    policy=inputs["policy"]
    assert policy["spec_path"]==custody.CONTRACT
    assert custody.sha(REPO/policy["spec_path"])==policy["spec_sha256"]
    item=inputs["repository_gate"];root=REPO/item["path"]
    assert custody.sha(root/"evidence.sha256")==item["manifest_sha256"]
    custody.verify_manifest(root)
    gate=load(root/"run-status.json")
    assert gate["status"]=="pass" and gate["schema"]=="fs-bench-pro-sdk-edit-repository-gates-v1"
    commands=load(root/"commands.json")
    expected=[["cargo","fmt","--all","--","--check"],
              ["cargo","test","--workspace","--all-targets","--all-features","--locked"],
              ["cargo","clippy","--workspace","--all-targets","--all-features","--locked","--","-D","warnings"],
              ["git","diff","--check"]]
    assert [command["argv"] for command in commands]==expected and all(command["exit_code"]==0 for command in commands)
    assert gate["source"]==custody.source_identity(gate["source"]["revision"])
    for key in ("source_seal","product_seal","harness_seal","cargo_lock_sha256","workload_sha256",
                "report_generator_sha256","custody_helper_sha256","release_generator_sha256","preparation_compatibility_sha256"):
        assert gate["source"][key]==result["source"]["candidate"][key], f"final compiled-source change: {key}"
    assert gate["source"]["contract_sha256"]==policy["spec_sha256"]
    head=custody.output("git","rev-parse","HEAD")
    for start,end,evidence_only in ((CANDIDATE,gate["source"]["revision"],False),(gate["source"]["revision"],head,True)):
        custody.output("git","merge-base","--is-ancestor",start,end)
        paths=custody.output("git","diff","--name-only",start,end).splitlines()
        allowed={"release-notes/0.1.2/sdk-edit-evidence.json"} if evidence_only else custody.DOCUMENTATION_FILES|{custody.CONTRACT}
        assert all(path in allowed or path.startswith(custody.EVIDENCE_PREFIXES) for path in paths), "final documentation/evidence scope"
    result.update(admission_eligible=True,repository_gates_status="pass",documentation_revision=gate["source"]["revision"],
                  repository_gate=item,publication_action="none",final_acceptance_spec_sha256=policy["spec_sha256"])


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--inputs",type=Path,default=HERE/"inputs.json")
    parser.add_argument("--performance-only",action="store_true")
    parser.add_argument("--check",action="store_true")
    parser.add_argument("--self-check",action="store_true")
    parser.add_argument("--final",action="store_true")
    args=parser.parse_args()
    if args.self_check:
        row={"resource_observation_profile":"ack-window-v1","category_peak_scope":"sampled-window-not-continuous",
             "exact_cgroup_window_attribution":"unavailable","native_cgroup_peak_scope":"whole-container-lifetime",
             "native_process_peak_scope":"whole-worker-lifetime","host_observation_ready_ns":1,"host_t0_ns":2,
             "host_t3_ns":3,"host_observation_finish_request_ns":4,"cgroup_window_start_ns":10,"cgroup_window_end_ns":20,
             "cgroup_window_duration_ns":10,"clock_sampler_start_ns":1,"cgroup_sample_count":2,
             "cgroup_lifetime_peak_bytes":100,"cgroup_memory_baseline_bytes":50,"cgroup_incremental_upper_bound_bytes":50}
        before=copy.deepcopy(row);old=[];new=[]
        frozen_clock_validation(row,old,"test");alias_validation(row,new,"test")
        assert old==["test observation scope"] and not new and row==before
        conflict={**row,"exact_cgroup_phase_attribution":"available"}
        try: alias_validation(conflict,[],"conflict")
        except AssertionError: pass
        else: raise AssertionError("conflicting alias accepted")
        failures=[];alias_validation({**row,"cgroup_incremental_upper_bound_bytes":0},failures,"bad-memory")
        assert failures, "real memory finding suppressed"
        assert tuple(report.ACCEPTED_NS)==(20_000_000,20_000_000,30_000_000)
        print("PASS final consumer alias/conflict/no-mutation/real-gate self-check")
        return
    result=classify(load(args.inputs),not args.performance_only)
    result["admission_eligible"]=False
    if args.final:
        assert not args.performance_only
        if args.check: custody.require_clean()
        validate_final_gate(load(args.inputs),result)
    text=render(result)
    if args.check:
        assert load(HERE/"classification.json")==result
        assert (HERE/"report.md").read_text()==text
    else:
        custody.write_json(HERE/"classification.json",result)
        (HERE/"report.md").write_text(text)
    print(json.dumps({"status":result["status"],"performance_rows":result["performance_rows"],"source_subproofs":result["source_subproofs"]}))
    raise SystemExit(0 if result["status"]=="pass" else 1)


if __name__=="__main__":
    main()
