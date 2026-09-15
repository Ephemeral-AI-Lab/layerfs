#!/usr/bin/env python3
"""Check roadmap arithmetic and membership only; never build/run LayerFS."""
import collections
import json
from pathlib import Path
import re


def check():
    root = Path(__file__).resolve().parent
    plan = json.loads((root / "cases.json").read_text())
    cases = plan["cases"]
    by_id = {case["id"]: case for case in cases}
    assert len(by_id) == len(cases) == 36, "duplicate/missing case IDs"
    regular = [case for case in cases if case["lane"] == "regular"]
    extended = [case for case in cases if case["lane"] == "extended"]
    assert len(regular) == plan["regular_case_count"] == 33
    assert len(extended) == plan["extended_case_count"] == 3
    assert collections.Counter(case["family"] for case in regular) == {
        "dedup_branch_history": 6, "file_size_transition": 7,
        "mixed_load_bearing": 4, "multi_workspace_development": 4,
        "branch_development": 6, "historical_access": 6,
    }
    fixtures = plan["fixtures"]
    for name, fixture in fixtures.items():
        classes = fixture["regular_file_classes"]
        inodes = sum(row["count"] for row in classes)
        inode_bytes = sum(row["count"] * row["size_bytes"] for row in classes)
        assert all(row["count"] > 0 and row["size_bytes"] >= 0 for row in classes)
        assert inodes == fixture["initial_regular_inodes"], name
        assert inode_bytes == fixture["initial_distinct_inode_bytes"], name
        aliases = fixture["initial_hardlinks"]
        assert inodes + len(aliases) == fixture["initial_non_directory_paths"], name
        assert inode_bytes + sum(aliases) == fixture["initial_logical_path_bytes"], name
        assert fixture["initial_symlinks"] == 0
        assert fixture["initial_non_directory_paths"] <= fixture["max_non_directory_paths"]
        assert fixture["initial_logical_path_bytes"] <= fixture["max_logical_path_bytes"]
        assert fixture["initial_directories_excluding_root"] <= fixture["max_directories_excluding_root"]

    cycle = plan["mixed_cycle"]
    assert cycle["created_commits"] == 5
    assert cycle["regular_unlinks"] == cycle["ordinary_regular_creates"] == 64 + 32
    assert cycle["directory_creates"] == cycle["directory_removes"] == 8 + 1
    assert cycle["main_edit_targets"] == 8 + 3 + 2 + 1 + 2
    assert cycle["sdk_edit_calls"] == cycle["sdk_edit_members"] == 2
    assert cycle["sdk_batch_calls"] == 0, "cross-file batches are unsupported"
    assert cycle["max_extra_non_directory_paths"] == 8 + 4 + 1
    assert cycle["max_extra_logical_path_bytes"] == 8 * 4096 + 4 * 64 + 4096
    for name, path_cap, byte_cap, data_dirs in [
        ("L100", 5000, 100_000_000, 52),
        ("L500", 30000, 500_000_000, 302),
    ]:
        fixture = fixtures[name]
        assert fixture["initial_non_directory_paths"] == path_cap - 16
        assert fixture["initial_logical_path_bytes"] == byte_cap - 65536
        assert fixture["max_non_directory_paths"] == path_cap
        assert fixture["max_logical_path_bytes"] == byte_cap
        assert (data_dirs - 3) * 100 + 3 * 32 >= path_cap - 16
        assert fixture["initial_directories_excluding_root"] == data_dirs + 10
        # Stage envelopes include retained aliases and one live atomic-save temp.
        for extra_paths, extra_bytes in [(0, 0), (0, 4096), (0, 0), (12, 33024), (13, 37120), (0, 0)]:
            assert fixture["initial_non_directory_paths"] + extra_paths <= path_cap
            assert fixture["initial_logical_path_bytes"] + extra_bytes <= byte_cap
    assert fixtures["S"]["initial_logical_path_bytes"] == 2_658_304

    content = plan["content_schedule"]
    assert content["refresh_pool_files"] == content["refresh_cohorts"] * content["refresh_files_per_cohort"] == 1024
    cohorts = [0 if c % 2 else (c // 2 - 1) % 16 for c in range(1, 21)]
    assert cohorts[:2] == [0, 0], "K10 must revisit recurrent content"
    assert len(set(cohorts)) > 1, "K100 must rotate beyond the hot cohort"
    # Each public edit must change bytes, including after a historical fork.
    def apply_control(state, first, last):
        state = dict(state)
        for j in range(first, last + 1):
            offset = 4096 * ((j - 1) % 2)
            value = (j - 1) // 2 % 2
            assert state[offset] != value, ("no-op control Commit", j)
            state[offset] = value
        return state
    initial = {0: "Z", 4096: "Z"}
    fork = apply_control(initial, 1, 5)
    apply_control(initial, 1, 10)
    apply_control(fork, 6, 15)
    descendant_fork = apply_control(fork, 6, 10)
    apply_control(descendant_fork, 11, 20)
    hot_regions = {(f, r): "A" for f in range(2) for r in range(3)}
    visits = collections.Counter()
    for j in range(1, 101):
        key = ((j - 1) % 2, (j - 1) // 2 % 3)
        visits[key] += 1
        value = "B" if visits[key] % 2 else "A"
        assert hot_regions[key] != value, ("no-op hotset Commit", j)
        hot_regions[key] = value

    environment = plan["environment"]
    assert environment["container_cpus"] == 2
    assert environment["container_memory_bytes"] == 2 * 1024**3
    assert environment["container_swap_bytes"] == 0
    assert environment["container_pids"] == 256
    assert not environment["default_all"] and not environment["default_extended"]
    assert environment["regular_worker_deadline_ms"] + environment["regular_cleanup_reserve_ms"] == 15000

    mixed_regular = []
    for case in cases:
        assert case["fixture"] in fixtures
        assert case["seeds"] == [1, 2, 3] and case["default_seed"] == 1
        assert not case["implemented"] and not case["qualified"]
        assert not case["default_admission_eligible"]
        commits = case["branch_local_new_commits"]
        assert len(commits) == case["branch_count"]
        assert sum(commits) == case["new_commits_total"]
        assert 1 <= case["max_live_workspaces"] <= case["branch_count"]
        assert case["container_count"] == 1
        if case.get("producer_case"):
            producer = by_id[case["producer_case"]]
            assert case["new_commits_total"] == 0
            assert case["fixture"] == producer["fixture"]
            assert case["retained_graph_roots"] == producer["retained_graph_roots"]
            assert case["requires_sealed_history_input"]
        else:
            assert case["retained_graph_roots"] == 1 + sum(commits)
            assert max(commits) <= case["longest_ancestry_commits"] <= sum(commits)
        if case["lane"] == "regular":
            assert case["modes"] == ["performance", "verification"]
            assert case["perf_complete_limit_ms"] == case["verify_complete_limit_ms"] == 15000
        else:
            assert case["default_status"] == "NOT_RUN_EXTENDED"
            assert case["verify_complete_limit_ms"] in (60000, 120000, 300000)
        if case["schedule"].startswith("M1"):
            assert all(count % 5 == 0 for count in commits)
            assert case["development_commits_per_branch"] in (10, 100)
            assert case["fixture"] in ("L100", "L500")
            if case["schedule"] == "M1-branch":
                k = case["development_commits_per_branch"]
                assert commits == [10, k, k]
                assert case["longest_ancestry_commits"] == 5 + k
            if case["lane"] == "regular":
                mixed_regular.append(case)
    assert len(mixed_regular) == 12
    for family in ("mixed_load_bearing", "multi_workspace_development", "branch_development"):
        assert {(c["fixture"], c["development_commits_per_branch"]) for c in mixed_regular if c["family"] == family} == {
            ("L100", 10), ("L100", 100), ("L500", 10), ("L500", 100),
        }
    # Check all local document links; this does not contact GitHub or the network.
    for doc in root.glob("*.md"):
        for target in re.findall(r"\]\(([^)]+)\)", doc.read_text()):
            if "://" not in target and not target.startswith("#"):
                assert (doc.parent / target.split("#")[0]).exists(), (doc.name, target)
    return {"status": "PASS", "scope": "roadmap-only, no product execution",
            "regular_cases": len(regular), "extended_cases": len(extended),
            "mixed_load_cases": len(mixed_regular), "fixtures": len(fixtures)}


if __name__ == "__main__":
    print(json.dumps(check(), sort_keys=True))
