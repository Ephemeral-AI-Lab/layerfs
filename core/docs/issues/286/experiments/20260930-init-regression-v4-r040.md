# #286 round 20260930-init-regression-v4-r040

> **Status: Family1 default functional Init regression PASS** at the current
> low-cost-codec product source. SDK numeric latency remains INELIGIBLE under
> uncontrolled source-cache residency; the functional, command/verifier and
> cleanup checks pass. This closes the earlier-family check required after
> v4 history r038/r039, so Families1–2 have scoped functional checkpoints.

The one-sample-per-case default selection ran100 then1,000 files at clean
source `5e775aa09`, product seal
`1cd4c27ed28a1caebbed73d8343ac72db87c577c22ef236775a071f74631eeb0`,
Init harness seal
`d0147f9260996f6288dd0c56c3409bb715988108c5e45aa73d9acd2284bb783f`.
The locked release SDK driver and independent verifier SHA-256 values are
`06a031846776ad00716d4a90c5a1166ccc25eccda55b19e68a1e5a45e079c0e8`
and `7c9fe69fd4a7977a78bf9b9f9b0169eba7fefddba5ca6ce55ccc4243dd74bab3`.
The worktree-local incremental build took4.962 s within its30 s bound.
Exact command:
`LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/benchmark/fs-bench-pro/runner.py run --family init_namespace --out benchmark-results/fs-bench-pro/issue286-init-regression-v4-r040`.
The prepared immutable fixtures were reused by exact identity outside timers;
each SDK call created a fresh Store/history owner. Build, measured children
and verification did not overlap within this worktree.

| Case | Raw SDK call | Complete command /15 s | Separate verifier /9.5 s | Verified namespace and selected bytes | Functional / cleanup | Numeric latency |
| --- | ---: | ---: | ---: | --- | --- | --- |
| 100 files | **42,571,167 ns** | **1,715,790,625 ns** | **606,546,708 ns** | 102 paths,53 selected files /3,354,003 B | **PASS / PASS** | **INELIGIBLE** |
| 1,000 files | **128,172,042 ns** | **140,344,125 ns** | **55,395,500 ns** | 1,011 paths,70 selected files /6,430,827 B | **PASS / PASS** | **INELIGIBLE** |

Both public `Client::init_project` calls returned known roots. The independent
release verifier reopened Store/history and checked every path/kind plus the
declared selected metadata/content scope; it did not hash every file's bytes.
The 10,000/100,000 tiers remain separately registered **NOT_RUN**. The
two-case command cycle took2.662 s within its recommended30 s bound; the
100-file complete-command wall includes lifecycle overhead and is not a raw
SDK latency. `runner.py verify --run` rederives PASS from retained evidence.
No cold Init or relative speed claim is made.

[Exact receipts, separate verification, original C5 files and SHA-indexed raw
evidence](20260930-init-regression-v4-r040/evidence-index.json) are append-only;
large original C2 Stores remain at their indexed local paths. Family2 v4
stride10/3/1 has its own r038/r039 same-seal PASS receipts with at most10%
storage deviation; none of the old strict receipts was relabeled. Next is
Family3's registered nine-cell public SDK/POSIX-FUSE write matrix. Families3–7
are NOT_RUN, PR#285 draft and #286 open. This report-only commit has
production LOC reference65,417→65,417, Core70,219→70,219,
combined135,636→135,636 (delta+0), counted with
`tools/production_loc.py --json --root <snapshot>` on exact first-parent and
staged/committed Git archives, including shipped SQL and excluding tests,
benchmark harness and docs.
