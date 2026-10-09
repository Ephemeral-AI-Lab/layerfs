# R7 research: the request path in layerfs-fuse and layerfs-daemon outside the SQL job

> **Status:** read-only research, 2026-10-09. Product source read only at `63c48d8dc`
> (`git show` / `git grep`; a byte copy of those blobs is in the scratch directory
> `research/request-path/src/`). Nothing was built, run or edited. No receipt was produced.
>
> Labels: **[S]** read from source at 63c48d8dc (file:line), **[R]** decoded from a named receipt,
> **[I]** inference (including every statement about Linux kernel behaviour, which I recall and did
> not read), **[E]** estimate.
>
> Path prefixes: `F/` = `core/crates/layerfs-fuse/src/`, `D/` = `core/crates/layerfs-daemon/src/`,
> `W/` = `core/crates/layerfs-workspace/src/`, `O/` = `core/crates/layerfs-overlay/src/`,
> `V/` = `core/vendor/fuser-0.18.0/src/`. Receipts are under
> `core/docs/issues/307/checks/r7-optimization-20261009/`.

## 0. Result in one page

1. **The user-space fixed cost outside the owner job is real in counts but not visible in time.**
   Counted from source, one GETATTR takes 21 lock/unlock pairs and 16 heap allocations outside
   `command.perform` (CREATE 24 / 21, WRITE 23 / 19, RELEASE 20 / 14). But L's time outside the owner is
   43.0 µs per request in C01 [R 659], and a passthrough with no product code spends 42.9 µs per request
   outside its own CPU on the same request stream [R 726-floor-analysis/floor.md §1.4]. One synchronous
   FUSE round trip costs 39.4 µs here before any work [R 723-floor-S01]. Trimming locks and allocations
   (P3–P7) acts on at most 3–5 µs per request [E] and cannot be shown by one cell sample; it needs a
   count or a per-thread CPU counter as its proof.
2. **What costs measurable time in my subject is collisions on the single turn, not locks.**
   C03: 3,540 requests found the turn held by a maintenance turn, were served by the owner thread and
   then hopped to a pool worker to reply (`steps 12,559` against `completed 9,020`, `wakes 3,540`
   [R 667]). One cross-thread wake is 17.9 µs [R 724-floor-PIPE]. L's outside-owner time in C03 is
   67 ms above the passthrough's, and 3,540 × 17.9 µs = 63 ms [R arithmetic, §2.3].
   **Proposal D: the owner thread keeps the continuation of a job it serves** (it replies itself instead
   of waking a worker). No new thread, no memory, no gating change; predicted C03 steps run by
   pool workers 3,540 → about 0, −55 to −65 ms [E]. (`wakes` itself counts notifications,
   `F/dispatch/task.rs:81`, and stays 3,540; the hand-off to a worker has no counter today.)
3. **P8 (gate `Shared::pass`): no event-only gate removes the per-job owner wake and still guarantees
   idle reclamation.** At the end of the last job the daemon's state is the same whether another request
   follows 45 µs later or never; telling them apart needs a timer or a device probe, both excluded. The
   8-turn bound is untouched by every option below.
4. **P1 (RELEASE reply first): reordering alone gives nothing, and I found no variant without a probe
   that is worth more than about 10–20 µs per file [E].** RELEASE is already asynchronous for the client;
   its close already overlaps the client's own time and saves one daemon sleep per file
   [R floor.md H2]. The close costs 32 µs of the single writer [R 659], more than any one idle window
   (about 24–30 µs [E]). The direct lever is the close job's own service time (32 µs for four statements
   against 8.5 µs for a read job [R 659]); that belongs to the SQL research.
5. **P2 (GETATTR from a record) removes an owner job, never a request**: about 11–13.5 µs per hit [E],
   1,000 hits in C01, 2,000 in C02, 3,000 in C03. It is a cache with a contract change (`F/coherence/
   reply_order.rs:1-9`) and an audit of every inode writer; rank it last.
6. **No request in C01 can legitimately not be sent** under `default_permissions` and the 60 s lifetime
   [I]. Requests that could go elsewhere: COPY_FILE_RANGE implemented (C07: 1,024 WRITEs of 64 KiB,
   C10: 512 READs + 1,024 WRITEs become at most 512 requests each [I]), NO_OPEN_SUPPORT (C12 −458),
   NO_OPENDIR_SUPPORT (C05 −244, C04 −20). All need product work outside the request path.
7. **One receive loop is safe for provider reads** (all five provider sites are preceded by
   `LeaveReceiver`), but no mounted test pins any step/wake/queue count. The cheapest regression test is
   a serial mounted flow asserting `steps == completed`, `wakes == 0`, owner `peak_queued == 1` from
   Status, which is exactly what receipts 659 and 663 show today.

## 1. Task 1 — one request on the receive thread, `read()` to `read()`

### 1.1 Common frame (every opcode)

| # | Step | Source [S] | Locks | Allocs | Clock |
| --- | --- | --- | --- | --- | --- |
| 1 | `read()` of `/dev/fuse` into the loop's one buffer (allocated once per loop, 16 MiB + 4,096 virtual) | `V/session.rs:543,549`, `V/channel.rs:31`, `V/read_buf.rs:8,25` | 0 | 0 | 0 |
| 2 | Sender clone (one `Arc` count), parse, second parse in dispatch, `debug!` level check (no logger installed in the daemon [I]) | `V/session.rs:550,553,557`, `V/request.rs:58-71` | 0 | 0 | 0 |
| 3 | Callback: opcode counter, `queue.receive()`, `received.admit(bytes)` | `F/request/state.rs:65,66,74`; `F/dispatch/admission.rs:50-62,179-230` | dispatch `State` ×2 | 0 | 0 |
| 4 | Build the request future (`Box::pin`), `Permit::handoff`: `Task::new` (`Arc::new_cyclic` + `Arc::new(Notification)`), lock, first step `advance(task, shared, true, true)` | `F/request/state.rs:159`, `F/request/mutate.rs:155`; `F/dispatch/admission.rs:253-300`; `F/dispatch/task.rs:49-58,203-225` | dispatch `State` ×1, `Task.future` ×1 | 3 | 0 |
| 5 | `services.request(&fence)`: `operation()` locks the snapshot and **clones the whole `BranchSnapshot`** (its `HistoryName(String)` is a heap clone) to read `.scope`; `ports_for` builds `Arc<StorePorts>` and `Arc<CanonicalClient>`; `workspace.scoped()` takes the base `RwLock` and clones the `BaseView`; `Arc::new(FilesystemPort)` | `D/service/filesystem_port.rs:40-42`; `D/store/operation.rs:26-41`; `D/store/ports.rs:61,89`; `W/workspace/state.rs:90-112` | snapshot ×1, base ×1 | 4 | 0 |
| 6 | Owner job submit: job future `Box::pin`, `complete` future `Box::pin`, `submit_when_available` (owner `State` pre-check), `Notifications::reserve`/`arm`/`release`, `try_submit` (owner `State`), `completion::admit` (`Arc<Cell>`), `Box::new(Job{admitted: Instant::now()})` | `D/service/filesystem_port.rs:76,85-98,107`; `D/overlay/admission.rs:52,65,75,122-169,222-245`; `D/overlay/owner.rs:251-379`; `D/service/completion.rs:76-85` | owner `State` ×2, slots ×3 | 4 | 1 |
| 7 | Inline turn on this thread: `serve_submitted` (connection lock), `serve`: `stopped()`, `Turn::begin`, `elapsed(job.admitted)`, `Instant::now()`, **`command.perform` (the SQL job, not counted here)**, `turn.settle` (`JobSql::accumulate` allocates the per-family rows), `elapsed(start)`, `sql_progress`, `progress`, `publish(Box::new(Outcome))` (`Cell.notifier`), `pass` | `D/overlay/owner.rs:476-547`; `D/overlay/queue.rs:268,372-389,434-436,454-472`; `D/service/job_sql.rs:65-83`; `D/service/completion.rs:95-117` | connection ×1, owner `State` ×4, notifier ×1 | 2 | 3 |
| 8 | Back in the future: `Pending::poll` takes the value, `ServiceReply::new` boxes the completion | `D/service/completion.rs:179-202`; `F/ports.rs:144-151`; `D/service/filesystem_port.rs:111` | notifier ×1 | 1 | 0 |
| 9 | **Reply**: one `writev` on `/dev/fuse`, no heap | `V/channel.rs:87`, `V/reply.rs` | 0 | 0 | 0 |
| 10 | After the reply: receipt drop (frees completion, `Cell`, `Outcome`, rows; `Credit::drop` locks owner `State` and calls `admission.notify()`, which returns at once when nothing is reserved), future drop, completion lock, `Task` and `Notification` freed | `D/overlay/credits.rs:48-59`; `D/overlay/admission.rs:86`; `F/dispatch/task.rs:273-274,289-297` | owner `State` ×1, dispatch `State` ×1 | 0 | 0 |
| 11 | Kept-step check, return to `read()` | `F/dispatch/admission.rs:290-297` | (inside the lock of 10) | 0 | 0 |

Per job there are also about five passes over a 1,904-byte `DatabaseWork` (14 × 17 `u64`) [S
`O/diagnostics/metrics.rs:200-241`, `D/overlay/queue.rs:268,454-472`], and inside the job two clock reads
per SQL statement [S `O/diagnostics/metrics.rs:91,150`].

### 1.2 Per opcode, as it differs

- **GETATTR** [S `F/request/callbacks.rs:83-98`, `F/operations/lookup.rs:191-239`,
  `D/service/filesystem_port.rs:363-393`, `W/operations/native_visit.rs:162-190`]: adds
  `Arc::new(VisitFacts)`, `Box<NativeReadVisit>`, a second base `RwLock` read. One owner job (class Read).
  Value → attr reply (`F/request/reply.rs:124`) → `dispose`.
- **Negative LOOKUP** [S `F/request/callbacks.rs:48-79`, `F/operations/lookup.rs:200,217`,
  `F/request/reply.rs:72`]: adds a `PathName` copy on the loop (`:67`) and a second one in
  `operation.clone()`. The visit returns `Refused`; the receipt and its credit are dropped **before** the
  ENOENT reply. No entry is cached (no negative lifetime).
- **CREATE** [S `F/request/callbacks.rs:552-573`, `F/request/mutate.rs:139-240`,
  `F/operations/mutation.rs:203-285`, `D/service/filesystem_port.rs:394-415,457-464,490-501`,
  `W/workspace/serials.rs:25-53`, `O/lifetime/tickets.rs:50-55,97-112`]: adds `Vec::with_capacity(N)`
  for names, `PathName`, the builder `Box`, `SystemTime::now()`, `reserve_serial` (serial `Mutex`; a
  synchronous History write once per 1,024 serials, `SERIAL_REFILL`), `input.clone()` (second
  `PathName`), `Box<visit>`, a reply ticket issued inside the job (tickets `Mutex`) and, after the reply,
  `reply_attempted` (tickets `Mutex` again, `Box::pin(ready(..))`, no owner job).
- **WRITE** [S `F/request/callbacks.rs:511-551`, `F/operations/write.rs:17-26`,
  `W/operations/types.rs:10`]: the request bytes are copied once into an `Arc<[u8]>` (`data.into()`,
  131,072 + 16 bytes for a full window), plus a `Box<closure>`. One Mutation job. Then ticket and
  `reply_attempted` as CREATE.
- **RELEASE** [S `F/request/callbacks.rs:168-219`]: the future awaits `services.close_file(..)` (`:194`,
  one Lifecycle job, `D/service/filesystem_port.rs:279-293`) and its credit drop, **then** `reply.ok()`
  (`:200`). The close precedes the reply.
- **FORGET** [S `F/request/inline.rs:42-87`]: no reply; one Lifecycle job (`services.forget`, `:69`).
- **UNLINK**: as CREATE without the serial reservation.
- **OPEN** [S `F/operations/lookup.rs:240-310`]: `source` job, `observe` job, release job(s): 3–4 owner
  jobs, each with the per-job part of the frame (rows 6–8: about 12 lock pairs, 7 allocations).
- **READ** [S `F/operations/read.rs:64-91`]: `source`, `observe`, `local_read`, releases (6 owner jobs in
  C09: 3,082 jobs for 512 READs [R 691]), an **unconditional** `LeaveReceiver` (`:76`) before
  `immutable` (`:77`), a `Vec::with_capacity(length)` (`:83`) and a clone of the local bytes (`:85`);
  two reader grants per READ (1,026 for 512 READs [R 691]). Another agent is reworking this path.

### 1.3 Corrected count table (outside `command.perform`)

| Opcode | Lock pairs (earlier note) | Allocations (earlier note) | Clock reads | Before the reply | After the reply |
| --- | --- | --- | --- | --- | --- |
| GETATTR | **21** (21) | **16** (14) | 4 | all but row 10–11 | receipt and future drop, completion lock |
| Negative LOOKUP | **21** (21) | **18** (16) | 4 | everything incl. receipt/credit drop | future drop, completion lock |
| CREATE | **24** (24) | **21** (19) | 4 + 1 `SystemTime` | all but ticket attempt | `reply_attempted` (1 lock, 1 alloc), drops |
| WRITE | **23** (23) | **19** (17) | 4 + 1 | as CREATE | as CREATE |
| RELEASE | **20** (20) | **14** (12) | 4 | everything incl. the close job and credit drop | future drop, completion lock |
| UNLINK | 23 | about 21 | 4 + 1 | as CREATE | as CREATE |
| FORGET | 20 | 14 | 4 | (no reply) | — |

GETATTR's 21 pairs [S]: dispatch `State` ×4 (receive, admit, handoff, completion) + `Task.future` ×1;
snapshot ×1; base `RwLock` ×2; owner `State` ×7 (pre-check, `try_submit`, `stopped`, `sql_progress`,
`progress`, `pass`, `Credit::drop`); `Notifications.slots` ×3; connection ×1; `Cell.notifier` ×2.
GETATTR's 16 allocations [S]: request future, `Task`, `Notification`, snapshot `String`, `StorePorts`,
`CanonicalClient`, `FilesystemPort`, `VisitFacts`, `Box<visit>`, job future, complete future, `Cell`,
`Job`, `JobSql` rows, `Outcome`, `ServiceReply` box. **The lock counts of the earlier note are
confirmed; its allocation counts were two low** in every column (the `BranchSnapshot` string clone and
the `Arc<Notification>` were missed). Its file:line references are stale.

No lock is contended in a serial flow (C01: `peak_queued 1`, `wakes 0` [R 659]).

## 2. Time per unit from receipts

### 2.1 Product cells at 63c48d8dc (class B, arm L, one sample each)

| Cell | Receipt | Command ms | Requests | Owner jobs | Owner wait / service ms | Outside owner ms (µs per request) | steps / completed / wakes | Maintenance turns |
| --- | --- | ---: | ---: | ---: | --- | --- | --- | ---: |
| C01 | 659 | 379.7 | 5,001 | 5,001 | 1.7 / 162.9 | 215.1 (43.0) | 5,000 / 5,000 / 0 | 0 |
| C02 | 663 | 914.8 | 6,002 | 6,002 | 2.3 / 175.4 | 737.1 | 6,001 / 6,001 / 0 | 0 |
| C03 | 667 | 715.9 | 9,021 | 9,108 | 59.3 / 300.4 | 356.2 (39.5) | 12,559 / 9,020 / 3,540 | 6,011 (142.5 ms) |
| C04 | 671 | 823.4 | 5,342 | 5,382 | 3.2 / 170.9 | 649.3 | 5,351 / — / 10 | 10 |
| C05 | 675 | 880.5 | 5,889 | 7,264 | 14.8 / 230.5 | 635.2 | 6,675 / — / 804 | 233 (8.7 ms) |
| C06 | 679 | 226.8 | 517 | 517 | 0.3 / 147.9 | 78.6 (152) | 516 / 516 / 0 | 0 |
| C07 | 683 | 499.4 | 1,550 | 1,552 | 0.7 / 320.2 | 178.5 | | |
| C09 | 691 | 167.1 | 517 | 3,082 | 7.3 / 74.8 | 85.0 | 1,564 / — / 1,058 | 0 |
| C10 | 695 | 400.2 | 1,546 | 4,110 | 55.1 / 262.6 | 82.5 | 3,649 / — / 2,121 | |
| C12 | 703 | 317.7 | 3,436 | 4,622 | 5.3 / 133.7 | 178.7 | 3,727 / — / 299 | 48 |

C01 per class [R 659]: Read 2,001 jobs, 8.5 µs each; Mutation 2,000 jobs, 56.9 µs; Lifecycle 1,001 jobs,
32.0 µs. Owner wait 342 ns per job. C06: 288 µs per 128 KiB WRITE job [R 679].
The "243 µs per file" of the task text is sample 587 (444.9 ms), a different harness identity that
overlapped the previous Workspace's reclamation; at 659 it is 215 µs per file outside the owner.

### 2.2 The floor [R 726-floor-analysis/floor.md, 723-floor-S01, 724-floor-PIPE]

- One synchronous FUSE request with a negligible daemon: **39.4 µs** = 10.1 client CPU + 11.9 daemon CPU
  + 17.4 nobody running. One cross-process wake: **17.9 µs** of wall (9.2 µs CPU on the woken side,
  8.7 µs nobody running). Wake cost does not grow with idle time up to 80 µs (this refutes my earlier
  hypothesis that replying sooner buys a cheaper wake).
- The registered command span contains about 45 ms of launch (33.6 / 45.1 / 55.1 ms min/median/max).
- The passthrough's receive thread sleeps 0.80 times per request in C01 (4,012 sleeps for 5,001
  requests): after RELEASE's reply the next request is already queued. A second loop removes that saving.

### 2.3 L outside the owner against the passthrough outside its own CPU (arithmetic on [R])

| Cell | L − owner ms | P1E − daemon CPU ms | Difference ms | Explained by |
| --- | ---: | ---: | ---: | --- |
| C01 | 215.1 | 214.5 | +0.6 | nothing to explain |
| C03 | 356.2 | 288.7 | **+67.5** | 3,540 worker hops × 17.9 µs = 63.4 ms [E] |
| C06 | 78.6 | 77.3 | +1.3 | nothing (see §6.5: the 128 KiB copy is not visible) |
| C07 | 178.5 | 121.3 | +57.2 | not explained; 10 % of the cell is 50 ms |
| C09 | 85.0 | 60.5 | +24.5 | 1,058 `LeaveReceiver` hops × 17.9 µs = 18.9 ms [E] |
| C12 | 178.7 | 194.1 | −15.4 | noise |

Single samples taken hours apart; the floor note says differences under about 10 % mean nothing. The
passthrough's daemon CPU contains about 11.9 µs per request of sleep/wake that L pays outside its owner
time, so the comparison is biased against L by up to 60 ms in C01; it still shows no excess.

**Consequence for everything below:** a change that removes locks or allocations is proved by a count
(allocations per request, `steps`, `wakes`, owner jobs, on-CPU ns of `fuser-0`), not by cell
milliseconds. A change that removes a cross-thread wake (17.9 µs) or a synchronous round trip (39.4 µs)
is the kind that shows.

### 2.4 The idle-window rule [E, built on §2.2]

Between a reply and the arrival of the next synchronous request the single turn is free for about
24–30 µs: client wake (≈9), client CPU (≈5–10), daemon wake (≈9), dispatch (≈3). Asynchronous work
(a close, a FORGET, a maintenance turn) placed in that window is free up to the window; beyond it, it is
paid 1:1 by the next request, plus **17.9 µs more** whenever the next request is then answered from a
different thread. This rule is what decides P1 and P8.

## 3. Task 2 — the design questions

### P1. RELEASE: reply first

Contract [S docs]: note 75 "FORGET and RELEASE use only disposal calls and are never fenced … RELEASE
still closes its file"; note 76 drain = `received == 0 && admitted == retained`
(`F/dispatch/admission.rs:121-131`); note 77 "open handles … are independent owners". No text says a
replied RELEASE means the close was applied. `close_held_file` (`O/lifetime/file_owners.rs:194-229`)
changes nothing visible in the namespace; `capture_ready` (`O/lifetime/generation.rs:120-130`) does not
depend on open handles. So replying before the close is admissible as long as the request stays an
admitted unit until the close ends (drain, `stop_service` and the forced path then still account it).

What each variant buys, with the timeline of one C01 file (t = 0 when `fuser-0` reads RELEASE; the
client's next GETATTR is already issued, [R floor.md H2]):

| Variant | Mechanism | Next reply at | Verdict |
| --- | --- | --- | --- |
| Today | close job (32 µs) then reply, then read GETATTR without sleeping | ≈ t+53 µs | baseline; RELEASE costs ≈27 µs on the path [E] |
| R1 reorder only | `reply.ok()` before `close_file().await`, same thread | ≈ t+53 | **0 gain**: nobody waits for RELEASE's reply [I] |
| R2 reply, then `LeaveReceiver` so a pool worker closes | worker starts ≈ t+14; GETATTR took the turn at ≈ t+7; the close queues behind it and `serve_submitted` (COMBINED = 1, `D/overlay/owner.rs:470,488`) serves it **on fuser-0 before GETATTR's own reply** | ≈ t+51 | **0 gain, +1 futex wake per file** |
| R3 reply, then hand the close to the owner thread as a job a submitter never combines | close runs on the owner thread in parallel; the following LOOKUP collides with it (32 µs > window), is served by the owner, then hops to a worker (+17.9) | worse than today unless proposal D lands; with D ≈ −15 to −20 µs per file [E] | only after D, and only if the collided path is proven to cost the overrun alone |
| "Deferred until after the next reply" on the receive thread | needs a guarantee for the case that no next request comes | — | **not possible without a timer or a probe** (same argument as P8) |
| R4 make the close job cheaper | 32.0 µs for a Lifecycle job against 8.5 µs for a Read job [R 659]; at ≈10 µs the close fits the client's own gap and costs nothing on the path, as in the passthrough | ≈ t+31 | **−20 µs per file [E]; SQL research, no dispatch change** |

Honest answer: nothing in the reply ordering helps without a probe. R4 is the lever; R3 is a
possible follow-up worth at most 15–20 µs per file (C01 −15 to −20 ms, about 5 %) with one owner wake
and one collision per file, and it changes C01's `wakes 0 / peak_queued 1` signature. FORGET is the
same case (C03: 1,000 FORGETs at 32.5 µs [R 667]). RELEASEDIR should stay as it is (gated `directory`
call, ENOTCONN semantics, `F/request/directory.rs:76-`).

If R3 is ever built: a pre-effect validation error still replies the error first; a close that fails
after the ok reply ends as `Retained` with no second reply (the kernel ignores RELEASE's status [I]);
notes 75 and 77 gain one sentence each; `mounted_drain.rs` fp21 (:402, asserts one Release opcode :499)
stays valid.

### P2. GETATTR from a revision-checked record

- It cannot remove the request (see §4): the gain is the owner job only, 8.5 µs service + ≈2.5–5 µs
  job frame = **11–13.5 µs per hit** [R 659 + E].
- A namespace-wide revision key misses after every unrelated WRITE or RELEASE, so validity must be per
  serial. Shape that meets the owner demands: one fixed-capacity, direct-mapped table **per daemon**,
  key (engine, namespace, incarnation, mount owner, root, serial) → `ViewStat`; about 32 KiB [E];
  a miss or an evicted slot is the normal visit.
- Fill: (a) the Value of a GETATTR visit; (b) the publishing job, from `Changes.inodes`. Verified [S]:
  create and unlink put the touched parent's full `Inode` there
  (`W/operations/namespace/create.rs:95-98`, `remove.rs:57-60`), and `ViewStat` derives from an `Inode`
  alone (`W/workspace/view.rs:40-59`), so no extra SQL.
- Evict/replace, after COMMIT and before the job's outcome is published: every publication for each
  changed inode (choke point `apply_checked`, `O/namespace/compound.rs:144`), `forget` for that serial,
  and a fence epoch for revoke, attach, close, install and quarantine.
- Inode-row writers that bypass `apply_checked` and must be audited [S grep]:
  `O/lifetime/composition.rs:232,239`, `O/lifetime/orphan.rs:84`, `O/maintenance/orphan.rs:50,130,203`,
  `O/maintenance/reclaim.rs:244`, `O/maintenance/garbage.rs:93`. I did not establish for each whether it
  can change a row a live lookup still names.
- Contract change: `F/coherence/reply_order.rs:1-9` and note 75 say a read-class reply "is computed by an
  owner job that runs after its request was received". It would become "… or from a record that every
  committed change to its inputs replaces before that change's reply". The permission/fence part of
  `native_fence` (`O/lifetime/native_visit.rs:40-94`: Workspace row, mount attached and not revoked,
  kernel reference held) must be covered by the epoch, or the record changes a check — which is forbidden.
- Hits per cell [R opcode counts + E]: C01 1,000 (parent after each create) → −11 to −13.5 ms (3 %);
  C02 2,000 (parent, and the file after WRITE; the WRITE's own publication refreshes the record);
  C03 3,000 → about −40 ms; C04 ≈1,000; C05 ≈1,100; C12 ≈500–600.
- Tests that assert the old counts: `layerfs-daemon/tests/visit_port.rs`
  (`a_lookup_over_resident_objects_is_one_read_job_and_no_reader_grant` :339,
  `a_create_is_one_mutation_job_and_its_reply_attempt_is_no_owner_job` :399), `native_coherence.rs`.
- Verdict: it is a cache, the owner prefers removing a step, and its exactness rests on an audit of
  maintenance writers. **Last in the order, and an owner decision.**

### P3–P7. Fixed-cost trimming (before → after, per GETATTR; [S] counts, [E] times)

| | Change | Locks | Allocs | Clocks | Saving | Risk |
| --- | --- | --- | --- | --- | --- | --- |
| P4 | `try_submit` first, build an `Admission` only on `AdmissionFull` (−1 owner `State`, −3 slots); skip `stopped()` for a turn just taken; one lock for `sql_progress` + `progress` (order aggregates → publish → pass kept); `Pending::poll` takes the value before the notifier lock | 21 → ≈13 | 16 | 4 | 0.3–0.5 µs per job | low |
| P5 | Read `.scope` under the snapshot lock instead of cloning `BranchSnapshot`; build `StorePorts`/`CanonicalClient` only on the first provider demand or serial refill; visits use the bound `Arc<Workspace>` | −2 | −3 | — | 0.4–0.8 µs per request | low–medium; also takes the blocking non-admitted client (`D/store/ports.rs:162-180`) off the receive path |
| P6 | Diagnostics: diff only families a job touched (dirty mask) instead of five passes over 1,904 bytes; no rows allocation when the submitter never reads `work()` (`tests/job_cost.rs` reads it); one chained clock read per statement instead of two (C01: 2 × 61,004 reads ≈ 3–5.5 ms [E]) | — | −1 | per statement 2 → 1 | 0.3–1 µs per job | low; receipt schema (`elapsed_ns`) must keep its meaning |
| Fuse micro | names array instead of `Vec` (`F/request/mutate.rs:221`); `Notification` inside `Task` (`F/dispatch/task.rs:49-58`); move instead of clone for `operation`/`input` (`F/operations/lookup.rs:200`, `mutation.rs:230`) | — | −1 to −3 | — | 0.1–0.3 µs | low |
| P7 | Reply before the owner bookkeeping | — | — | — | ≤1 µs of latency, no CPU | conflicts with "aggregates current before the completion is visible"; **drop** |
| P3 | Straight-line first step (no boxed futures, no `Cell`, no `Admission`) for a job that gets the turn at once | 21 → ≈6 | 16 → ≈2 | 4 → 2 | 3–4.5 µs per request upper bound | **high**: two paths must account, fence and drain identically; **not recommended now** |

Total of P4 + P5 + P6 + micro: about 1–2.5 µs per request, 5–12 µs per created file, 5–12 ms of C01
[E] — below what one sample can show (§2.3). They are worth doing as simplifications (P5 removes a
clone and a latent blocking path; P4 removes a structure from the common path), not as a speed claim.

### P8. Gating the maintenance wake in `Shared::pass`

Facts [S]: `pass` sets `state.maintenance |= …` (`D/overlay/queue.rs:376`), releases the turn (`:382`),
and notifies when `wanted = queued != 0 || state.maintenance` (`:383-387`); `idle` does the same
(`:355-367`). The owner thread's `poll` runs a maintenance turn when `served >= 8 || job.is_none()`
(`:346`); `maintenance_due` is `maintenance && served >= 8` (`:190-192`) and blocks the inline turn
(`D/overlay/owner.rs:355-360`). A maintenance turn handles **one item in one transaction**
(`O/maintenance/ready.rs:147-206`, LIMIT 1). Idle reclamation with no further job is pinned by
`layerfs-daemon/tests/owner.rs:138` (`idle_cleanup_runs_without_an_explicit_reclaim_or_status_job`, 2 s
bound) and `:53`.

What it costs [R 667]: C03 has 6,011 maintenance turns (23.7 µs each, about six per unlinked file) and
3,540 collided requests: owner wait 16.7 µs on average, then a worker hop of 17.9 µs.

**No exact gate exists.** Any rule "do not notify when X" with X computed from daemon state at the end
of a job gives the same answer whether a request follows in 45 µs or never; with no later event, a
skipped notify is reclamation that never starts. A timer or a device poll would distinguish them; both
are outside the rules. The only event-safe refinement is `notify iff queued != 0 || (maintenance &&
owner_is_waiting)`, with the flag set and the condition re-checked under the `State` lock in `wait`
(`D/overlay/queue.rs:390-400`); it saves a futex call only when the owner is already awake.

Options, in my order of preference:

- **D. Keep the gate; make the collision cheap** (§5, item 1). The owner thread replies for the job it
  served. C03 worker-run steps 3,540 → ≈0, −55 to −65 ms [E]. The 8-turn bound and idle reclamation are untouched
  because `queue.rs` is untouched.
- **A. Fewer or cheaper turns per unlinked file** (SQL/reclamation research): 142.5 ms of turns is the
  larger term. Note that batching several items into one longer turn does **not** reduce collision cost
  by itself (a 140 µs turn makes the next request wait about 100 µs); it helps only by lowering the
  cost per item.
- **B. Maintenance on the thread that just replied**, one bounded turn when
  `!busy && queued == 0 && maintenance` (hook after `permit.handoff` returns, `F/request/state.rs:85-89`;
  cursors moved from the locals at `D/overlay/owner.rs:549-552` into shared state), and notify the owner
  only if the hint is still set. Idle is still guaranteed (the last reply runs a turn and notifies for
  the rest). It places a 23.7 µs turn in the ≈24–30 µs window, so a serial client gains perhaps
  40–70 ms in C03 [E]; but with E busy callers it takes up to one turn per request from the receive
  thread that today runs in parallel on the owner thread. It also needs notes 21/75 changed
  ("maintenance on a receive thread"). **Owner decision; not my recommendation.**
- **C. A grace timer on the owner thread**: excluded by rule 6 (time as the fix).

## 4. Task 3 — kernel requests per cell (all kernel statements are [I])

Negotiated [S `F/mount/profile.rs:31-65`, `F/mount/syscalls.rs:63-76`]: ASYNC_READ | BIG_WRITES |
MAX_PAGES, max_write = readahead = 131,072, max_background 1, congestion 1, mount data
`allow_other,default_permissions`, 60 s entry/attr lifetime (`F/request/reply.rs:18`), FOPEN_KEEP_CACHE.
Every flag below is reachable through the vendored fuser without editing it (`V/lib.rs:345-352`
`add_capabilities`; `V/ll/flags/init_flags.rs`, `fopen_flags.rs`).

| Cell | Product opcode counts [R] | Could legitimately not be sent |
| --- | --- | --- |
| C01 (659) | Lookup 1,000, Create 1,000, Write 1,000, Release 1,000, Getattr 1,000, Flush 1 | **none** |
| C02 (663) | as C01 with Getattr 2,001 | none |
| C03 (667) | Lookup 1,000, Create 1,000, Write 1,000, Release 1,000, Unlink 1,000, Forget 1,000, Getattr 3,001, Opendir 1, Readdir 17, Releasedir 1 | none (Opendir/Releasedir 2) |
| C04 (671) | Lookup 1,200, Getattr 1,011, Setattr 1,000, Mkdir 110, Create 1,000, Release 1,000, Opendir 10, Releasedir 10 | Opendir + Releasedir 20 |
| C05 (675) | Lookup 1,200, Getattr 1,111, Setattr 1,000, Mkdir 110, Create 1,000, Release 1,000, Opendir 122, Readdir 222, Releasedir 122, Statfs 1 | Opendir + Releasedir 244 (4 %) |
| C12 (703) | Lookup 957, Getattr 865, Open 229, Release 460, Create 231, Write 235, Read 110, Link 102, Unlink 106, Mkdir 97, … | Open 229 + their Release 229 (13 %); Opendir/Releasedir 10 |
| C07 (683) | Write 1,536, CopyFileRange 1 | 1,024 WRITEs of 64 KiB → ≤512 COPY_FILE_RANGE |
| C10 (695) | Read 512, Write 1,024, CopyFileRange 1 | 1,536 → ≤512 |

Why the five requests of one C01 file are irreducible:

- **GETATTR(parent) after every create/unlink.** The kernel invalidates the directory's attributes when
  it changes it (`fuse_dir_changed`), and under `default_permissions` the next path walk through that
  directory refreshes mode/uid/gid before the permission check. No reply field updates the parent: the
  entry reply describes the child only. The only ways to stop it are dropping `default_permissions`
  (a changed permission check: forbidden) or the kernel not invalidating (not ours). **No reply-side
  way exists.**
- **LOOKUP before CREATE**: an uncached name is looked up first; avoiding it needs a cached negative
  entry, i.e. a lifetime: excluded.
- **RELEASE** of a created file is always sent while the daemon implements open; **FLUSH** is already
  gone (ENOSYS once, `F/request/callbacks.rs:261-263`).
- C02's second GETATTR per file: WRITE invalidates size/mtime when the kernel has no writeback cache
  (forbidden) and the next `stat` asks again.
- C03's rm phase per file: GETATTR(file), UNLINK, FORGET, GETATTR(parent).

Flags and what they would need:

| Flag / reply field | Removes | Product work | Verdict |
| --- | --- | --- | --- |
| COPY_FILE_RANGE implemented (today ENOSYS, `F/request/callbacks.rs:588-602`; after ENOSYS the kernel copies through a 16-page pipe, hence 64 KiB WRITEs) | C07 −512 requests and 64 MiB through the device; C10 −1,024 | a bounded copy job in workspace/overlay (payload research); request path: one callback, one Mutation job per bounded window | **worth doing**; request-path share ≈ −20 to −50 ms per cell [E], owner share larger and not mine to estimate |
| NO_OPEN_SUPPORT (OPEN → ENOSYS) | C12 −458; does not remove a created file's RELEASE | handle-less READ/WRITE/SETATTR with custody released at FORGET; redesign of handle ownership (note 77) | large; owner decision |
| NO_OPENDIR_SUPPORT | C05 −244, C04 −20 | handle-less READDIR, cookies keyed by inode (note 74 redesign) | medium; only C05 matters |
| READDIRPLUS (today ENOSYS, `:609-623`) | LOOKUPs of cold walks (E cells), none in C | lookup custody per returned entry | not for C cells |
| CACHE_SYMLINKS | repeat READLINKs (none in C) | none | a lifetime by another name: owner decision |
| ATOMIC_O_TRUNC | a SETATTR before OPEN with O_TRUNC | truncate inside open | negligible in C |
| FOPEN_CACHE_DIR | repeat listings | invalidation on directory change | none in C |
| PARALLEL_DIROPS | nothing (concurrency only) | — | no count change |
| larger max_write / max_background / readahead; negative entry or longer lifetimes | — | — | excluded by rule 6 |

I do not propose answering `copy_file_range` with a different errno to steer `cp`; that fits a command.

## 5. Task 4 — one receive loop, several callers, several mounts

Threads [S]: per mount `layerfs-mount-N` (joiner), `layerfs-fence-N`, one `fuser-0` receive thread
(`F/session/startup.rs:89,100-102,113-115`; `RECEIVE_LOOPS = RECEIVE_SLOTS = 1`,
`F/session/state.rs:29`, `F/dispatch/types.rs:6`); per daemon K = read_handles + 2 pool workers
(`F/dispatch/types.rs:16-20`), one `layerfs-overlay-owner`, control workers.

- **E processes on one mount.** The kernel queues; `fuser-0` runs each first step inline. Requests that
  need only the owner are served strictly one after another with no hand-off: the queue is in the
  kernel. A step that cannot finish parks; at most 16 are admitted per lane (`HANDOFFS`,
  `F/dispatch/types.rs:4`), and the 17th **blocks `fuser-0` in `admit`** (`F/dispatch/admission.rs:228`).
  That is head-of-line blocking for every small request of that mount, for example behind 16 parked cold
  reads. It is by design ("this handoff-capacity wait is the only wait of a receive loop",
  `F/request/state.rs:56`) and no test measures it.
- **A small request behind a large write.** The kernel splits at 128 KiB and each WRITE is synchronous,
  so the wait is one window job (288 µs [R 679]), not the whole write.
- **A receive thread serves one foreign job before its own reply.** With COMBINED = 1
  (`D/overlay/owner.rs:470,488`) the turn holder serves one queued job after its own and only then
  returns to its future and replies. Under contention a small request's reply can therefore wait for one
  other job (bounded: one).
- **Provider reads.** All five `immutable()`/`base()` sites are preceded by `LeaveReceiver`
  (`F/operations/lookup.rs:230,290`, `mutation.rs:265`, `read.rs:76`, `directory.rs:263`), and a kept
  step also runs as receiving. Three things can still take time on `fuser-0`: the serial refill (one
  synchronous History write per 1,024 creates, `W/workspace/serials.rs:8,25-53`,
  `D/store/ports.rs:205-228`); overlay page I/O inside a bounded job; and the latent blocking demands of
  the non-admitted operation client (`D/store/ports.rs:162-180`, `read_ticket()?.wait()`). **I did not
  prove that no receive-thread path reaches the third**; P5 would remove the question.
- **W mounts.** W receive threads contend for one turn. A loser parks; its job is served by the turn
  holder (one) or the owner thread, and when the owner thread serves it the reply needs a worker hop
  (note 75: "Wakes from a thread that runs no request step, such as the owner thread, queue the step and
  wake a worker"; `F/dispatch/task.rs:60-95`). So under W-mount contention most requests pay one extra
  17.9 µs wake today. Proposal D removes that hop for every case, not only C03.

Tests that exist: `layerfs-fuse/tests/dispatch.rs` pins steps and wakes with synthetic futures
(including `a_kept_step_on_the_receiving_thread_still_leaves_it_before_provider_work` and
`repeated_collisions_complete_every_request_once_with_exact_step_counts`);
`layerfs-daemon/tests/owner.rs:1028` (4 submitters × 300 rounds, 2 namespaces);
`mounted_concurrency.rs` (r6_1: 24 processes; r6_2: Commit against 20 writers; r6_3; r6_6);
`mounted_parking.rs` (fp8; `a_read_of_locally_written_bytes_still_waits_for_a_store_reader` :583; fp34);
`mounted_drain.rs` (fp21, fp29, fp31); `forced_unmount.rs`; `mounted_cycles.rs`.

Not covered: **no daemon mounted test asserts `steps == completed`, `wakes == 0` or `peak_queued` for
any flow**; nothing measures a small request behind 16 parked requests; nothing counts hops with two
mounts.

Cheapest regression test for "one loop got worse" (a count test, no timing): one mounted Workspace,
serial create/write/close/stat of new names (no unlink, no cold read), then from Status assert
`steps == completed`, `wakes == 0`, owner `peak_queued == 1` — the values receipts 659 and 663 already
show. Variants as bounds, not equalities: E = 4 processes on one mount (`wakes == 0`,
`peak_queued <= 1`, since one loop serializes them), W = 2 mounts (`wakes <= completed`,
`peak_queued <= 2`).

## 6. Task 5 — ranking and staged plan

### 6.1 Ranked list

| # | Change | Cause, as a count | Counter after | Saving [E] | Growth | Risk |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | **D: owner thread keeps the continuation it publishes** | C03 3,540 worker hops; C05 804; C12 299; every parked request under W mounts | steps handed to pool workers = `LeaveReceiver` yields only (C03 ≈ 0); needs a new diagnostics counter, `wakes` does not change | 17.9 µs per collided request; C03 −55 to −65 ms (8–9 %), C05 −10 to −14 ms, C12 −5 ms; 0 in C01 | per collision, O(1) | medium |
| 2 | **COPY_FILE_RANGE** | C07 1,024 WRITEs of 64 KiB; C10 512 READs + 1,024 WRITEs | ≤512 CFR, 0 WRITE/READ for the copy | 39.4 µs per removed request + data transfer: C07 ≈ −30 ms, C10 ≈ −50 ms outside the owner; owner share larger | per window | medium (needs a payload copy job) |
| 3 | R4: cheaper close/forget job (SQL research) | 32 µs Lifecycle job per file on the path | Lifecycle µs per job | up to −20 µs per file; C01 −20 ms | per file | theirs |
| 4 | P5 + P4 + P6 + fuse micro | 21 → ≈11 locks, 16 → ≈9 allocations per GETATTR | allocations per request (test-side counting allocator) | 1–2.5 µs per request; 5–12 µs per file; **not visible in one sample** | per request | low |
| 5 | R3: close on the owner thread (after 1) | — | C01 worker-run steps stay 0 with D; `wakes` ≈1,000; `peak_queued` 2 | −15 to −20 µs per file, uncertain | per file | medium–high |
| 6 | NO_OPENDIR_SUPPORT / NO_OPEN_SUPPORT | C05 244, C12 458 requests | opcode counts | 39.4 µs + job per request: C05 ≈ −12 ms, C12 ≈ −25 ms | per open | high (redesign) |
| 7 | P2: GETATTR record | 1,000–3,000 Read jobs per cell | Read jobs −N | 11–13.5 µs per hit; C01 −12 ms, C03 −40 ms | per hit | high (contract + writer audit) |
| 8 | Write-buffer reuse (§6.5) | 512 allocations of 131,088 bytes in C06 | `fuser-0` minor faults | unknown; C06 shows no excess (+1.3 ms, §2.3) | per WRITE | low, **unproven need** |
| — | P3 straight-line step; P7 reply before bookkeeping; P8 options B, C | | | | | not recommended |

Per created file in C01 (215 µs outside the owner, of which 5 × 39.4 = 197 µs is the round-trip floor):
only items 3, 4, 5 and 7 touch it, for at most about 20 + 10 + 13 µs. **C01's distance to A2 (183 ms) is
not closable from the request path**: the passthrough itself needs 285.5 ms for this request stream
[R floor.md §1.3].

### 6.2 Proposal D in detail

- Today [S]: a parked request's job is served by the owner thread (`D/overlay/owner.rs:548-593`); its
  `publish` (`:546`) calls the task's waker on the owner thread; `Task::notify`
  (`F/dispatch/task.rs:60-95`) sees a thread that is not collecting, pushes the step to `ready` and wakes
  a worker; the worker replies.
- Change: the dispatcher exposes a collection scope (the thread-locals at `F/dispatch/task.rs:113-123`
  already implement it for receive threads and workers). The owner thread opens it around the service of
  one queued job and, after `pass` has released the turn, runs **at most the one step it collected**,
  as a receiving thread: `LeaveReceiver` and `NextTurn` (`:302-335`) still hand the step to a worker, so
  no provider read ever runs on the owner thread. A thread that opens no scope behaves as today.
- Invariants kept: publish before reply; one reply per request; drain accounting (the step ends through
  the same `advance`, `:289-297`); "a loop runs at most one step besides the first" becomes "a thread
  that serves a job runs at most the one step that job woke"; the 8-turn bound and idle reclamation
  (`queue.rs` untouched). Contract text of note 75 changes in one sentence.
- Cost: the owner thread spends the reply's `writev` (a few µs) before its next poll.
- Memory: 0 bytes. Storage: 0. Threads: 0 new.
- Files: `F/dispatch/task.rs`, `F/dispatch/mod.rs` or `lib.rs` (export), `D/overlay/owner.rs`, note 75.
- Proving counter: `MountWork` (`F/dispatch/diagnostics.rs:3-17`) has `steps`, `wakes`, `completed` but
  nothing that says a step was queued to the pool. Add one diagnostics field (for example `handed`,
  incremented where `Task::notify` pushes to `ready`, `F/dispatch/task.rs:87-90`, and where a kept step
  is given back, `F/dispatch/queue.rs:215-230`). It is an ordinary counter, not a test hook, and it is
  what S0 pins before D changes it.
- Tests: `a_wake_from_a_thread_that_runs_no_request_step_goes_to_a_pool_worker`
  (`layerfs-fuse/tests/dispatch.rs:575`, asserts `(steps, wakes) == (2, 1)` and that a worker ran the
  second step) stays valid for a thread that opens no scope; new dispatcher test for the scope (the
  scope's thread runs the second step, one step per scope, `LeaveReceiver` leaves it); new mounted
  test: a request that parks behind a held turn completes with `handed == 0`.
- Concurrent callers / several mounts: strictly fewer wakes; replies of parked requests are serialized
  on the owner thread, which already serializes their jobs.
- Risk: a continuation that submits another job runs its inline turn on the owner thread (same code as
  on a receive thread); stop/shutdown ordering between the owner thread and `stop_service`
  (`F/dispatch/admission.rs:98-117`) must be checked — I did not trace it.

### 6.3 Staged plan (each stage green, own commit, own proving counter)

Fuse first, then daemon. Every commit reports production LOC before/after per the repository rule.

| Stage | Crate | Content | Proving counter | Tests |
| --- | --- | --- | --- | --- |
| S0 | fuse + tests | Add the `handed` diagnostics counter; pin today: mounted serial flow `steps == completed`, `wakes == 0`, `handed == 0`, `peak_queued == 1`, owner jobs per opcode; E = 4 and W = 2 bounds; a request parked behind a held turn has `handed == 1` | `handed` exists and equals `wakes` minus kept steps | new `layerfs-daemon/tests/` file; ≤120 s, bounded waits |
| S1 | fuse | Collection scope API in the dispatcher (no caller yet) | dispatcher unit counts | `layerfs-fuse/tests/dispatch.rs` additions; `:575` unchanged |
| S2 | daemon | Owner thread uses the scope (proposal D) | `handed` in the S0 parked test 1 → 0; C03 `handed` ≈3,540 → ≈0 (`wakes` unchanged) | S0 test updated in the same commit |
| S3 | fuse | Micro: names array, `Notification` in `Task`, no clone of `operation`/`input` | allocations per request −1 to −3 (counting allocator in the test binary, not in product source) | new alloc-count test with synthetic services |
| S4 | daemon | P5 lazy request services | allocations per request −3 | `visit_port.rs`, `fenced_port.rs` unchanged counts |
| S5 | daemon | P4 owner/admission lock trimming | owner wait ns per job [R] 342 → lower; no lock counter exists | `admission_future.rs`, `completion_future.rs`, `owner.rs:1028` |
| S6 | daemon + overlay | P6 diagnostics (dirty-family mask, one clock read per statement) | statements unchanged; `job_cost.rs` values identical | `job_cost.rs` |
| S7 | all | COPY_FILE_RANGE (with the payload research) | C07 Write 1,536 → 512, C10 Read 512 → 0, Write 1,024 → 0 | new mounted copy test, verifier |
| S8 | daemon | R3 only if S2 measured as predicted and R4 did not already remove the term | C01 Lifecycle jobs unchanged, `peak_queued` 2 | `mounted_drain.rs` fp21 |
| S9 | all | P2 only on owner decision | Read jobs −N | `visit_port.rs:339,399`, `native_coherence.rs` |

One sample at the tip after the batch, per the R7 small-steps rule; S2 and S7 are the only stages whose
effect a cell sample can be expected to show.

### 6.4 Memory (current [S] and after)

- Per mount today: three threads; one receive buffer of 16 MiB + 4,096 virtual (`V/read_buf.rs:8,25`;
  resident only as far as touched, at most about one max request [I]); one lane of 16 hand-off slots.
- Per in-flight request today: boxed future, `Task`, `Notification`, request services (four
  allocations), per job seven allocations; a WRITE holds one `Arc<[u8]>` of its length (≤128 KiB), so a
  lane holds at most 16 × 128 KiB = 2 MiB of write data.
- After: D 0 bytes; P4/P5/P6 fewer allocations; P2 ≈32 KiB per daemon; write-buffer reuse ≤128 KiB per
  receive loop (W mounts → W × 128 KiB) if it is ever built. None rises with file count, size, directory
  size or edit count. Allocated storage is unchanged by every item.

### 6.5 The 128 KiB write copy — downgraded

The daemon target is `aarch64-unknown-linux-musl` and product source sets no global allocator [S grep],
so each full-window WRITE allocates 131,088 bytes from musl's malloc; if that malloc maps such a size
directly, each WRITE pays an mmap, about 33 page faults and a munmap [I, not verified — it depends on
which malloc the zig-linked musl uses]. But C06 shows no excess outside the owner (+1.3 ms for 512
WRITEs against the passthrough, §2.3), so the term is at most a few ms per cell unless it hides inside
owner service time (the job touches the same pages). **Measure before proposing**: `minflt` of `fuser-0`
across C06; a count near 512 × 33 ≈ 17,000 confirms it, a count near zero closes it. If confirmed, the
generic fix is a retained buffer per receive loop reused when `Arc::get_mut` succeeds, falling back to
a fresh allocation otherwise.

## 7. What I could not verify

- Every kernel statement (`fuse_dir_changed`, `default_permissions` refresh, LOOKUP before CREATE,
  asynchronous RELEASE/FORGET, the splice fallback for `copy_file_range`, what each INIT flag removes).
  I recalled them; I did not read kernel 6.12 source.
- Every µs estimate marked [E], in particular the timelines of P1 and the idle window of §2.4. They are
  built from the passthrough and pipe probes [R 723, 724], not from a per-thread observation of the
  product daemon; none exists in receipts 652–703.
- That the 67 ms of C03 in §2.3 is the worker hop: it matches 3,540 × 17.9 µs, but it is two single
  samples taken hours apart.
- musl's malloc behaviour for 131,088 bytes, and whether `fuser-0` takes page faults per WRITE.
- The SQL text of `FENCE_LOOKUP` / `FENCE_FILE` / `FENCE_HANDLE` (I read the Rust caller only).
- For P2: whether each of the eight inode-row writers outside `apply_checked` can change a row a live
  lookup names; whether any read path writes rows.
- That no receive-thread path reaches the blocking demands of the non-admitted client
  (`D/store/ports.rs:162-180`).
- For D: shutdown ordering between the owner thread running a step and dispatcher `stop_service`; and
  that C03's 3,540 `wakes` are all owner-thread notifications (inferred from `steps − completed = 3,539`
  and 6,011 maintenance turns; the receipt has no per-thread split).
- The longest single owner job and the longest single maintenance turn (receipts give averages only),
  hence any worst-case latency number beyond "one job".
- Opcode-level `steps`/`wakes` for C07, C08, C11 (not decoded).
