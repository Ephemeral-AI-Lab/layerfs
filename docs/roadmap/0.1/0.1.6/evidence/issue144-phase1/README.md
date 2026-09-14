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

PENDING — ranked after the commit/end attribution closes. The receipt
evidence so far points at route-level transport costs (exec 11.4 s backing
wait) dominating over #130 storage mechanisms (P130.2's −18 %/−23 % page
reductions are ~1–2 % of this workflow).

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
