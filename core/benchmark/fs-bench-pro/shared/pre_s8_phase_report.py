"""Reproduce supported phase observations without inventing continuous peaks."""
from .evidence_engine import rows
from .evidence_jobs import require


def phase_report(path):
    phases = {}
    count = 0
    for row in rows(path):
        require(row.get("schema") == "pre-s8-phase-observation-v1" and row.get("index") == count,
                "original resource record identity/order")
        count += 1
        process = row["process"]
        if process["scope"] == "macos-process":
            rss = process["memory_bytes"]["rss"]
        else:
            rss = process["smaps_rollup_bytes"]["Rss"]
            require(process["memory_status_reported_bytes"]["VmSwap"] == 0 and process["smaps_rollup_bytes"]["Swap"] == 0,
                    "observed process swap")
        item = phases.setdefault(row["phase"], {"samples": 0, "interior_samples": 0,
            "rss_baseline_bytes": None, "rss_final_bytes": None, "rss_observed_max_bytes": 0,
            "first_sample_ns": process["read_open_ns"], "last_sample_ns": None,
            "maximum_sample_gap_ns": 0, "cgroup_observed_max_bytes": None,
            "cgroup_categories_observed_max_bytes": {}, "artifacts": {}})
        if item["last_sample_ns"] is not None:
            item["maximum_sample_gap_ns"] = max(item["maximum_sample_gap_ns"], process["read_open_ns"] - item["last_sample_ns"])
        item["last_sample_ns"] = process["read_open_ns"]
        item["samples"] += 1
        item["rss_observed_max_bytes"] = max(item["rss_observed_max_bytes"], rss)
        if row["boundary"] == "baseline":
            require(item["rss_baseline_bytes"] is None, "duplicate phase baseline")
            item["rss_baseline_bytes"] = rss
        elif row["boundary"] == "final":
            require(item["rss_final_bytes"] is None, "duplicate phase final")
            item["rss_final_bytes"] = rss
        else:
            require(row["boundary"] == "interior", "unknown resource boundary")
            item["interior_samples"] += 1
        if row["cgroup"] is not None:
            cgroup = row["cgroup"]
            current = cgroup["memory_current_bytes"]
            item["cgroup_observed_max_bytes"] = max(item["cgroup_observed_max_bytes"] or 0, current)
            for name, value in cgroup["memory_stat_bytes"].items():
                item["cgroup_categories_observed_max_bytes"][name] = max(item["cgroup_categories_observed_max_bytes"].get(name, 0), value)
        for observed in row["artifacts"]:
            if "allocated_bytes" in observed:
                name = observed["path"].rsplit("/", 1)[-1]
                held = item["artifacts"].setdefault(name, {"logical_observed_max_bytes": 0, "allocated_observed_max_bytes": 0})
                held["logical_observed_max_bytes"] = max(held["logical_observed_max_bytes"], observed["logical_bytes"])
                held["allocated_observed_max_bytes"] = max(held["allocated_observed_max_bytes"], observed["allocated_bytes"])
    require(count > 0, "no original resource observations")
    for phase, item in phases.items():
        item["phase_peak_bytes"] = None
        item["phase_peak_status"] = "UNAVAILABLE_CONTINUOUS_COVERAGE_AND_CLOCK_ATTESTATION"
        item["scope"] = "external-acknowledged-phase-window"
        item["coverage"] = ("END_MARKER_ONLY" if phase == "finished" else
                            "BASELINE_INTERIOR_FINAL_OBSERVED" if item["interior_samples"] and item["rss_baseline_bytes"] is not None and item["rss_final_bytes"] is not None else
                            "INCOMPLETE_INTERIOR_OR_BOUNDARY")
        item["numeric_acceptance"] = "OWNER_DEFERRED"
    return {"schema": "pre-s8-phase-report-v1", "admission_eligible": False,
            "scope": "sampled diagnostic observations; no exact or continuous peak", "records": count,
            "phases": phases, "unavailable": ["exclusive macOS file-cache/kernel/physical-I/O attribution",
            "internal SQLite copy/pager-page attribution", "continuous phase maxima", "cross-size RSS acceptance"],
            "category_rule": "overlapping process/cgroup fields must not be summed"}
