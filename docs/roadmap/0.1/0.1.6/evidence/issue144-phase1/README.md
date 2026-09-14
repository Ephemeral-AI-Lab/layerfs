# Issue #144 Phase 1 — tiny-create-500-mixed-v4 root-cause investigation

Status: COMPLETE (single-sample diagnostics; nothing here is acceptance
evidence). Execution: #144 Phase 1, executing #130's objective on the wired
container host-authority route (#124 M1 wiring, PRs #139/#140). No product
optimization was implemented in Phase 1; this branch ships counters only.

References:
- Objective and scope: [#144](https://github.com/Ephemeral-AI-Lab/layerfs/issues/144)
- Operational handoff: [issue130-tiny-create-500-optimization-handoff.md](../../issue130-tiny-create-500-optimization-handoff.md)
- Measured starting point: ledger L44 in
  [overlay-snapshot-verification-ledger.md](../../overlay-snapshot-verification-ledger.md)
- Raw receipts (git-ignored, retained locally): `benchmark-results/issue144-phase1/runs/`

## 0. Owner inputs pinned before analysis (brief §0)

- The "v0.1.5" target is **tag `v0.1.5` in this repository**; its benchmark rows
  were produced by binary `c55daf13…` at source `1ff1f2ddd` (ancestor of the
  tag, inside the v0.1.5 release history), recorded in
  `release-notes/0.1.5/benchmark-performance.csv`. Owner confirmed 2026-09-15;
  no other artifact exists (zero occurrences of "0.1.5.1" at main `77f652812`,
  verified).
- No separately named optimization technique: the techniques are derived from
  the v0.1.5 source and receipts (§4).
- The frozen numeric target (§6) was held "awaiting owner confirmation" until
  the commit/end attribution and the tier-100 probe landed (owner decision
  2026-09-15); both have now landed and §6 proposes the staged numbers for
  confirmation. No registered criterion is weakened anywhere in this report.

## 1. Measured starting point and fresh reproduction

Ledger L44 (M2, candidate `918ad73de` = current main `77f652812` minus
docs-only commits; verified `git diff --stat` touches only `docs/`):

| arm | begin | exec | commit | visibility | end | total |
|---|---|---|---|---|---|---|
| M2 candidate median (n3) | 10.5 ms | 11,362.6 ms | 4,406.8 ms | 0.074 ms | 4,249.2 ms | 20,029.7 ms |
| M2 same-route control median (n3) | 11.1 ms | 10,980.2 ms | 4,128.9 ms | 0.067 ms | 3,983.9 ms | 19,395.8 ms |
| v0.1.5 (legacy route, single sample) | 10.733 ms | 166.715 ms | 59.709 ms | 0.107 ms | 5.395 ms | 242.660 ms |

Fresh Phase-1 instrumented samples from this branch (source `5951ab172`,
binary `186713ac…`, image `layerfs-bench-infra:2dfb9b2e54f2ff2a`, seed 1,
`--setup clone`, collection mode, one sample each — diagnostics):

| run | begin | exec | commit | visibility | end | total | note |
|---|---|---|---|---|---|---|---|
| `create500-instrumented-c1` | 13.0 ms | 7,901.5 ms | 5,376.9 ms | 0.1 ms | 4,430.6 ms | 17,722.1 ms | PASS; dirty-tree seal (pre-commit instrumentation), retained |
| `create500-instrumented-c2` | 10.5 ms | 7,694.2 ms | 4,474.5 ms | 0.1 ms | 4,340.1 ms | 16,519.4 ms | PASS; clean identity |

The two samples sit below the M2 single-sample spread (18.7–21.1 s); the M2
n3 medians remain the ledger baseline. The Phase-1 samples ran alone on an
idle machine; the difference is noted, not explained (single-sample
diagnostics; no claim made). An earlier build attempt aborted with
`source changed during qualified image build` after a docs edit landed during
the image build — operational note: do not edit the tree while a qualified
build runs; no receipt was produced by that attempt.

## 2. Instrumentation shipped in this branch (D1 + D2)

**D1 — host-authority dispatch counter.** `HostOperations` counts every
decoded host wire request by operation class (atomic increments at decode; no
allocation in the dispatch path; take-with-reset semantics). The counts
surface in the run receipt's `FuseWriteReceipt` as `host_authority_*` fields.
Focused component check: `host_runtime::tests::local_and_tcp_runtime_share_live_authority_and_published_coverage`
asserts counted lookup/write/read dispatches on both the in-process and
TCP-backed mounts.

**Exercised in a run receipt (exit criterion 1):** `create500-instrumented-c2`
records `host_authority_dispatches: 4,518` with `live_backing_calls: 4,518` —
every daemon→host backing exchange in the run was a host-authority dispatch,
proving counter-level (not just route-level) that the wired host-authority
implementation executed. Dispatch mix: lookup 1,062 · attr 1,482 · create 500
· write 450 · chmod 510 · mtime 510 · fsync 1 · pin 2 · unpin 1.

**D2 — phase splits.** `WorkspaceCommitReceipt` gains `pre_capture_ns`,
`maintain_ns` (nested), `construction_tail_ns`, `stage_ns`, `acknowledge_ns`;
`WorkspaceCommitDiagnostics` gains the `construction_*` bridge (previously the
host route's `SnapshotCandidateDiagnostics` were dropped before reaching the
receipt — the all-zero M2 diagnostics rows are explained by exactly this
gap, not by zero work). End gains a `HostEndReceipt` (`settle_ns`,
`settle_steps`, `after_detach_ns`, `state_removal_ns`). All are `Instant` +
thread-local `Copy` notes matching the existing phase mechanism: no allocation
inside timed paths, no product behavior change.

## 3. Attribution (D2) — all counters reproducible from one sample

### 3.1 Exec ≈ 15.4 ms/file (7.69 s / 47% of the c2 workflow)

| counter | v0.1.5 | wired route (c2) |
|---|---|---|
| kernel write requests | 450 | 450 (identical) |
| FUSE callbacks (sum) | ≈4,495 | ≈4,516 |
| daemon→host backing calls | **7 total** | **4,518 (~9.0/file)** |
| host-authority dispatches (D1) | — | 4,518 (= backing calls) |
| host dispatch time | 2.1 ms | **4,627 ms (1.02 ms/call)** |
| backing wait (client) | 7.5 ms | 7,191 ms (1.59 ms/call) |
| exec phase | 166.7 ms | 7,694.2 ms |

Workload shape (grounds the math): the tiny-create workload is **serial** —
per file `open(O_CREAT)` → `pwrite` → `close`, then a normalization pass
issuing `chmod(path)` + `utimensat(path)` per file, then one root `fsyncdir`.
Every dispatch is on the critical path; transport concurrency cannot help
this case. The per-file dispatch mix and its sources:

- create 500 + write 450 — the essential mutations (host-before-ack).
- chmod 510 + mtime 510 — normalization (500 files + 10 parent dirs).
- **attr 1,482 (~3/file)** — kernel getattrs (~1/file) plus a daemon-side
  **re-fetch after every SETATTR**: the SETATTR handler issues its mutation
  op and then a separate `Op::Attr` to build the kernel reply
  ([filesystem.rs setattr](../../../../crates/layerfs-fuse/src/filesystem.rs)) —
  ~2 avoidable round trips per file.
- lookup 1,062 (~2.1/file) — path resolution of the two normalization
  syscalls; v0.1.5 leaked only ~1/file (521) through the same 1 s entry TTL.
- FLUSH and RELEASE are already answered locally (stateless open) — no wire
  ops.

Serialization constants on the route (implementation, not contract): daemon
FUSE mount `n_threads = 1` ([host_mount.rs](../../../../crates/layerfs-fuse/src/host_mount.rs));
`HostClient::call_owned` holds the single connection mutex across the whole
exchange ([host_client.rs](../../../../crates/layerfs-fuse/src/host_client.rs));
host physical dispatch `backing = Semaphore::new(2)` with
`max_blocking_threads(2)` ([live_runtime.rs](../../../../crates/layerfs-fuse/src/live_runtime.rs)).
Tao's archaeology (§4, T10) confirms these constants are **identical in
v0.1.5** — they are not the differentiator; the per-op round trips are.

**Exec root-cause chain:** unchanged kernel/workload; every per-file FUSE
operation becomes one synchronous daemon→host wire round trip
(host-before-acknowledgment ownership); ~9 dispatches/file; each costs
1.02 ms host work + ~0.6 ms transport/admission on a serialized pipeline.
9.0 × 1.59 ≈ 14.3 ms/file ≈ the exec phase.

### 3.2 Commit ≈ 8.9 ms/file (4.47 s / 27%) — fully attributed (unattributed ≈ 0)

| sub-phase (c2) | time |
|---|---|
| capture (snapshot acquisition) | **0.0 ms** — the O(1) contract holds on the benchmark path |
| **pre-capture `maintain()` drain** (`maintain_ns`) | **3,404 ms** |
| candidate_plan | 419 ms |
| content (encode) | 581 ms |
| namespace | 47 ms |
| object_admission (1 txn, 473 objects) | 20 ms |
| construction_tail / stage / acknowledge / publication | 2 / 0 / 0 / 0 ms |
| unattributed | **0 ms** |

The M2 "unattributed 76.5%" is solved: it is the **pre-commit bounded
maintenance drain of reclamation work queued during exec** — the same per-item
drain that dominates End. Construction itself (plan+content+namespace+
admission ≈ 1.07 s) is single-worker (`worker_limit=1`) with O(D·log N)
per-file `view.before` lookups through a private store reader invisible to
receipts (now bridged by the `construction_*` diagnostics fields: changed_keys
1,010, file_tasks 500, full_builds 50). Insertion is already one batched
transaction — per-file store transactions are not the problem.

### 3.3 End ≈ 8.7 ms/file (4.34 s / 26%) — fully attributed

| component (c2) | time |
|---|---|
| **settle drain** (`settle_ns`) | **4,021 ms — 512 steps × 7.85 ms** |
| daemon unmount + wait (lifecycle receipt) | ≈350 ms |
| `after_detach` | ≈0 ms |
| state-directory removal | ≈0 ms (430 ms in c1 — sample variance) |

The settle drain is the C1/C2 correspondence contract's deferred
installation: the first Commit queues **every changed node (512) into the new
correspondence's pending index**, and End's bounded maintenance loop
canonicalizes them one per step (overlay snapshot → inode record → one store
read of the published object → `Description::build` → `correspondence::plan`
→ piece replace → overlay `prepare`+`install`). It is O(D) per-file work,
charged at settlement — required so the next small edit does not fall back to
full-file comparison. v0.1.5 had no such machinery and paid full-file
comparison at the next commit instead.

**The workflow's dominant block is the two serialized per-item drains**
(commit pre-capture 3.40 s + end settle 4.02 s = **7.43 s, 45% of the
workflow**), both ~7.9–8.0 ms per item, both bounded loops with identical
step shape.

## 4. Why v0.1.5 was fast — technique-by-technique (brief §5)

Reconstructed from source at `1ff1f2ddd` and receipts (Tao, this Phase; full
call-by-call decomposition in the agent record). The 7 wire calls: SEED (full
initial mirror), 3 LOOKUPs (path-walk with sibling/complete-leaf prefetch),
RESERVE (1 MiB append window), one BATCH at the workload's own `fsyncdir`
(824 KB append + all 510 dirty facts), CANCEL_RESERVATION.

| technique | phase it explains | conflict with current contracts | verdict |
|---|---|---|---|
| T1. Container-local live mirror serves all metadata mutations with zero wire ops | exec | violates host-before-acknowledgment ownership + single-authority rule | **reject** |
| T2. 1 MiB cross-file append window; write acked while bytes are container-memory-only | exec | same two contracts (an acknowledged write could be lost with the container) | **reject** as-is; batched transfer re-based on host-owned bytes is a legal adapt |
| T3. Lazy SEED + on-demand LOOKUP with sibling/leaf prefetch + immutable read cache | create, exec | none (read-only caching of immutable host state; host stays authoritative) | **adopt** (already host-authoritative in main) |
| T4. Kernel atomic-open for O_CREAT + `FOPEN_DIRECT_IO` on created handles | exec | none — unchanged in main | **adopt** (already present) |
| T5. Commit-time dirty-prefix export (FACTS + spool pushed from container) | commit | violates O(1) no-drain snapshot acquisition + single-authority | **reject** |
| T6. Host-side straight construction, single batched admission transaction | commit | construction half is already main's `build_candidate`; skipping correspondence violates C1/C2 | construction **adopt** (already main); no-correspondence **reject** |
| T7. Checkpoint install back into the live mirror across commits | commit | none (presentation-compatible) | **adopt** |
| T8. Commit pause-fence (`FREEZE`) + quiesce | commit | violates the no-pause Commit rule | **reject** |
| T9. Unmount-only teardown; spool retired at commit | end | none — same shape in main | **adopt** |
| T10. Single FUSE thread + fixed admission semaphores | — | **not a differentiator**: constants identical at main `77f652812` | N/A |

Net: v0.1.5's speed came from container-authoritative state (T1/T2/T5/T8),
all four of which are named-contract violations under the current semantics;
what survives (T3/T4/T6-construction/T7/T9) is already in main. The
regression is the route — per-op synchronous host installation — exactly as
L44 suspected. Reverting the legacy owner is not a proposable direction.

## 5. Ranked draft optimization directions (D3)

Ranked by measured share × expected effect ÷ risk. All preserve CAS/CDC/
FULL-DELTA/pack/compression, C1/C2 locality, O(1) snapshot acquisition,
exact stage/retry and Created/UpToDate receipts, the no-freeze Commit rule,
and host-before-acknowledgment ownership. Memory is bounded at **Workspace**
scope (existing `ResourcePolicy`/admission budgets), never per-file
unbounded state.

### R1 — Cut round trips per file (exec 7.69 s; → #124)

1. **SETATTR reply carries the attr** (like `Op::Create` does today): removes
   the post-setattr `Op::Attr` re-fetch — measured ~1,020 dispatches.
   O(1)/op, client+server wire change, no new state.
2. **Combined `SetAttr` wire op (mode+mtime in one)**: halves the
   normalization mutations (1,020 → 510) and the lookups that ride them.
   One mutation, one replay slot — same acknowledgment semantics.
3. **Kernel dentry/attr cache leak**: 2.1 LOOKUP dispatches/file vs v0.1.5's
   ~1.0 through the same 1 s TTL — find and fix the leak before adding
   caches; invalidation must ride the existing coherence machinery.

Expected: ~9.0 → ~5.5 dispatches/file at unchanged per-op cost → exec
≈ 4.7 s. Risk: low (1) to medium (2, protocol addition; 3, invalidation
correctness). Cost: small PRs.

### R2 — Cheaper host dispatch (1.02 ms/call, 4.63 s; → #124 + #130 P130.4)

1. **Bounded Workspace-scoped immutable metadata page cache** (P130.4-sanctioned):
   cuts the per-dispatch metadata page work (in-process reference: 48.2
   writes / 280 reads per tiny create). Charged to the Workspace budget.
2. **Inline dispatch** on the serve loop when a physical permit is free
   (skip the `spawn_blocking` handoff per frame).
3. **Admission fast path** for the default charge.

Expected: 1.02 → ≈0.5 ms/call; with R1 → exec ≈ 2.5–3 s. Risk: medium
(cache entries are never authoritative — single-authority rule). Cost: medium.

### R3 — Un-serialize the maintenance drains (7.43 s, 45% of the workflow; → #130)

Measured: commit `maintain_ns` 3,404 ms and end `settle_ns` 4,021 ms over
512 steps × 7.85 ms — two serialized per-item loops with identical shape.

1. **Batch the drain step**: one overlay `prepare`+`install` per K nodes
   (K≈64) amortizes the per-node index page writes: ~8 → ~1 ms/item. Work
   stays O(D); bounded batches keep admission exact.
2. **Parallelize independent items** across bounded Workspace-scoped workers
   (the steps are per-node independent — 512 steps × 8 ms ≈ linear).
3. **(Owner decision) overlap drains with foreground phases** on background
   host threads: real parallelism, but the plan's rule "moving cost out of
   End cannot create an apparent improvement" governs reporting — overlapped
   work must stay charged with its CPU reported. Flagged for owner/spec
   confirmation before design.

Expected: 7.43 → ≈1.5–2 s combined. Risk: medium (exact correspondence,
page ownership and rollback semantics under batching/parallelism). Cost:
medium. This is the single largest lever in the workflow.

### R4 — Commit construction (1.07 s; → #130)

1. **Bounded-worker content construction** (`worker_limit=1` today): content
   581 → ≈150–200 ms; sorted construction and deterministic output preserved
   by the existing merge.
2. **Batch the per-file `view.before` lookups** (2/file + directory
   fallback): one sorted cursor pass over the previous canonical root turns
   O(D·log N) into O(log N + D).

Expected: 1.07 → ≈0.5 s. Risk: low-medium. Cost: small-medium.

### R5 — Wire-level BATCH op for multi-operation exchanges (→ #124)

N ops per frame, per-op results in one reply, per-op sequence numbers with
exact acknowledged-prefix replay, admission by total frame bytes at the
Workspace transfer budget, serial fallback on NoSpace (the existing
`call_batch` pattern). Does **not** cut this case's serial critical path;
targets bulk cases (tier-100 probe: 28.8 ms/file), git-tool and multi-worker
cases. Compounds with R1/R2.

### R6 — Bulk-case admission failure (→ #124/#130, blocking)

The tier-100 probe (§7) failed with `scratch allocation exceeds admitted
growth` — the wired route cannot currently *complete*
`tiny-bulk-create-100-mixed-v3` under the standing `ResourcePolicy`. Whether
this is an owner policy decision or unbounded per-file scratch growth is
unresolved (§8); it blocks the registered <1 s criterion outright.

### Combined expectation and the honest floor

R1–R4 (no overlap decisions): exec ≈ 2.5 s + commit ≈ 1 s + end ≈ 1 s ≈
**4.5 s (~3.7× vs the c2 sample)**. v0.1.5 parity (242.7 ms) is **not
reachable** by these directions alone: a serial workload with
host-before-ack ownership has a floor of ~4 dispatches/file; at v0.1.5-class
per-op cost that is ≈ 0.4–0.9 ms/file ≈ 200–450 ms exec — parity needs the
full R1+R2 program *and* further dispatch-path reduction. The staged target
(§6) reflects this honestly.

## 6. Frozen numeric target (D4) — proposed, awaiting owner confirmation

Owner decision 2026-09-15: freeze only after attribution + tier-100 probe.
Both landed (§3, §7). Proposal (no registered criterion weakened):

- **Primary gate (unchanged):** no material regression under the prospective
  #118 n3 same-route paired screen (`max(15% of control median, 3 ms)` with
  ≥2/3 pairs slower; CPU analogue `max(15%, 1 ms)`); whole-workflow metric,
  phases reported separately.
- **Stage A (exec):** per-file exec cost ≥65% reduction — from ≈15.4 ms/file
  (9.0 dispatches × 1.59 ms) to ≤5.4 ms/file (≈5.5 dispatches × ≤1.0 ms), via
  R1+R2.
- **Stage B (commit+end):** combined per-file cost ≥70% reduction — from
  ≈17.6 ms/file to ≤5.3 ms/file, via R3+R4.
- **Stage C (whole workflow on create-500):** first milestone **≤5.0 s**
  (~3.3× vs c2; ~21× the v0.1.5 row), stretch **≤2.5 s**. Parity (242.7 ms)
  is explicitly *not* promised.
- **Tier-100 strict <1 s bulk create/delete:** carried **unchanged**; currently
  BLOCKED — the case cannot complete (§7). Not waived, not re-scoped; the
  admission failure must be resolved (R6) before the criterion can even be
  measured.

## 7. Tier-100 strict-criterion probe (measured, single sample)

`tiny-bulk-create-100-mixed-v3` (1,000 files / 100 MiB; v0.1.5 row 983.33 ms;
registered strict criterion <1 s), seed 1, `--setup clone`, same sealed
identity as c2:

- **Result: INCOMPLETE / FAIL.** Exec ran 28,803.9 ms (≈28.8 ms/file) and the
  product then failed with `Workspace(Storage(Io(Custom { kind: Other,
  error: "scratch allocation exceeds admitted growth" })))` — a bounded
  admission failure during the bulk create exec. No commit/end phases ran;
  the timing criterion is therefore unmeasured as a pass/fail, but at
  28.8 ms/file the <1 s criterion (≤1 ms/file all phases) is decisively out
  of reach at the current per-file route cost, independent of the admission
  failure.
- Raw receipt: `benchmark-results/issue144-phase1/runs/bulkcreate100-probe-c1/`
  (INCOMPLETE, `verification_status: NOT_RUN`, exit error retained).

## 8. Open unknowns / blocked experiments (D5)

1. **Bulk-100 admission failure (R6):** exact failing allocation site and
   whether the scratch growth is linear-in-input (policy decision needed) or
   superlinear (scaling defect). Blocked on product decision/fix — routed to
   #124/#130; Phase 1 did not touch `ResourcePolicy`.
2. **Drain composition at commit time:** the pre-capture `maintain()` drained
   ≈3.4 s before the *first* commit — the exact pending-item mix (reclamation
   queued per exec operation vs correspondence) is inferred from the step
   shape (same 8 ms/item as the End settle) but not yet itemized; one more
   counter level (per-queue step counts) would pin it.
3. **Lookup leak (R1.3):** why 2.1 LOOKUP dispatches/file cross with a 1 s
   entry TTL when v0.1.5 leaked ~1.0 — mount/reply configuration difference
   not yet identified.
4. **Sample variance:** c1 vs c2 pre-capture maintain (4.07 vs 3.40 s) and
   state-removal (430 vs 0 ms); both single samples. The M2 n3 medians remain
   the baseline; Phase-2 terminal evidence uses the paired screen.
5. **Host process CPU during drains:** not yet measured; needed if R3.3
   (overlap) is pursued, to report overlapped work honestly.

## 9. Ownership routing of the findings (per #144's ownership section)

- R1, R2 (round trips, dispatch cost, wire protocol) → **#124** (route);
  reconcile per-FUSE-request overhead findings with **#70** (its git-tool
  targets unchanged).
- R3, R4, R6 (drains, construction, admission policy) → **#130** (storage
  mechanisms), re-aimed at the now-measured dominant costs.
- The tier-100 probe failure and the benchmark-facing mirror → **#125** note.
- #130, #124, #125, #144 all remain OPEN; nothing here closes or re-scopes
  them; no release/tag/deployment; 25k/two-second and million-file
  obligations remain DEFERRED/OPEN.
