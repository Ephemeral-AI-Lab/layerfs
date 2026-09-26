# #232 silent Exec progress diagnostic, 2026-09-26

> **Status: Dated planning checkpoint; not release evidence or a product contract.**
> This is one cause-finding attempt at a new source and image identity. The
> historical scenario-v2 receipts and the #245 one-attempt diagnostic remain
> unchanged, including their FAIL results.

Run the registered `prepend-head-4k-on-10mib-ops-1-exec-v2` command exactly
once through public `WorkspaceApi::exec` and mounted POSIX/FUSE. The image
enables the existing per-callback `LAYERFS_FUSE_TRACE`; the release SDK driver
uses `LAYERFS_EXEC_PROGRESS_DIAGNOSTIC=1` only to retain the daemon's raw log
before Sandbox deletion. The command, 5 s native no-progress rule, 30 s Exec
request, one construction worker, cache policy and independent byte-copy
master setup are unchanged. No performance admission, speed or cold-cache
claim follows from this instrumented run; verification is `SKIPPED` unless
the observed mechanism requires an independent proof.

Before the attempt, record the clean source/tree, product and harness seals,
release binary hashes, image ID and source hash, registry/workload hashes,
prepared master identity, exact command and output path in a new local
`FREEZE.json`. The image command is
`python3 core/benchmark/fs-bench-pro/image/build.py --fuse-trace` and the one
run command is
`LAYERFS_EXEC_PROGRESS_DIAGNOSTIC=1 python3 core/benchmark/fs-bench-pro/runner.py run --case prepend-head-4k-on-10mib-ops-1-exec-v2 --out benchmark-results/fs-bench-pro/issue232-exec-progress-diagnostic-01 --verification skipped`.
Retain the attempt whether it succeeds, fails or is ineligible, together with
raw daemon log, driver output and cleanup result. Count mounted READ/WRITE
callbacks and record the typed Exec outcome. A callback count alone does not
establish when each callback completed; do not turn it into a timed-progress
claim without stronger evidence.
