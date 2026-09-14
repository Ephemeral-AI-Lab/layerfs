# Issue #144 Phase 1 — tiny-create-500-mixed-v4 root-cause investigation

Status: DRAFT in progress (single-sample diagnostics only; nothing here is
acceptance evidence). Execution: #144 Phase 1, executing #130's objective on
the wired container host-authority route (#124 M1 wiring, PRs #139/#140).

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
  no other artifact (no "v0.1.5.1" exists anywhere in the repository — verified
  by `git grep` at main `77f652812`).
- No separately named optimization technique: the techniques are derived from
  the v0.1.5 source and receipts (§4), not from memory or assumption.
- The frozen numeric target (brief §4) is **awaiting owner confirmation** until
  the commit/end attribution and the tier-100 bulk probe land (owner decision
  2026-09-15). Nothing in this report weakens a registered criterion.

## 1. Measured starting point and reproduction

Ledger L44 (M2, candidate `918ad73de` = current main `77f652812` minus
docs-only commits; verified: `git diff --stat 918ad73de..77f652812` touches
only `docs/`):

| arm | begin | exec | commit | visibility | end | total |
|---|---|---|---|---|---|---|
| candidate median (n3) | 10.5 ms | 11,362.6 ms | 4,406.8 ms | 0.074 ms | 4,249.2 ms | 20,029.7 ms |
| same-route control median (n3) | 11.1 ms | 10,980.2 ms | 4,128.9 ms | 0.067 ms | 3,983.9 ms | 19,395.8 ms |
| v0.1.5 (legacy route, single sample) | 10.733 ms | 166.715 ms | 59.709 ms | 0.107 ms | 5.395 ms | 242.660 ms |

Fresh Phase-1 reproduction from main + the D1 dispatch counter (this branch):
PENDING — filled from `benchmark-results/issue144-phase1/runs/`.

## 2. D1 — host-authority dispatch counter (dispatch proof)

- `HostOperations` now counts every decoded host-authority wire request by
  operation class (atomic increments at decode; no allocation in the dispatch
  path; take-with-reset semantics matching the transport metrics).
- The counts surface in the run receipt's `FuseWriteReceipt` as
  `host_authority_*` fields (commit-op record, covering the exec window),
  alongside the existing transport counters.
- Focused component check: `host_runtime::tests::local_and_tcp_runtime_share_live_authority_and_published_coverage`
  extended to assert counted lookup/write/read dispatches on both the
  in-process and TCP-backed mounts.
- Exercised in a run receipt: PENDING (fresh run below).

### 2.1 D2 phase-split instrumentation added in this branch

- Commit `WorkspaceCommitReceipt` gains `pre_capture_ns`,
  `construction_tail_ns`, `stage_ns`, `acknowledge_ns` (previously inside
  `unattributed_ns`), and `WorkspaceCommitDiagnostics` gains the host-route
  construction bridge fields (`construction_*`) populated from the previously
  dropped `SnapshotCandidateDiagnostics`.
- End gains a `HostEndReceipt` (`settle_ns`, `settle_steps`,
  `after_detach_ns`, `state_removal_ns`) recorded from
  `end_workspace_session` on the host route; the daemon-side
  `WorkspaceLifecycleReceipt` is unchanged.
- All are `Instant` + thread-local `Copy` notes matching the existing phase
  mechanism: no allocation inside timed paths, no product behavior change.

## 3. Attribution (D2) — receipts and counters

### 3.1 Exec ≈ 23 ms/file (11.4 s / 57% of the workflow)

Retained M2 receipt (candidate-c1, diagnostic) and the identical-kernel-workload
v0.1.5 receipt:

| counter | v0.1.5 | wired route (M2) |
|---|---|---|
| kernel write requests | 450 | 450 (identical) |
| FUSE callbacks (sum) | ≈4,495 | ≈4,516 |
| daemon→host backing calls | **7 total** | **4,538 (~9.1/file)** |
| backing request bytes | — | 1,055,720 (~233 B/call) |
| backing wait (client) | 7.5 ms | **11,404 ms** |
| host dispatch (server) | 2.1 ms | **7,353 ms (1.62 ms/call)** |
| socket write + read | 2.0 ms | 346 ms |
| client queue | 0.18 ms | 72.8 ms |
| exec phase | 166.7 ms | 12,017.6 ms |

Callback mix (both routes): create 500, write 450, setattr 1,020 (chmod +
mtime normalization per file), flush 500, release 500, lookup (521 v0.1.5 /
1,088 wired), getattr (501 / 454), opendir/releasedir/fsyncdir 4.

Source-traced serialization constants on the wired route (not contract
requirements):

- daemon FUSE mount `n_threads = 1`, `clone_fd = false`
  (`crates/layerfs-fuse/src/host_mount.rs`, `mount_host`)
- `HostClient::call_owned` holds the single connection mutex across the whole
  request/reply exchange (`crates/layerfs-fuse/src/host_client.rs`)
- host-side physical dispatch: `backing = Semaphore::new(2)` and
  `max_blocking_threads(2)` on the process-wide `LiveRuntime`
  (`crates/layerfs-fuse/src/live_runtime.rs`)

Root-cause chain (exec): the workload/kernel side is unchanged; every per-file
FUSE operation becomes one synchronous daemon→host wire round trip
(host-before-acknowledgment ownership); the pipeline is serialized end-to-end
by the three constants above; each host dispatch performs real authority work
(metadata page I/O; in-process reference: 48.2 page writes / 280 page reads
per tiny create). 9.1 calls/file × ~2.5 ms ≈ 23 ms/file.

### 3.2 Commit ≈ 8.8 ms/file (4.4 s / 22%)

M2 candidate receipt `WorkspaceCommitReceipt` (existing instrumentation):

| sub-phase | time |
|---|---|
| capture (snapshot acquisition) | **125 ns** — O(1) contract holds on the benchmark path |
| candidate_plan | 418.4 ms |
| content (encode) | 586.3 ms |
| namespace | 44.3 ms |
| object_admission (1 txn, 473 objects) | 19.9 ms |
| publication | 0.3 ms |
| **unattributed** | **3,485.1 ms (76.5%)** |

CandidateStats: 480 objects / 900,259 B; 473 inserted (batched, single
admission transaction); 7 reused. v0.1.5 commit for the same workload: 59.7 ms,
fully attributed in its receipt (content 26.5 ms, output pipeline 9.9 ms,
output admission 6.0 ms, namespace 8.3 ms, object admission 13.0 ms, legacy
pause_fence 1.3 ms, capture 24 µs).

Source inventory of the unattributed region (all four regions now carry
counters added in this branch — see §2.1):

- **Pre-capture** (`lifecycle.rs::commit_host_session` before the capture
  closure): presentation check, read-metrics snapshot, `host.maintain()`
  (bounded maintenance drain of the *previous* published context),
  `record_write_metrics` (a control round trip to the daemon), `published_head`.
- **Construction tail** (`snapshot_candidate.rs` after the namespace note):
  `objects.finish` (per-object identity authentication + child sort),
  correspondence bookkeeping, `workspace_admission` session setup.
- **Stage** (`workspace.rs::stage_workspace_root`): exact-stage insert
  transaction.
- **Acknowledge** (`commit_attempt.rs::acknowledge_workspace_publication`).

Cross-case evidence (stat500 control, same route, 500 attr-only changes,
UpToDate): commit total 456 ms of which 455 ms unattributed — so the
unattributed region has a fixed ≈0.46 s component plus ≈6.1 ms per *created*
file in create-500. The new counters split these from one sample.

Also established from source: construction reads the previous canonical root
through a **private store reader** whose metrics never reach any receipt
(`store.snapshot_reader()` mints a fresh metrics Arc) — the 418 ms plan phase
does O(D·log N) `view.before` lookups ×2 per file plus `directory_lookup`, all
invisible to receipts today. Construction is **single-worker**
(`worker_limit=1` on the host route). Insertion is already batched (one
transaction, 473 objects) — per-file store transactions are NOT the commit
problem.

### 3.3 End ≈ 8.5 ms/file (4.2 s / 21%)

M2 candidate receipts: daemon-side `WorkspaceLifecycleReceipt` (End) totals
348.1 ms (unmount 330.3 ms, wait 17.7 ms); the operation's service time is
4,537 ms, leaving ≈ **4.16 s of host-side work** without a phase split.

Source inventory (now counter-split by this branch — `HostEndReceipt`):

- **Settle** (`host.maintain()` at End, Clean mode): the first Commit queues
  **every changed node (D=500) into the new correspondence's pending index**
  (`snapshot_candidate.rs::queue_maintenance`); End drains it with a bounded
  maintenance loop whose step canonicalizes exactly one node — per-node work:
  overlay snapshot, inode record, ranges restore, **one store read of the
  published small-file object** (the 451 calls / 835 KB in op-6's read
  receipt), `Description::build`, `correspondence::plan`, per-span piece
  replace, `overlay.prepare`+`install`. This is **O(D) per-file work at End**
  and matches the ≈8.3 ms/file scaling. It is the C1/C2 correspondence
  contract's deferred installation (required so the next small edit does not
  fall back to full-file comparison) — charged at settlement, not hidden.
- **`after_detach`**: retained-attempt resolution + `detach_kernel()` — loops
  remaining kernel references 128/page (≈500 releases here).
- **State removal**: `remove_dir_all` of the per-Workspace state directory
  (spool, arena/catalog files, journals, scratch).

v0.1.5 end: 5.4 ms total (unmount 1.4 ms + wait 1.1 ms — no correspondence
machinery existed; the next commit paid full-file comparison instead).

### 3.4 Workload shape (grounds the round-trip math)

The tiny-create workload is **serial** (one syscall chain at a time;
`ordinary_workloads.rs::apply`): per file `open(O_CREAT|O_WRONLY)` → `pwrite`
→ `close`, then a final normalization pass issuing `chmod(path)` +
`utimensat(path)` per file, then one root `fsyncdir`. Path-based normalization
resolves the name each time → kernel LOOKUP + SETATTR per call. This is why a
serial workload cannot be helped by transport *concurrency*: each syscall
blocks on its own reply. The levers are **fewer operations per file** and
**cheaper per-operation cost** (§5).

## 4. Why v0.1.5 was fast — technique-by-technique (brief §5)

PENDING — source reconstruction with file:line at `1ff1f2ddd` (agent
in progress), receipt support from §3. Will include: technique | phase |
measured support | applicability to the wired route | what it would break
(CAS/CDC/FULL-DELTA/pack/compression, C1/C2, ownership, receipts, V1,
host-before-ack, no-freeze) | verdict (adopt / adapt / reject).

## 5. Ranked draft optimization directions (D3)

Ranked by measured share × expected effect ÷ risk. All preserve CAS/CDC/
FULL-DELTA/pack/compression (untouched), C1/C2 locality, O(1) snapshot
acquisition, exact stage/retry and Created/UpToDate receipts, the no-freeze
Commit rule, and host-before-acknowledgment ownership. Memory bounds are
Workspace-scoped and charged to the existing `ResourcePolicy`/admission
budgets — never per-file unbounded state. Measured shares are from the
instrumented single sample (§1, diagnostic); Phase 2 re-verifies per change.

### R1 — Cut round trips per file on the mounted path (exec; → #124)

Measured basis: 4,521 host-authority dispatches for 500 files (~9/file, D1
receipt), each a serialized TCP exchange; the workload is serial so every
dispatch is on the critical path.

- **R1a. Drop redundant attribute fetches on OPEN.** The daemon's
  `prepare_kernel_open`/`validate_kernel_open` each issue `Op::Attr` — ~2
  extra round trips per file (measured: `attr` dispatches 1,482 ≈ 3/file vs
  ~1/file that the kernel needs). The CREATE/LOOKUP reply already carries the
  attr. Complexity O(1)/op; no new state.
- **R1b. Combined `SetAttr` wire op (mode+mtime in one request).** The
  benchmark's normalization issues `chmod` + `utimensat` per file (measured:
  chmod 510 + mtime 510). One op halves these and the path-resolution
  LOOKUPs that ride with them. One mutation = one replay slot (same
  acknowledgment semantics as today's Chmod/Mtime).
- **R1c. Answer FUSE FLUSH locally.** Every WRITE is host-owned before its
  acknowledgment (the decided V2 contract), so FLUSH has no unacked data to
  push and no deferred error to report; measured flush callbacks: 500.
  `Op::Fsync` remains the explicit durability path. This *relies on* the
  host-before-ack contract rather than weakening it.
- **R1d. Kernel dentry/attr caching.** 1,065 LOOKUP dispatches (~2.1/file)
  vs v0.1.5's 521 — both routes use a 1 s entry/attr TTL, so the difference
  is in the mount/reply configuration; fix the leak before adding caches.
  Requires correct invalidation through the existing coherence machinery.

Expected: ~9 → ~4 dispatches/file; with per-dispatch cost unchanged,
exec ≈ 7.9 s → ≈ 4 s. Risk: low (R1a/c are pure client-side waste removal);
R1b is a wire-protocol addition (versioned, first-party); R1d needs an
invalidation-correctness check. Cost: small, reviewable PRs.

### R2 — Cheaper host dispatch (exec; → #124 + #130 P130.4)

Measured basis: `host_dispatch_ns` 7.35 s / 4,538 calls ≈ 1.62 ms per
dispatch in the retained M2 sample — the host authority's own per-op work
(metadata page copies; in-process reference 48.2 writes / 280 reads per
tiny create).

- **R2a. Bounded Workspace-scoped immutable metadata page cache** (explicitly
  sanctioned by P130.4): cuts re-reads in the dispatch path; evictable pages,
  disk ownership model unchanged; charged to the Workspace budget.
- **R2b. Inline dispatch on the serve loop** when a physical permit is
  immediately available (skip the `spawn_blocking` handoff per frame).
- **R2c. Admission fast path** for the default charge (`budget.enter`).

Expected: 1.62 → well under 1 ms per dispatch; combined with R1,
exec ≈ 1.5–2.5 s. Complexity: O(1)/op with bounded memory; cache is
O(budget) at Workspace scope. Risk: medium (cache coherence must respect
the single-authority rule — cache entries are never authoritative, only
installed immutable pages). Cost: medium.

### R3 — Un-serialize the two maintenance drains (commit + end; → #130)

Measured basis (new counters, this branch): commit `pre_capture_ns`
4,072 ms (dominated by the pre-commit `maintain()` drain of reclamation
queued during exec) and End `settle_ns` 4,104 ms over 512 steps (≈ 8.0 ms
per correspondence-canonicalization step). Together ≈ 8.2 s of the 17.7 s
workflow — the single largest block, and pure per-item serialization.

- **R3a. Batch the drain step.** Each step does per-node overlay
  `prepare`+`install` (index page writes per node). Preparing/installing one
  root per K nodes (K=64) amortizes the page writes: 8.0 → ≈ 1 ms/item.
  Work stays O(D); bounded batches keep admission exact.
- **R3b. Parallelize independent drain items** across bounded workers
  (Workspace-scoped, like the existing construction worker formula):
  wall ≈ drain_time / workers.
- **R3c. (Owner decision needed) overlap drains with foreground phases** on
  background host threads. The plan's rule "moving cost out of End cannot
  create an apparent improvement" governs reporting: overlapped work must
  still be charged and its CPU reported. Flagged for owner/spec confirmation
  before design.

Expected: 8.2 s → ≈ 1–2 s combined (R3a+R3b alone). Risk: medium — batching
and parallelism must preserve exact correspondence semantics (C1/C2), page
ownership and rollback; the per-step algorithm is already independent per
node, which the counters confirm (512 steps × 8 ms ≈ linear). Cost: medium.

### R4 — Commit construction (→ #130)

Measured basis: plan 477 ms + content 721 ms + namespace 78 ms + admission
25 ms ≈ 1.3 s (capture is 125 ns — the O(1) contract holds); insertion is
already one batched transaction (473 objects); no per-file store
transactions exist.

- **R4a. Bounded-worker content construction** (host route pins
  `worker_limit=1` today): content 721 → ≈ 200 ms at 4 workers; sorted
  construction and deterministic output preserved by the existing merge.
- **R4b. Batch the per-file `view.before` lookups** (2 per file + directory
  fallback): one sorted cursor pass over the previous canonical root turns
  O(D·log N) into O(log N + D).

Expected: 1.3 s → ≈ 0.5 s. Risk: low-medium (determinism and exact
predecessor comparisons unchanged). Cost: small-medium.

### R5 — Wire-level batching for multi-operation exchanges (→ #124)

A host-wire `BATCH` op: N operations per frame, per-op results in one reply,
per-op sequence numbers with exact acknowledged-prefix replay, admission by
total frame bytes at the Workspace transfer budget, serial fallback on
NoSpace (the existing `call_batch` pattern). **This does not cut the serial
tiny-create critical path** (each syscall blocks on its own result) — it
targets bulk cases (45k dispatches at tier-500), git-tool and future
multi-worker cases, and compounds with R1/R2 for the tier-100 criterion.

### R6 — End state-directory removal (→ #130, minor)

`state_removal_ns` 431 ms: `remove_dir_all` of the per-Workspace state
directory (arena/catalog/journal files from exec churn). After R3 shrinks
per-item page churn, the file count drops; remaining cost is honest
per-Workspace cleanup.

### Combined expectation and the honest floor

With R1–R4 (no overlap decisions): exec ≈ 2 s + commit ≈ 1 s + end ≈ 1 s
≈ **4 s total (~4.4×)**. The v0.1.5 row (242.660 ms) is **not reachable** by
these directions alone: a serial workload with host-before-acknowledgment
ownership has a floor of ~3 round trips/file; at v0.1.5-class per-op cost
(~0.1 ms) that is ≈ 0.3 ms/file ≈ 150 ms exec — parity would require
Route-level per-op costs two orders below today's, i.e. the full R1+R2
program *plus* further dispatch-path work. The staged target (§6) reflects
this honestly.

## 6. Frozen numeric target (D4)

Awaiting owner confirmation (owner decision 2026-09-15): the staged target is
frozen only after the commit/end attribution and the tier-100 bulk probe land.
Candidate stage structure from the brief (§4): Stage A exec per-file
reduction, Stage B commit/end per-file reduction, Stage C whole-workflow
factor vs the v0.1.5 row; the registered strict <1 s tier-100 bulk
create/delete criterion is carried unchanged.

## 7. Tier-100 strict-criterion probe

PENDING — one diagnostic sample of `tiny-bulk-create-100-mixed-v3`
(1,000 files / 100 MiB; v0.1.5 row 983.33 ms; registered strict <1 s criterion).

## 8. Open unknowns / blocked experiments (D5)

PENDING.
