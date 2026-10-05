# S1 exit audit — shared SQLite engine

> **Status:** Completed milestone audit; S1 implementation and required covering evidence pass. Completion source is the local S1 milestone commit following `14c8a7be4`.

Primary completion target: **S1**, using the explicit deliverable/exit row in
[07](../303/07-implementation-validation.md#3-slices)
and [tracker #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307).
Parent source: `14c8a7be4214a81cd029dd6a959011ba92db5b0c`. The accompanying
S1 closure slice removes the arbitrary default total-page quota and fills the
missing payload/name/scratch/lease paired access evidence. This audit does not
claim S2–S13 or complete S0/native acceptance.

## Criteria and evidence map

| Required S1 criterion | Actual implementation | Identity-matched evidence / remaining check |
| --- | --- | --- |
| One initialized database per daemon, before readiness | `Owner::start` creates one `Overlay` on its owner thread, acknowledges startup only after schema/profile readback; clients cannot access a DB connection | Active daemon source; final locked host/Linux owner proofs in s1-closure cover the profile change |
| No per-Workspace database/schema/init | `open_workspace` inserts one incarnation/base routing row in the existing connection; create_new refuses an existing path | Source and binary/routing tests at `14c8a7be4`; no directory-presence claim |
| Schema/version/application identity and selected settings read back | Schema v4; MEMORY/OFF/EXCLUSIVE, mmap0, busy0, foreign keys, page4096, FILE temporary state, pager setting, SQLite version/options checked | Actual macOS3.51.0/Linux3.53.2 receipts in terminal-reclaim; final host/Linux profile tests read back the new format ceiling |
| Namespaced typed metadata/payload/scratch/ownership | Inode/dentry keys, fixed binary cells/masks, operation-owned scratch, exact lease keys; route incarnation is checked for every operation | Public engine binary/isolation/lease/scratch tests at `14c8a7be4`; semantic filesystem operations belong to S4/S5 |
| Prepared SQL and indexed access | Cached statements, namespace-leading primary/unique keys and capture/reclaim indexes; no custom mutable tree/graph or resident namespace mirror | Source; current access plan diagnostics share exact production query templates |
| Binary payload and routing correctness | BLOB data/validity, zero/255 bytes, exact cell equality and cross-namespace absence; names/scratch remain binary | Actual public tests repeated on the final S1 source in s1-closure |
| Paired EXPLAIN/runtime profiles from first hot paths | Actual statement status reset per invocation; fixed14-family counters; point/capture/name/scratch/payload/lease/reclaim plans and observed work | Original residual-scan failure and repair retained at f2a381119; updated capture/install/reclaim at14c8a7be4; new access-profile.log covers remaining families |
| Atomic errors, one attempt, original uncertainty | Bounded BEGIN/body/COMMIT; definite body rollback once if still active; actual SQLite FULL; unsafe I/O/corrupt/not-DB or failed commit quarantines and retains nested cause; no busy/retry/resend | Real FULL/known-refusal tests and source at14c8a7be4, rerun for changed default profile; actual device ENOSPC/commit uncertainty campaigns remain S6/S7/integration acceptance |
| No artificial total-state cap disguised as processing budget | Default changes from1,048,576 pages/4GiB to no LayerFS quota; selects/readbacks SQLite's format maximum; optional explicit physical quota is separately reported | New source `profile.rs`; explicit32-page FULL case retains its quota/atomic outcome; no huge-capacity performance claim |

All S1 criteria above now have implementation and required evidence. The final
covering checks pass on the closure source: locked core all-target tests (561),
warning-denying Clippy, formatting, product boundary (492 files) and its tool
self-tests (25). Actual Linux ARM64 overlay/daemon all-target tests pass (17).
Raw logs are retained in [s1-closure](checks/s1-closure/core-test.log), including
[Linux](checks/s1-closure/linux-test.log), [Clippy](checks/s1-closure/clippy.log)
and the [first access diagnostic](checks/s1-closure/access-profile.log).
Earlier qualifying checks are reused only at their source/build scope; no failed
receipt is relabelled. No new performance campaign or native FUSE pass is claimed.

Build identity: Rust1.85.1 locked core manifest/lockfile and unchanged root ARM64
Cargo flags. Linux image is
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`,
with worktree-owned `core/target/cluster2-linux` and
`core/target/cluster2-linux-cargo`. macOS uses the existing `core/target` build.
Only docs/evidence reconciliation follows these passing source checks.

## Dependency scope and boundaries

S1 consumes the S0 engine prerequisites: stock macOS/Linux SQLite capability,
one-attempt profile/error contracts, typed routing, bounded job/window interfaces,
and explicit source/resource risk ownership. These are established in the source
and prior receipts. The independent fuser timestamp gate blocks full native
capability closure/S8/S12; it does not invalidate SQL startup/routing/BLOB/index
proofs. Full S0 remains unchecked until its own required contracts/risks close.

Physical cells remain a bounded engine primitive. Choosing/qualifying effective
write/truncate/hole/orphan/failure composition is S0/P8 and S5/S6 work. Namespace
semantics, complete capture/install ownership, physical headroom and aggregate
resource acceptance are not inferred from the S1 engine foundation. A schema can
advance with those implementations; S1 closure does not freeze an unproved final
payload layout or accept the withdrawn extent/fold/pin algorithms.

Existing S2 owner startup is used because it proves the S1 readiness boundary.
Existing S6 terminal work supplies exact retained ownership/namespace cleanup
evidence already present in the engine. No new runtime/base feature is added to
close S1. The next engine completion target is S2's fixed frontier and exact
success/failure custody; base/runtime work will be tied to its named dependencies.

## Resource and implementation limits

The removed4GiB default was arbitrary, not physical headroom protection. SQLite's
format ceiling is4,294,967,294 pages; actual filesystem/device capacity can be
lower. [SQLite limits](https://www.sqlite.org/limits.html) and
[max_page_count](https://www.sqlite.org/pragma.html#pragma_max_page_count) describe
the distinction. Startup requires exact readback rather than silently clamping.
S6 must still implement physical reservations/headroom; S7 must qualify pager,
journal, copy, queue/cache and cumulative costs. Larger ceilings and small probes
do not prove large-workload support.

Current access diagnostic uses one target cell/name/scratch and an exact owner
beside128/1024/4096 unrelated rows. macOS VM work stays payload21, name25,
scratch25, lease51 (the lease family includes scratch-owner validation plus
positive/negative exact lease queries); no fullscan/sort/autoindex/reprepare.
Linux VM work stays payload20, name24, scratch24, lease48 over the same
populations, also with no fullscan/sort/autoindex/reprepare. These are count-driven
scope proofs, not latency/cache-state measurements.
