# LayerFS 0.1.1

> **Status:** Planning and admission; no release candidate exists.
>
> **Compatibility:** Preserve the released 0.1.0 contract.

## Problem statement

LayerFS 0.1.0 has no reproducible evidence for the complete public lifecycle at
large namespace sizes. An exploratory import of 9,000 empty files stayed
CPU-bound in `layerstack init` for more than three minutes, but it was stopped
before Branch fork, Workspace create, or Commit. Separately, the current
Workspace planner constructs complete base and final namespace manifests.

These observations motivate measurement; they do not yet prove which path, if
any, is a patch-worthy defect.

## Goal

Make this public lifecycle exact and resource-bounded from 100 through 100,000
regular files:

```text
existing directory
  -> LayerStack initialize
  -> Branch fork
  -> real-FUSE Workspace
  -> ten-byte overwrite
  -> Commit
  -> End
  -> fresh reopen and verification
```

The baseline decides whether initialization, localized Commit planning, both,
or neither need a production fix. v0.1.1 still completes and freezes the
reproducible namespace benchmark; operations without a measured defect close
as measured with no code change.

## Files to read

Read the contracts and harness first:

- [Namespace-v2 benchmark and optimization specification](namespace-optimization-spec.md)
- [Namespace-v2 execution handoff prompt](namespace-v2-handoff-prompt.md)
- [Baseline](baseline-2026-09-02.md)
- [0.1.x benchmark contract](../benchmarking.md)
- [Benchmark harness](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Benchmark runner](../../../../benchmark/fs-bench-pro/run.sh)
- [Benchmark workload](../../../../benchmark/fs-bench-pro/workload.rs)

If the baseline admits a defect, read only its implementation path:

- Initialization: [LayerStack import](../../../../crates/layerfs-layerstack-store/src/layerstack.rs)
- Commit planning: [Workspace changes](../../../../crates/layerfs-workspace/src/changes.rs)
  and [Workspace limits](../../../../crates/layerfs-workspace/src/limits.rs)
- Real adapter proof: [live FUSE test](../../../../crates/layerfs-sdk/tests/live_fuse.rs)
  and [live Docker test](../../../../crates/layerfs-sdk/tests/live_docker.rs)

## GitHub issue

Current admission issue:

- [x] [#6 — Measure and admit the large-namespace lifecycle](https://github.com/Ephemeral-AI-Lab/layerfs/issues/6)
- [x] Assign it to `@yifanxuaaa`.
- [x] Track benchmark implementation, the 0.1.0 baseline, admission decisions,
  real-FUSE correctness, and cleanup in that issue.
- [x] Track the admitted initialization continuation in
  [#7](https://github.com/Ephemeral-AI-Lab/layerfs/issues/7), demand-loaded
  Workspace Create in [#9](https://github.com/Ephemeral-AI-Lab/layerfs/issues/9),
  and the namespace-v2 fixture contract in
  [#10](https://github.com/Ephemeral-AI-Lab/layerfs/issues/10).
- [x] Track the measured bounded direct-admission continuation in
  [#11](https://github.com/Ephemeral-AI-Lab/layerfs/issues/11); #7 remains the
  200-MB/s outcome issue.
- [ ] Create a release issue only after a candidate exists.

### Current namespace-v2 continuation

The implemented namespace-v2 fixture and retained candidate are correct but
not a performance PASS. The warm/uncontrolled-cache 100,000-file median is
4.502 seconds for 500 MB, or 111.1 MB/s. The retained path prepares canonical
segments in about 2.44 seconds, then spends about 1.54 seconds in SQLite step
and commit work. It also writes and rereads about 647 MB of temporary object
segments.

Issue #11 owns one replacement for that sequential boundary: eight existing
import producers, exact bounded initialization-local metadata interning,
256-KiB/512-object owned slabs, a four-slab synchronous queue, and the calling
thread as sole SQLite admission owner. The metadata table starts empty in
every fresh process and Store and is destroyed with the operation; it is
in-operation common-result reuse, not a warm or persistent product cache.

The new path may neither add a worker nor change canonical bytes, the Store
schema, eager initialization, or visibility-last publication. Correctness is
proved outside the timed initialization measurement. The binding throughput
floors are 300/400/400/200 MB/s for 100/1,000/10,000/100,000 files. No result
becomes PASS until every tier meets its floor, the 100,000-file median is at
most 2.5 seconds and 40,000 files/s, and the CPU and memory gates pass.

## Benchmark contract

The table below is the implemented namespace-v1 fixture retained by the dated
baseline. The active namespace-v2 profile keeps these scenario IDs and runner
but uses separate fixture, byte-budget, result-schema, and optimization-gate
identities. See the
[namespace-v2 specification](namespace-optimization-spec.md). It does not add
another benchmark family.

| Scenario | Regular files | Directories | Bytes per file | Logical bytes |
| --- | ---: | ---: | ---: | ---: |
| `namespace-100` | 100 | 1 | 2,500 | 250,000 (0.25 MB) |
| `namespace-1000` | 1,000 | 10 | 2,500 | 2,500,000 (2.5 MB) |
| `namespace-10000` | 10,000 | 100 | 2,500 | 25,000,000 (25 MB) |
| `namespace-100000` | 100,000 | 1,000 | 2,500 | 250,000,000 (250 MB) |

`MB` is decimal; exact byte counts are authoritative. Each regular file has
deterministic path-derived content that is unique across files. Fixtures use
100 regular files per directory and are prepared outside LayerFS timing.

All timed rows use real Linux FUSE. Materialization is used only for the
untimed equality proof at 10,000 files / 25 MB.

Add one LayerFS-only namespace runner with one-case and full-matrix modes. It
must not call or contribute to the registered payload totals or the paired
Cloudflare Computer campaign. Run the existing paired payload campaign once,
after the LayerFS candidate is stable.

## Admission checklist

### Harness

- [x] Add all four namespace scenarios to `fs-bench-pro`.
- [x] Add a separate `run-namespace.sh` with one-case and full-matrix modes.
- [x] Run each tier in a fresh process and immutable evidence directory.
- [x] Refuse to overwrite evidence and retain every valid sample.
- [x] Record exact source, harness, container, host, and fixture identities.
- [x] Reuse the existing fresh-process ten-byte positional overwrite.
- [x] Verify missing, extra, and changed paths after a fresh Store reconnect.

### Measurements

- [x] Report initialization, Branch fork, Workspace create, edit, Commit, End,
  reopen verification, and complete lifecycle time separately.
- [x] Define `complete_product_ns` from immediately before LayerStack
  initialization through completed reconnect/reopen verification; exclude and
  record fixture, Store, Client, container, and report setup.
- [x] Report CPU time, peak RSS, scanned files and bytes, candidate/inserted/
  reused objects and bytes, and transaction maxima.
- [x] Name and retain the evidence source for every metric; reject unavailable
  fields instead of emitting silent zero defaults.
- [ ] Run one exploratory sample per tier against the 0.1.0 product path.
- [ ] Repeat only where needed to distinguish a defect from environment noise.
- [x] Do not invent a fixed latency gate before the baseline exists.
- [x] Record accept, defer, or reject decisions for initialization and Commit.

### Conditional root-cause fixes

For every admitted defect:

- [x] Add one focused check that fails on 0.1.0 and passes on the candidate.
- [x] Profile the public path and fix the shared root cause.
- [x] Reuse existing builders and mutation state before adding machinery.
- [x] Preserve file bytes, directory order, links, modes, mtimes, canonical
  identities, bounded admission, and visibility-last publication.
- [x] Preserve rename, hard-link, metadata, and open-unlink behavior touched by
  a Commit-planner change.
- [x] Keep the final-delta resource bound; do not hide whole-tree planning by
  raising it.
- [x] Prove unchanged persistent subtrees are reused where applicable.
- [ ] Rerun all four namespace tiers and verify the reopened result exactly.

### FUSE and cleanup proof

- [ ] Complete every timed tier through real Linux FUSE.
- [ ] Treat 100,000 files / 250 MB as the ceiling without a pre-baseline
  latency threshold.
- [x] At 10,000 files / 25 MB, prove untimed materialization and FUSE produce
  the same logical state and canonical root.
- [x] Run Docker create/start/attach/execute/Commit/End/stop/remove.
- [x] After mount success, inject daemon-attachment failure and prove no leaked
  mount, container, process, output reader, spool, Workspace, or Branch lease.
- [x] Document the proved subset without claiming universal POSIX support.

## Compatibility gates

- [x] Preserve the five-table Store schema.
- [x] Preserve canonical bytes, identity domains, and the CDC profile.
- [x] Preserve LayerStack, Layer, Branch, Commit, Workspace, and Execution
  semantics.
- [x] Preserve Store visibility, transaction, and acknowledgement semantics.
- [x] Preserve documented SDK and CLI behavior.
- [x] Preserve compatibility with the released container-daemon protocol.
- [x] Preserve existing memory, paging, Workspace transaction, and cleanup
  bounds; initialization has its own bounded 8,191-object admission batches.
- [x] Audit the selected candidate diff against every item above.

An incompatible change belongs in 0.2.0.

## Acceptance criteria

- [ ] All four real-FUSE tiers complete initialization, Branch fork, Workspace
  create, edit, Commit, End, and exact fresh-reopen verification.
- [ ] The 100,000-file / 250 MB ceiling completes without OOM, swap, bound
  failure, resource leak, or incorrect result.
- [ ] Initialization and localized Commit costs are reported separately.
- [ ] Every admitted defect has one focused failing check and one shared
  root-cause fix.
- [x] A released 0.1.0 Store opens on the candidate.
- [x] Frozen canonical fixtures remain unchanged.
- [x] `cargo fmt --all -- --check` passes.
- [x] `tools/test-fast.sh` passes below its 120-second warm-suite ceiling.
- [x] Warning-denying workspace Clippy passes.
- [x] `git diff --check` passes.
- [x] Real-FUSE, Docker, equality, and cleanup proofs pass on capable Linux.
- [ ] The full LayerFS-only namespace matrix has candidate-source evidence.
- [ ] Admit the four namespace scenario definitions into the append-only
  registry without changing the frozen v0.1.0 rows.
- [ ] Existing registered payload rows have no unexplained regression.
- [ ] The existing paired LayerFS/Cloudflare payload campaign passes once after
  candidate stability; it is not part of the optimization loop.
- [ ] Limitations and evidence agree with the implementation.
- [ ] Select one clean immutable source commit.
- [ ] Bump the workspace version and lockfile to `0.1.1`.
- [ ] Create `docs/versioned/0.1.1/` and `release-notes/0.1.1/` from verified
  candidate evidence.
- [ ] Run release gates against that exact source, build source archives and
  checksums, then publish the annotated `v0.1.1` tag and release.

## Deferred to 0.1.2

- [x] Prepend transfer optimization and extent-aware `copy_file_range`.
- [x] Borrowed Workspace ranges.
- [x] Fragmented-write, sparse-growth, and broader mixed-edit resilience.

Broad refactors, new incompatible adapter operations, other platforms,
remote synchronization, a TUI, new crates, and another database remain out of
scope unless separately admitted by a later roadmap.

## References

- [Namespace-v2 benchmark and optimization specification](namespace-optimization-spec.md)
- [Namespace-v2 execution handoff prompt](namespace-v2-handoff-prompt.md)
- [#11 bounded cold-start direct admission](https://github.com/Ephemeral-AI-Lab/layerfs/issues/11)
- [0.1.x benchmark contract](../benchmarking.md)
- [0.1.x development guide](../development.md)
- [0.1.2 proposals](../0.1.2/README.md)
- [0.1.3 draft](../0.1.3/README.md)
- [0.1.4 draft](../0.1.4/README.md)
- [Baseline — 2026-09-02](baseline-2026-09-02.md)
- [Agent handoff prompt](handoff-prompt.md)

Add another dated checkpoint only when evidence, scope, or release status
materially changes.
