# `fs-bench-pro` v0.1.2 family format

> **Status:** Current planning checklist; no release candidate exists.

The general [benchmark rules](../../../general/benchmark_rules.md) and the
[issue #20 specification](sdk-only-edit-benchmark-rebuild.md) are authoritative.

## Active layout

```text
benchmark/fs-bench-pro/
├── families/edit_length_preserving.rs
├── families/edit_length_changing.rs
├── families/edit_canonical_chunk_count.rs
├── src/main.rs
├── src/sdk_file_edit.rs
├── lib-edit-sdk-runner.sh
├── run-edit-length-preserving.sh
├── run-edit-length-changing.sh
└── run-edit-canonical-chunk-count.sh
```

Each family owns only its ordered registry, edit plans, fixture identity,
expected counters, and self-check. The shared Rust executor owns the singular
SDK call topology and receipt. The shared shell library owns arguments,
custody, supervision, timeouts, summaries, and evidence sealing. No new crate,
editor, dependency, or benchmark-only product API is required.

The old `edit_same_count` and `edit_count_changing` files and runners remain
reproducibility-only and are unreachable from the active registry.

## Command modes

```text
run-edit-*.sh --self-check
run-edit-*.sh RUN_ID CONTAINER --case ID --repetition 1 --mode performance
run-edit-*.sh RUN_ID CONTAINER --case ID --mode verify
run-edit-*.sh RUN_ID CONTAINER --all --mode admission
```

Self-check is product-free and finishes under two seconds. Selected performance
or verification emits `admission_eligible=false`. Complete family execution
requires explicit `--all`.

## Singular operation and timing

All plans and replacement buffers exist before T0:

```text
T0 -> Client::edit_workspace_file_range -> T1/T2
T2 -> Client::commit_workspace_session -> T3
T3 -> Client::end_workspace_session -> T4
```

`T2` is the exact stored `T1` timestamp. Therefore
`edit_commit_ns == edit_call_ns + commit_call_ns` as an integer equality.
Only after T4 may one read-only Branch query validate the returned Commit ID;
its `visibility_validation_ns` is not part of a performance distribution.

No registered scenario calls Exec or the batch API. A real FUSE projection is
attached, but mutation-caused FUSE payload and Workspace spool counters are
zero.

## Evidence

Each family writes the exact layout frozen in the issue #20 specification:
environment custody, `performance/raw.jsonl`, `verification/raw.jsonl`, one
scenario/repetition directory, `run-status.json`, `report.md`, and
`evidence.sha256`.

Terminal admission requires 5 repetitions for every ID and both source arms:
560 performance rows total. The 56 aggregate verifier receipts bind the five
performance repetitions and contain 112 independent source-arm subproofs.

The report shows per-scenario medians and min-max ranges for Edit, Commit, Edit
plus Commit, process RSS, cgroup memory, CDC/candidate work, zero FUSE/spool,
and every status. Derived summaries must be reproducible only from sealed raw
JSONL and the sealed report generator.
