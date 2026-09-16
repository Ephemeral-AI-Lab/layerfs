# LayerFS 0.1.6 Developer Preview

> **Status:** LayerFS 0.1.6 Developer Preview release record.
> [Tag and downloads](https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.6).

The owner directed this closure on 2026-09-16: close the v0.1.6 issue set
honestly, prepare the release documents, and commit, tag and publish v0.1.6 from
this tree. [#154](https://github.com/Ephemeral-AI-Lab/layerfs/issues/154) and
[#122](https://github.com/Ephemeral-AI-Lab/layerfs/issues/122) were closed with
posted reports, [#152](https://github.com/Ephemeral-AI-Lab/layerfs/issues/152)
with its published tally, and #151 with its disposition recorded in the
[roadmap index](../../docs/roadmap/0.1/0.1.6/README.md).

## What v0.1.6 is

1. **Live Workspace state moved into the sandbox.** The container-side owner
   holds the mutable namespace, the file data and a private packed payload
   backing created for its mount; the host keeps canonical construction, the
   SQLite Store and publication, and learns the workspace's mutable state only
   through a Commit-time transfer. A stale backing directory from a crashed mount
   is refused, not reused.
2. **No pause or quiesce step.** `FREEZE`/`RESUME` survive only as wire constants
   with no dispatch handler and `commit_pause_fence_ns` is 0 in all 56 SDK-edit
   rows; the workspace is quiesced by construction of the transfer, not by a
   control-plane fence.
3. **One construction worker by rule.** `construction_worker_limit()` and the
   canonical construction it feeds are single-producer, every run exports
   `LAYERFS_CONSTRUCTION_WORKERS=1`, and `init_namespace` is the only exception.
   The measured cost is published, not hidden: six dedup/CDC construction cells
   are 1.50–1.65× v0.1.5 and are owner-accepted, and the cold
   `namespace-100000` Init target is owner-waived (4.986 s against 2.7 s).
4. **A bounded sandbox spool.** The sandbox's own residency fell from ~500 MiB of
   page cache to **≤ 2.6 MiB**, and the Commit-time transfer now pays a storage
   read (2.1 GiB/s) instead of the cache-served 19 GB/s an earlier design
   appeared to show. Six identical runs produced container lifetime peaks from
   24.6 MB to 189.8 MB, which is why a lifetime cgroup number is no longer a
   gate.
5. **New registered coverage for branch controls and longer history.** The
   v0.1.6 selection set adds the F4 compact branch controls, the F5 `namespace-inode`
   history rows, the F6 `historical_access` boundary/inode/fork/divergent-head
   cases and three declared extended cases (a four-workspace control and two
   exhaustive replays).
6. **Two correctness defects found in-campaign and fixed at the root**, plus one
   reliability repair: the HN orchestrator's stage counters and the corrected
   boundary-cycle exchange, and the six `workspace_reliability`
   fault-injection proofs, now 27/27 PASS on the frozen candidate.

## Compatibility

**The Store format did not change.** `SCHEMA_VERSION` stays 10; no schema or
static SQL changed since v0.1.5; `layerfs-content` and
`layerfs-layerstack-store` are byte-identical to the v0.1.5 release. Supported
schema-6/7/8/9 Stores connect without promotion, schema-5 and other unsupported
versions are rejected without mutation, and there is no in-place promotion, no
downgrade and no retained-history transfer command. Explicit compaction stays
removed; previously compacted Stores remain readable through the retained
authenticated LFCNT1 read path.

What did change is the runtime route and two additive surfaces: the daemon's
mount request carries the workspace's snapshot backing root, and the public SDK
gains two methods behind the `test-instrumentation` feature. The public CLI is
unchanged. **A v0.1.6 sandbox owner and a v0.1.5 host do not share a live
workspace** — match SDK, CLI, daemon and runtime components. See the
[release contract](release-contract.md) and the
[versioned manual](../../docs/versioned/0.1.6/README.md).

## Qualification at a glance

The v0.1.6 selection set is **36 rows** (33 regular + 3 declared extensions),
seed 1, one sample per case and mode: **28 performance receipts** (25 gate `PASS`
and 3 gate `EXCEPTION`, above the 15 s family target inside the declared 60 s
invocation allowance) and **36 independent verifications, every one `PASS`**,
with **every cleanup `PASS`** and no `FAIL`, `TIMEOUT` or `NOT_RUN` anywhere in
the set. All 28 performance receipts reused the closed prepared master
(`cache_hit: true`, `clone_method: closed-quiescent-byte-copy`), and no proof was
reused.

**Published as measured, never relabeled:** five mode-level results are above the
15 s family target — `v016-branch-mixed-500mb-30000-k100-v1` (16.492 s
performance, 23.10 s verification under the owner-declared **30 s** ceiling),
`v016-mixed-development-500mb-30000-k100-v1` (18.312 s / 22.121 s),
`v016-workspace-mixed-500mb-30000-k100-v1` (15.760 s / 20.360 s), and the two
verify-only exhaustive replays (15.014 s and 59.326 s under frozen 120 s and
300 s watchdogs). The cold `namespace-100000` Init target is owner-waived
(4.986 s), the six single-worker construction regressions are owner-accepted, and
kernel-dirty shared `mmap` is still not captured by a Commit. Read
[acceptance](acceptance.md) and [waivers](waivers.md).

The wider #152 sandbox-local campaign is **196 collected cells over three
candidate identities**: 189 PASS, six owner-accepted single-worker material
regressions, the waived 2.7 s cold-Init target, three owner-banked controls cited
rather than re-run, nine optional rows not run, and the inherited 11 + 11
`historical_access` rows `NOT_RUN` because their sealed v2 Store is not
recoverable. No cell was dropped and no number was re-labelled; see the
[#152 final report](../../docs/roadmap/0.1/0.1.6/evidence/issue152-final-report.md).

## Read the release record

- [Release contract and compatibility boundary](release-contract.md)
- [Owner acceptance and disposition](acceptance.md)
- [Waivers, declared exceptions and accepted costs](waivers.md)
- [Verification and source applicability](verification.md)
- [Benchmark closeout: every measured selection](benchmark-closeout.md)
- [Derived performance table](benchmark-performance.csv) · [derived proof table](benchmark-verification.csv)
- [Machine-readable release evidence](release-evidence.json)
- [Artifact preparation and checksums](artifacts.md)
- [GitHub release announcement](github-release.md)
- [Versioned manual](../../docs/versioned/0.1.6/README.md)
- [Limitations](../../docs/versioned/0.1.6/limitations.md)
- [Changelog](../../docs/releases/v0.1.6/CHANGELOG.md)
- [#154 final report](../../docs/roadmap/0.1/0.1.6/evidence/issue154/final-report.md)
- [#152 final report](../../docs/roadmap/0.1/0.1.6/evidence/issue152-final-report.md)
- [v0.1.6 roadmap and acceptance record](../../docs/roadmap/0.1/0.1.6/README.md)

This is a **source-only Developer Preview** with no crash or power-loss
durability promise. No crates.io package, prebuilt executable or public runtime
image is part of this release; the GitHub release carries the tagged source and
its checksums. Every number in this record was measured on the identity chain
printed in [verification](verification.md); nothing was re-measured for
publication.
