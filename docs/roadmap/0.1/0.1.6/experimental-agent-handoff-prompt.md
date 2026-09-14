# Handoff prompt: implement the sandbox-local v0.1.6 experiment

Copy the prompt below into the implementing agent's task. This document prepares the handoff; creating it has not started implementation or benchmarks.

---

Implement the combined #149 + #150 experiment and execute the ordered pipeline in #151. Carry the work through focused correctness checks and the three benchmark gates, diagnosing failures at the current gate before advancing. Deliver attributable results, including failures, without claiming an unmeasured speedup or release readiness.

The repository is `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`. Use an isolated `codex/` branch/worktree starting from **v0.1.5's source commit `6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13`**. Verify the peeled tag with `git rev-parse 'v0.1.5^{commit}'`; the annotated tag object is not the source commit. Preserve the existing checkout and unrelated changes. Inspect current status and existing execution evidence before starting so completed work is not repeated.

**Read these contracts first, in this order.** Paths below are relative to the repository root; product source references must be read at the v0.1.5 base before deciding what to reuse.

1. `docs/roadmap/0.1/0.1.6/experimental-implementation-pipeline.md` — implementation dependencies, exit checks, ordered gates and evidence reuse. Execution: [#151](https://github.com/Ephemeral-AI-Lab/layerfs/issues/151).
2. `docs/roadmap/0.1/0.1.6/sandbox-local-snapshot-spec-and-plan.md` — complete behavior, fixtures, budgets, encoding and acceptance contract. Requirements: [#149](https://github.com/Ephemeral-AI-Lab/layerfs/issues/149).
3. `docs/roadmap/0.1/0.1.6/sandbox-host-connection-architecture.md` — ownership, ASCII comparison, removal list, transfer, publication and result handling. Architecture: [#150](https://github.com/Ephemeral-AI-Lab/layerfs/issues/150).
4. `docs/roadmap/0.1/0.1.6/sandbox-host-connection-review.md` — reviewed failure cases and decisions; especially stalled transfer, Store rollback ownership, physical delta dependencies and generation-safe results.
5. `docs/research/v016-sandbox-snapshot-review-2026-09-15.md` — why the current host-authority route became expensive. The historical 25k temporary-overlay figure is not final CAS storage and its debug timing is not a comparable Commit baseline.
6. `release-notes/0.1.5/release-contract.md` at the release commit — inherited schema, canonical identity, encoding limits and existing durability boundary.
7. `benchmark/AGENTS.md`, `docs/general/benchmark_rules.md`, then `benchmark/fs-bench-pro/QUICKSTART.md` — apply the explicit `v016-local-snapshot-experiment-v1` exception. It permits sandbox temporary backing and requires one sample per case/arm. Legacy host-only-backing and n3 rules do not override it.
8. `benchmark/fs-bench-pro/families/tiny_file_churn/README.md`, `mod.rs`, `perf.sh`, `verify.sh`, and `setup.sh` — existing workload, fixture generation, timing and independent verification. Follow only the helpers these entrypoints actually use.

Read applicable ancestor/directory instructions. The replacement documents may initially be uncommitted and absent from the v0.1.5 worktree: preserve them from the current checkout or the full issue bodies. Freeze the agreed planning documents and scoped benchmark exceptions in attributable commits before benchmark implementation/measurement. Bring over the minimum compatible benchmark adapters; do not accidentally import current-main product code into the control. Older overlay handoffs and broad v0.1.6 matrices are historical context, not this execution plan.

**Trace these existing implementation areas before editing.** Read callers and ownership flow, not just individual functions. Reuse first-party implementations rather than introducing a second storage engine.

| Responsibility | v0.1.5 source paths |
|---|---|
| Local writes, FUSE visibility and transport | `crates/layerfs-fuse/src/live_owner.rs`, `live_runtime.rs`, `live_transport.rs`, `live_wire.rs`, `filesystem.rs` |
| Mutable namespace, pieces, backing and old checkpoint reset | `crates/layerfs-workspace-core/src/namespace.rs`, `file_edit.rs`, `backing.rs`, `checkpoint.rs` |
| Capture, canonical construction and Workspace lifetime | `crates/layerfs-workspace/src/capture.rs`, `reconcile.rs`, `worker.rs`, `lifecycle.rs`, `live_backing.rs`, `registry.rs` |
| SDK/daemon routing and lock ownership | `crates/layerfs-sdk/src/client.rs`; `crates/layerfs-daemon/src/lib.rs`, `protocol.rs` |
| Canonical identity, small content, CDC and extent reuse | `crates/layerfs-content/src/object/digest.rs`, `canonical.rs`; `crates/layerfs-content/src/file/content.rs`, `cdc/mod.rs`, `extent.rs`, `rope/edit.rs` |
| FULL/DELTA selection, compression, physical dependencies and authentication | `crates/layerfs-layerstack-store/src/objects/admission.rs`, `delta.rs`, `small_candidates.rs`, `pack.rs`, `read.rs` |
| Admission isolation and transactional branch publication | `crates/layerfs-layerstack-store/src/store.rs`, `workspace.rs`; `crates/layerfs-layerstack-store/sql/workspace/insert_commit.sql`, `advance_branch.sql` |

Use existing focused tests, especially `crates/layerfs-content/tests/fastcdc_shifted_stream.rs`, `extent_model.rs`, the Store's small-candidate/small-chain tests, and relevant Workspace/FUSE/SDK lifecycle checks. Do not launch their entire historical campaign by default.

**Understand and preserve CAS, CDC and DELTA.**

CAS is the common identity and deduplication layer. Canonical object bytes determine their authenticated identity; exact existing content reuses that identity. Compression or a physical DELTA representation must reconstruct and authenticate the same canonical object. Do not replace canonical hashing with path/mtime checks or hash only changed bytes as if that were the full object's identity.

CDC is the large-file content decomposition mechanism. Preserve existing content-defined chunks, extent references and range provenance. A localized edit should reuse unchanged extents and process only the replacement/boundary data required by the released algorithm. Flattening every edited large file and rebuilding all chunks defeats the required locality even if readback is correct.

For nonempty files below 128 KiB, preserve whole-file SmallContent and the existing FULL/DELTA physical policy. FULL is a self-contained compressed representation; DELTA encodes relative to an eligible base. Exact CAS hits need no new copy. Similar but different content may benefit from DELTA; it is not guaranteed to do so. Keep candidate selection, savings tests and FULL fallback. Schema 10 permits bounded chains: at most 8 dependency edges, 512 KiB decoded closure and 256 KiB retained encoded capacity. Preserve empty-file and exact-128-KiB behavior from the released implementation.

These mechanisms cooperate. Both the small-content and large-file paths retain CAS, authentication, existing compression and packing. Do not replace Init/import behavior, change compression settings, force DELTA for every small file, or introduce a new codec. Preserve transitive physical delta bases as well as logical object references: a published object must remain readable after the sandbox is destroyed.

```text
Frozen generation + previous canonical roots + range provenance
                              |
                 Existing canonical construction
                    /                       \
          SmallContent                  Large-file CDC
          FULL or DELTA                  chunks + extents
                    \                       /
             CAS reuse + compression + authentication + packing
                              |
                 Checked admission and DB publication
```

**Implement in this dependency order.**

```text
I0 Freeze source, contract, harness, budgets and control configuration
 -> I1 Local mutable ownership and compact sandbox backing
 -> I2 Stable owned snapshot, proved locally
 -> I3 Bounded frozen transfer with continuing live service
 -> I4 Existing single-worker builder and exact DB publication
 -> I5 Generation-safe completion and reusable Workspace lifetime
 -> I6 Focused correctness/resource checks; seal complete candidate
 -> B1 create-500 PASS -> B2 bulk-create-500 PASS -> B3 25k PASS
```

At I1, use one daemon registry for multiple Workspaces. Mount live views at `/workspaces/<workspace-id>/`; keep private compact shared backing at `/snapshots/<workspace-id>/`. This backing is not a copied directory tree or second mounted snapshot. Retain snapshot descriptors in memory and internal incarnation/generation/attempt identity. Allow one active Commit and at most one queued Commit per Workspace. Retire only the selected Workspace on End.

Remove per-mutation host reserve/append/install acknowledgement, dirty-state mirroring and sync-triggered full-prefix export from the candidate route. v0.1.5 still has host payload backing, so merely selecting its old route is insufficient. Keep the coordinator, canonical builder and SQLite Store on the host. Mutable snapshot-input transfer occurs at Commit; control/SDK operations, immutable-base cache misses and Commit results still communicate.

At I2, reuse compact pieces and bounded metadata. Retain an immutable generation by ownership; mutate exclusive nodes in place and copy only touched shared nodes. Prove `write A -> capture G -> modify/rename/delete live -> G sees A; live sees the successor`. Avoid whole-map clones, whole-directory copies, sparse page-heavy indexes, per-file snapshots and unbounded generation chains.

At I3, stream only frozen input through bounded validated framing. A stalled transfer must still allow a local write, SDK edit/status and uncached base read to finish. Do not hold a whole-Commit lifecycle lock or monopolize the control connection. Bound queues, reader retention and no-progress handling; count all service CPU.

At I4, retain the Store admission/operation permit and rollback ownership. Before publication, the host must own every required canonical object and physical delta dependency independently of sandbox state. Use the existing conditional SQLite transaction. Prepare a bounded exact attempt result; record successful SQL completion before cancellation points, awaits, telemetry or reply delivery. Cancellation before publication abandons the owned attempt; once publication begins, the host completes/resolves it independently of the caller. A lost successful reply must not cause recapture or duplicate publication. A real indeterminate SQL outcome must not be guessed from autocommit status.

At I5, return exact generation coverage/provenance without reinstalling a checkpoint into the live Workspace. A late C1 acknowledgement must never clear G+1 changes. Preserve prior canonical roots/range correspondence so small C2 edits do not rebuild everything since Begin. The same mount, descriptors and Workspace support further commands and Commits. Release only unowned backing; `/snapshots/<id>/` may still contain bytes owned by the live successor after Commit.

At I6, prove non-pausing behavior with deterministic latches, C1/C2 isolation, two-Workspace isolation including End A while B remains usable, allocation failure, cancellation/publication failures, and relevant POSIX lifetime behavior. Verify exact CAS reuse, eligible DELTA plus FULL fallback, threshold transitions, large-file localized extent reuse, and Store reads after sandbox destruction/reopen. Use existing checks and run only affected checks; no broad matrix.

**Apply these rules throughout.**

- No Workspace-backing `fsync`, `fdatasync`, `sync_data` or `sync_all` for durability. The Workspace is disposable. Keep application sync calls supported with volatile ordering, readable owned bytes and known-error reporting. Commit must work without a preceding fsync. Keep the same workload sync calls in control and candidate; do not remove them to improve timing. Preserve the database transaction/rollback machinery and existing Store configuration. No process-crash/power-loss guarantee is being added.
- No third-party library edits, patches, forks, vendoring, dependency substitutions, new dependencies, version/feature changes or lockfile changes. Supported APIs only. Ordinary first-party source imports may change to connect reused code; Init/import encoding behavior stays intact.
- No pause, quiesce, wait-for-command completion, whole-cache drain, remount, freeze/build/checkpoint/resume or bulk work under a global lock. Short metadata synchronization is allowed. Bounded admission can reject excess work; it must not corrupt state or disguise target-case exhaustion as success.
- Exactly one shared canonical compute worker for the experiment, in both arms. No per-file encoding tasks or parallel reclamation pool. Necessary bounded FUSE/control/I/O services remain identified and fully counted; compilation jobs are a separate matter.
- Do not introduce mmap as a snapshot requirement. Writable shared-mmap visibility remains unresolved; excluding that supported mode was proposed but not adopted. Ordinary-write qualification may proceed with the limitation recorded. Do not silently disable mappings or claim these tests establish full mmap snapshot support.
- One performance sample per case/arm/source configuration: one control, then one candidate. No n3, warmup performance samples, medians, best-of selection, unchanged retries or broad campaigns. Separate correctness verification is required and is not a second performance sample.

**Validate the three exact cases, sequentially.**

| Gate | Workload | Required evidence |
|---|---|---|
| B1 | `tiny-create-500-mixed-v4`: 500 target creates on the existing 5,000-file / 500 MiB background, seed 1 | Authentic public FUSE lifecycle, exact fixture/readback, phase and total timing, resource/storage gates |
| B2 | `tiny-bulk-create-500-mixed-v3`: 5,000 new files totaling 500 MiB on a 200-file / 1 MiB witness namespace, seed 1 | Same gates, bounded transfer and large/small-file encoding; resulting namespace 5,200 files / 501 MiB |
| B3 | `local-snapshot-create-25000-onebyte-v1`: empty initial namespace, three Commits on one Workspace, then End | Compact metadata/backing, successive-Commit locality, old/new snapshot correctness, storage reuse and cleanup |

For B1/B2, reuse the registered generators and verifiers unchanged. For B3, add only the smallest adapter to the existing lifecycle runner. Create `f00000` through `f24999`, one byte `ordinal % 251`; files 0644, root 0755, normalized mtime 1700000000 seconds. C1 creates all files. C2 changes the 256 ordinals `97*j`, `j=0..255`, to `(ordinal+1)%251`. C3 restores their original bytes. Preserve the exact normalization/root-sync schedule in #149 in both arms. Three Commits are three different operations within one sample, not three statistical repetitions. This highly deduplicable case does not replace the mixed cases' encoding coverage.

Collect each control only when that gate is reached, using the sealed v0.1.5 product and comparable harness, fixtures, cache treatment and environment. Do not use historical debug timings as controls. Use the real public path, not a component microbenchmark, direct Store mutation or benchmark-specific product shortcut.

Time Begin, exec, complete Commit, visibility, End/required cleanup and complete public-call workflow according to the frozen contract. Transfer and encoding remain inside complete Commit; cleanup required by End stays in End. Full verifier reads, digests, reopen and censuses belong in separate verification, not product timers. Record compile/setup separately.

**Expect compact storage, comparable or faster execution, bounded CPU/RAM and one worker. Enforce the declared gates; do not merely check exit status.**

| Metric | Required limit |
|---|---|
| Exec, complete Commit, End and total workflow time | Candidate <= control + `max(15% of control, 3 ms)`; each B3 edit/Commit cycle independently gated |
| Total host + sandbox workflow CPU | Candidate <= control + `max(15% of control, 1 ms)` |
| Aggregate accounted algorithm allocations | 64 MiB across active host/sandbox owners; charge allocated capacity |
| Combined transfer/staging buffers | 8 MiB within the 64 MiB total; preserve the existing 8 MiB final-delta allowance within accounting |
| Process memory | Sum of relevant process peaks <= control + `max(15% of control, 8 MiB)`; also report individual peaks and kernel/cgroup usage |
| B1/B2 temporary physical backing | <= control + `max(15% of control, 1 MiB)` |
| B3 temporary physical backing | <= 32 MiB peak across host and sandbox, including retained/unlinked backing and construction temporary files |

These budgets are the spec's frozen engineering targets, not measured achievements. B2 legitimately stores 500 MiB of new payload: its disk use is not subject to B3's 32 MiB ceiling, but its RAM/staging limits still apply. Report temporary backing separately from final canonical Store growth, payload bytes and immutable fixtures. Preserve the inherited encoding/storage-efficiency proofs; do not invent a compression ratio for every input. Keep the declared 2-CPU/2-GiB/no-swap/256-PID sandbox and operation deadlines from #149. No extra worker, hidden staging copy, raised budget or extended timeout to manufacture a pass.

**Keep one evidence ledger and finish honestly.**

Use #151 or its linked append-only evidence document. Record stage, source/build/image identities, fixture/harness, exact command, worker/route counts, metrics versus limits, raw receipts, correctness and cleanup results, and next action. Retain failed and invalid attempts. Missing required metrics mean INCOMPLETE; a valid numerical miss means FAIL.

On a failure, diagnose the current gate and make the smallest relevant fix. A real source change permits one necessary new candidate sample. Reuse the control only while comparable; rerun an earlier passed gate only if the new change invalidates its applicability. Never combine incompatible candidate revisions into an all-three-pass claim.

Deliver the implementation, focused runnable checks, three-case result table, source-applicability table, removed-versus-retained behavior, remaining limitations and adoption recommendation. Passing this experiment does not automatically merge main, close #149/#150, release v0.1.6, or establish durability/cloud/mmap support. Keep progress in the existing pipeline rather than starting new architecture or benchmark campaigns.
