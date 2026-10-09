# C01: where the 44 µs per request outside the owner goes

> **Status:** Research; informative and not a product contract.

Read-only analysis by a research subagent at commit `fdc24ef3f`, 2026-10-09. Kernel statements are recollection, not read locally; nothing was built or run.

Everything below is read from source at `fdc24ef3f` unless marked **[inference]** or **[estimate]**; kernel statements are recollection of v6.12 `fs/fuse` and `kernel/sched/fair.c`, not read locally. Nothing was built or run.

Path prefixes: `F/` = `core/crates/layerfs-fuse/src/`, `D/` = `core/crates/layerfs-daemon/src/`, `V/` = `core/vendor/fuser-0.18.0/src/` (the patched fuser copy, `core/Cargo.toml:52-53`), `X/` = `codex/phase7-experiment-305:core/experiment/real-tree/src/`, `LEDGER` = `core/docs/issues/307/checks/r7-optimization-20261009/LEDGER.md`.

Assumed kernel: 6.12.76-linuxkit with 8 vCPUs, taken from the experiment branch (`core/docs/issues/305/A2-PIN-REPORT.md:159`). The R7 receipt's `environment.txt` holds only environment variables, no `uname`.

## 0. Headline findings

1. **A2 is not single-threaded and does more round trips than we do.** It runs two receive loops (`X/main.rs:765`) and answers FLUSH with success (`X/main.rs:677-685`), so it serves 7 requests per file against our 5. Its 183.4 ms is 26 µs per request (`LEDGER:1524-1528, 1943`).
2. **On a serial client no other daemon thread runs.** Receipt 527 `mount_work` shows `receive_units 5001, completed 5000, steps 5000, wakes 0`, and `owner_work` shows `peak_queued 1, maintenance_jobs 8`. Every request finishes in its first step on `fuser-0`, with 0 futex syscalls on the path.
3. **User-space fixed cost outside the owner job is about 3–5 µs per request [estimate from counts].** That is 20–24 lock pairs, 12–19 allocations, 4–5 clock reads, about 50 atomics and about 14 KB of diagnostics copies. It is not 44 µs.
4. **The rest is wake-up latency [inference].** Each synchronous request wakes two idle vCPUs. The branch's probe measured a pipe round trip at 42 µs across CPUs and 2.5 µs on one CPU (`core/docs/issues/305/A2-WALK-REPORT.md:135-139`), about 21 µs per wake. Four synchronous requests per file at that rate, plus about 15 µs of bash, is already about 183 µs with zero daemon time.
5. **So C01 cannot beat 183 ms by trimming alone if a wake costs 21 µs here.** A2's own number implies about 10–12 µs per wake in its run, which was a different harness and date. No arm-P C01 sample exists in the R7 harness; take one before spending on proposals 3 and later.
6. **Two structural levers exist inside the rules.** RELEASE's close (35 µs service) runs in front of the next file's GETATTR while the client sleeps. And every reply is followed by about 20 µs in which the daemon is idle while the client's vCPU wakes; work moved there is free.

## 1. Task 1 — user-space trace on `fuser-0`

### Skeleton common to all five requests

**fuser loop**
- `read(fd, buf)` at `V/channel.rs:31`, called from `V/session.rs:549`.
- `self.ch.sender()` Arc clone and in-place parse (`V/session.rs:550`, `V/request.rs:43-53`); `operation()` is parsed twice (`V/session.rs:553`, `V/request.rs:71`).
- `self.reply()` clones the channel Arc again (`V/request.rs:555`). No lock, no allocation.

**Admission** (`F/request/state.rs:58-83`)
- Opcode counter, 1 atomic (`F/request/accounting.rs:111`).
- Lock 1: dispatch `Shared.state` in `receive` (`F/dispatch/admission.rs:51`, lock at `F/dispatch/queue.rs:86`), plus a `MountQueue` clone (2 Arcs).
- Lock 2: the same mutex in `admit` (`admission.rs:187`); scans 16 slots (`:208-218`).

**Hand-off** (`F/request/state.rs:85-89`, `F/dispatch/admission.rs:253-299`)
- Allocation: the boxed request future (`state.rs:159`, `F/request/mutate.rs:155`, or `F/request/callbacks.rs:189`).
- Allocations: `Arc<Task>` and `Arc<Notification>` (`F/dispatch/task.rs:49,57`).
- Lock 3: `Shared.state` (`admission.rs:263`).
- `advance` (`task.rs:203`): lock 4 is `Task.future` (`:205-209`); waker clone (`:215`); thread-local sets; `catch_unwind`.
- **One poll (`task.rs:220-225`), which runs the whole request including the reply.** One waker exists per request and is never woken; it is cloned three times (`task.rs:215`, `D/overlay/admission.rs:225`, `D/service/completion.rs:183`).

**Per-request services** (`D/service/filesystem_port.rs:40-42`, `D/store/operation.rs:26-36`)
- `snapshot` mutex and a `BranchSnapshot` clone (`operation.rs:38-41`).
- Allocations: `Arc<StorePorts>` (`D/store/ports.rs:61`), `Arc<CanonicalClient>` (`:89`), `Arc<FilesystemPort>`.
- `Workspace::scoped` takes the base `RwLock` read (`layerfs-workspace/src/workspace/state.rs:90-107`).

**One owner job** (`filesystem_port.rs:75-115`)
- Allocations: two boxed futures (`:76`, `:107`).
- `submit_when_available` (`D/overlay/admission.rs:122-169`): lock on owner `State` (`:146`); lock on `Notifications.slots` in `reserve` (`:52`).
- `Admission::poll` (`:222-245`): slots lock in `arm` (`:65`), then `try_submit` (`D/overlay/owner.rs:251-379`).
- `try_submit`: owner `State` lock (`:276`); allocations `Arc<Cell>` (`completion.rs:77`) and `Box<Job>` with `Instant::now()` (`owner.rs:330-335`); takes the turn inline (`:355-365`).
- `serve_submitted` (`:476`): connection mutex (`D/overlay/queue.rs:325`).
- `serve` (`:502-547`):
  - `stopped()` re-locks `State` (`queue.rs:435`).
  - `Turn::begin` copies `DatabaseWork` (14 `StatementWork`, about 2 KB, `layerfs-overlay/src/diagnostics/metrics.rs:31-66`).
  - `elapsed`, `Instant::now()` (`:531-532`).
  - **`command.perform` (`:541`) is the measured "service".**
  - `turn.settle` makes three more diagnostics reads and diffs (`:435-446`).
  - `sql_progress` locks `State` (`queue.rs:268`); `elapsed` (`:544`); `progress` locks `State` (`queue.rs:455`).
  - Allocation `Box<Outcome>`, which carries `JobWork`, over 2 KB (`:546`).
  - `publish`, then `Cell::settle` locks `notifier` (`completion.rs:95`); there is no waiter and no waker.
- `pass` locks `State` (`queue.rs:373`). No notify unless `queued != 0 || maintenance` (`:383-387`).
- `release_slot` locks slots (`admission.rs:75`).
- `Pending::poll` locks `notifier` (`completion.rs:184`).
- Allocation: `ServiceReply::new` boxes the completion (`F/ports.rs:148`).

**Reply.** One `writev` (`V/channel.rs:87`) with 2 iovecs: a 16-byte stack header plus a stack struct (`V/ll/reply.rs:37-49`). No heap copy; errors and empty replies are the header only.

**After the reply, before the next `read()`**
- The receipt drops, freeing the completion box, `Cell` and `Outcome`.
- `Credit::drop` locks owner `State` (`D/overlay/credits.rs:53`); `admission.notify()` returns on `reserved == 0` (`admission.rs:86`).
- The future drops (ports, client, facts).
- Lock 5 on dispatch: `Shared.state` to free the slot (`task.rs:289-297`); `announce` is gated by `observers != 0` (`queue.rs:130-134`).
- `Task` and `Notification` are freed.

### Per-request differences

- **GETATTR** (`F/request/callbacks.rs:83-98`, `F/operations/lookup.rs:191-216`). Adds `Arc<VisitFacts>` (`lookup.rs:192`), the base `RwLock` read in `native_read_visit` (`layerfs-workspace/src/operations/native_visit.rs:171`) and the boxed visit (`filesystem_port.rs:385`). Reply at `F/request/reply.rs:124`.
- **LOOKUP, negative** (`callbacks.rs:48-79`). Adds the `PathName` Vec copy (`:67`) and its clone per visit (`lookup.rs:200`). The receipt drops before the reply (`lookup.rs:217`), so the credit lock is before the reply. Reply is `ENOENT` as an error (`reply.rs:72`, `F/attributes.rs:96`), so nothing is cached.
- **CREATE** (`callbacks.rs:552-573`, `mutate.rs:199-240`).
  - Adds `Vec::with_capacity(N)` (`mutate.rs:221`), the `PathName` copy (`:223`), the boxed builder (`:238`) and `SystemTime::now()` (`:156`).
  - Adds `reserve_serial`, a `serials` mutex (`layerfs-workspace/src/workspace/serials.rs:27`), and an `input.clone()` that copies the name again (`F/operations/mutation.rs:230`).
  - Adds the ticket `issue` lock inside the job (`layerfs-overlay/src/lifetime/tickets.rs:50`).
  - Reply at `mutate.rs:118-124`. After it: `reply_attempted` takes the tickets lock (`tickets.rs:100`) and allocates a boxed ready future (`filesystem_port.rs:498`).
- **WRITE** (`callbacks.rs:511-551`). The one copy of the data goes into `Arc<[u8]>` (`F/operations/write.rs:23`, `layerfs-workspace/src/operations/types.rs:10-19`); the later clone is an Arc clone. Boxed builder (`callbacks.rs:549`). Otherwise as CREATE without the serial and the name.
- **RELEASE** (`callbacks.rs:168-219`). **The close job runs first (`:194`) and the reply comes after it (`:200`); everything is before the reply.** No visit box.

### Answers

- **(a) Futex wakes that wake a thread per request: 0.** Futex syscalls of any kind: 0. Every condvar notify on the path is gated: `queue.rs:130-140` (dispatch), `D/overlay/queue.rs:383-387` and `owner.rs:371-376` (owner), `completion.rs:88-102`, `admission.rs:86`. The gates matter because Rust's futex `Condvar::notify_*` makes the syscall even with no waiter [std, recollection]. Only `fuser-0` runs. Asleep throughout:
  - 4 dispatch workers in `wait_runnable` (`F/dispatch/workers.rs:263-283`, count from `F/dispatch/types.rs:16-19`).
  - `layerfs-overlay-owner` in `Shared::wait` (`D/overlay/queue.rs:390-399`).
  - `layerfs-mount-N` joining the loop and `layerfs-fence-N` in `wait_for_change(.., Duration::MAX)` (`F/session/startup.rs:100-119, 171-175`).
  - Control connection workers (`D/application/connection.rs:47`).
  - No timer thread exists.
- **(b) Per-request or per-file wake of another thread: none in C01, with one exception.** `Shared::pass` calls `wake.notify_all()` at the end of every job while `state.maintenance` is set, not only when maintenance is due (`D/overlay/queue.rs:376-387`). The hint is set by `enqueue`, `wake_generation`, `schedule_shrink` and the orphan paths (`layerfs-overlay/src/maintenance/ready.rs:84,234`, `garbage.rs:238`, `orphan.rs:219`, `lifetime/orphan.rs:131`, `lifetime/close.rs:142`).
  - In C01 that is about 8 maintenance turns per 1000 files (receipt `maintenance_jobs 8`).
  - In unlink cells it is every job: the owner thread wakes, takes `busy`, and the next request queues behind it and returns through a worker. That matches C03's 62 ms of owner wait (`LEDGER`, baseline table).
  - No per-request wake comes from RELEASE's close, the reader pool, metrics or logging; `debug!` is a level check.
- **(c) Receive buffer.** `vec![0; 16 MiB + 4096]` (`V/read_buf.rs:8,25`, `V/session.rs:57`), allocated once per loop (`V/session.rs:543`) and reused. It is zero-filled lazily, so only touched pages become resident [inference]. The request is parsed in place; the only copies out are the name and the write data listed above.
- **(d) Reply.** One `writev` of header plus struct from the stack, with no intermediate buffer. `ReplyData` for READ passes the slice through.
- **(e) Counts per request**, outside `command.perform`, counted from source, ±2:

| | GETATTR | LOOKUP | CREATE | WRITE | RELEASE |
| --- | ---: | ---: | ---: | ---: | ---: |
| Syscalls (`read` + `writev`) | 2 | 2 | 2 | 2 | 2 |
| Futex syscalls, thread wakes | 0 | 0 | 0 | 0 | 0 |
| Mutex/RwLock pairs: dispatch 5, services 2, owner `State` 7, slots 3, connection 1, notifier 2, other | 21 | 21 | 24 | 23 | 20 |
| Heap allocations (each freed) | 14 | 16 | 19 | 17 | 12 |
| `Instant::now`/`elapsed` + `SystemTime::now` | 4 | 4 | 4+1 | 4+1 | 4 |
| Atomic RMW (mostly Arc counts) [estimate] | ~45 | ~48 | ~55 | ~52 | ~42 |
| Polls / wakers created / wakes | 1/1/0 | 1/1/0 | 1/1/0 | 1/1/0 | 1/1/0 |
| Copies of request bytes | 0 | name ×2 | name ×2 | data ×1 | 0 |
| `DatabaseWork` (~2 KB) copies or diffs | ~6 | ~6 | ~6 | ~6 | ~6 |

Per file that is about 109 lock pairs, 78 allocations, 22 clock reads and 10 syscalls. On aarch64 Linux `clock_gettime` is served by the vDSO from the virtual counter, about 30 ns each [inference]. Inside the job, each SQL statement adds two more clock reads (`metrics.rs:91,112`); at 101 statements per file (`LEDGER`, step 9) that is about 200 per file, counted as service.

## 2. Task 2 — A2

**Entry point.** `fuser::mount(fs, …)` (`X/main.rs:798`), which is `Session::new` then `Session::run` (`V/lib.rs:1054-1060`). `n_threads = Some(2)`, `clone_fd = false` (`X/main.rs:765-766`), so two `fuser-N` threads block in `read()` on one fd. The kernel wakes the longest waiter, so a serial client alternates threads (`LEDGER`, step 9 cause).

**Per-request work** (profile `A2`: `cached` true, every other switch false, `X/main.rs:735-743`).
- Each callback takes one `Mutex<State>` (`X/costs.rs:49-58`), bumps a `HashMap` counter (`X/main.rs:76-97`), makes its ext4 call and replies inline.
- GETATTR: one `lstat` (`:216-225`).
- LOOKUP: path join plus `lstat` (`:191-202`).
- CREATE: `open(O_CREAT)`, `fstat`, two map inserts (`:431-452`).
- WRITE: `pwrite` (`:402-416`).
- FLUSH: `r.ok()` (`:684`).
- RELEASE: one map removal (`:309-313`).
- That is 1–2 mutex pairs, 0–3 small allocations (PathBuf), no clock reads (costs are off unless `EXPERIMENT_COSTS`, `X/main.rs:744-749`), no task, no queue.

**What the kernel is told.**

| Item | Ours | A2 |
| --- | --- | --- |
| INIT flags | ASYNC_READ, BIG_WRITES, MAX_PAGES if offered (`F/mount/profile.rs:33-40`) | Same, fuser default (`V/lib.rs:110,122-128`); nothing added for plain A2 (`X/main.rs:145-166`) |
| max_write / max_readahead | 131072 / 131072 (`profile.rs:42-44`) | Same (`X/main.rs:167-170`) |
| max_background / congestion | 1 / 1 (`profile.rs:45-46`) | Same (`X/main.rs:171-174`) |
| Mount options | `allow_other,default_permissions,max_read=131072`, nosuid, nodev, noatime (`F/mount/syscalls.rs:63-76`) | Same in effect (`X/main.rs:764-777`) |
| Entry / attr lifetime | 60 s (`F/request/reply.rs:18`) | 60 s (`X/main.rs:133-135`) |
| Negative lookup | `ENOENT` error, not cached | Same (`X/main.rs:199-201`); only profile A2SL caches |
| Open reply | FOPEN_KEEP_CACHE (`reply.rs:158-164`) | Same (`X/main.rs:136-142`) |
| FLUSH | `ENOSYS` once, then none (`F/request/callbacks.rs:261-263`) | Success every time: 2 per file in C01 |
| FSYNC, xattr | `ENOSYS`, sticky (`callbacks.rs:264-300`) | Success / `EOPNOTSUPP` every time (`X/main.rs:686-729`) |

Not negotiated by either: PARALLEL_DIROPS, AUTO_INVAL_DATA, DO_READDIRPLUS, NO_OPEN_SUPPORT, NO_OPENDIR_SUPPORT, CACHE_SYMLINKS, HANDLE_KILLPRIV, ATOMIC_O_TRUNC, SPLICE_*, FOPEN_NOFLUSH, FOPEN_DIRECT_IO.

**Sequences.**
- Ours: GETATTR(parent), LOOKUP, CREATE, WRITE, RELEASE. Receipt 527 opcode counts are 1000 each plus 1 FLUSH.
- A2: GETATTR, LOOKUP, CREATE, FLUSH, WRITE, FLUSH, RELEASE (`LEDGER:1524-1526`).
- Neither avoids GETATTR(parent). CREATE invalidates the directory's attributes and `default_permissions` refreshes them at the next permission check [kernel, recollection]. Only the A2WN and A2S profiles avoid it, by dropping `default_permissions` (`X/main.rs:778-787`).
- Neither avoids LOOKUP: every name is new, and 6.12 has no atomic open.

**So the kernel is told the same thing, and we already receive fewer requests.** Flags that would legitimately remove requests in other cells (none helps C01):
- NO_OPENDIR_SUPPORT: removes OPENDIR and RELEASEDIR per listing (find, rm -r, tar, git).
- DO_READDIRPLUS with AUTO: removes one LOOKUP per entry on a cold walk (find, tar, git status, cp -r).
- FOPEN_CACHE_DIR: repeat listings in one mount send no READDIR.
- NO_OPEN_SUPPORT: removes OPEN and RELEASE per open of an existing file (cat, cp source, git, tar). It needs handle-less reads and writes, with open custody ending at FORGET. It does not remove a created file's RELEASE [kernel, recollection].
- ATOMIC_O_TRUNC: removes the SETATTR(size 0) after a truncating open (overwrites, checkout).
- max_write and max_pages at 1 MiB: 8× fewer READ and WRITE (dd, cp, cat, tar).
- COPY_FILE_RANGE implemented: one request per chunk in place of READ+WRITE pairs (cp).
- CACHE_SYMLINKS: repeat READLINK.
- max_background above 1: no fewer requests, but readahead READs queue while the daemon works, so neither side sleeps between them.
- Negative entries with the same 60 s lifetime (repeated probes of absent names) is a new use of the lifetime; I read it as borderline under the no-longer-lifetimes rule.

## 3. Task 3 — kernel wake-up path

All of this section is recollection of v6.12 or inference.

**Path.**
- Sending: `fuse_simple_request` → `__fuse_request_send` → `fuse_dev_queue_req` puts the request on `fiq->pending` and calls plain `wake_up(&fiq->waitq)`, not a sync wake. The caller then sleeps in `request_wait_answer`.
- Replying: the daemon's `writev` reaches `fuse_request_end`, which calls plain `wake_up(&req->waitq)`.
- Placement: 6.12 FUSE does not use `WF_CURRENT_CPU`. In `select_task_rq_fair` a 1:1 ping-pong is not wide, and `wake_affine_idle` returns the wakee's previous CPU when it is idle. Even a sync wake ends on the idle previous CPU through `select_idle_sibling`, which is why the pipe probe also costs 42 µs.
- Result: client and daemon sit on two different vCPUs, both idle between turns. An arm64 guest idles in WFI with no polling state, so every wake is an IPI to a vCPU whose host thread has blocked. That is about 20 µs per wake and two per synchronous request.

**Reply after 5 µs against reply after 40 µs.**
- Placement does not change; the client returns to its own idle CPU either way.
- What changes is how long the client's vCPU has been idle, and how much of our work is on the critical path. Whether wake latency grows with idle time is not known here. The zero-work pipe at 42 µs argues against a fast regime for short idle; A2O at 29.5 µs and E03 at 9–17 µs per request (`A2-PIN-REPORT.md:127-130`) show the cost is state-dependent.
- The daemon running on for tens of µs after `writev` does not delay the client unless they share a CPU. It does delay the next request if the client is back first.

**A second daemon thread.** Each thread sleeps two request periods and runs colder. But a surplus wake (from the asynchronous RELEASE) can deliver a thread that finds the next request already queued.

**RELEASE in front of the next request.** Yes, by construction with one loop:
1. The client's `close` queues RELEASE in the background and wakes `fuser-0`; the client does not sleep.
2. About 5–15 µs later it queues GETATTR. `wake_up` finds no waiter, and the client sleeps.
3. `fuser-0` arrives, reads RELEASE first (FIFO), runs the whole close (35 µs service plus overhead, reply last), and only then reads GETATTR.

**Hypotheses and the observable each predicts**, deltas over the measured command:

| # | Hypothesis | Prediction |
| --- | --- | --- |
| H1 | Outside time is wake latency, not daemon CPU | `fuser-0` on-CPU ns (schedstat field 1) ≈ 193 ms + 5000 × 6–9 µs ≈ 225–240 ms; run-queue wait (field 2) ÷ timeslices (field 3) ≈ 15–25 µs. If on-CPU is 300 ms or more, the fixed cost is about 20 µs per request and trimming is the first lever. |
| H2 | RELEASE precedes GETATTR on the critical path | `fuser-0` voluntary switches ≈ 4000, not 5000: one `read()` per file finds a request pending |
| H3 | No hidden per-request wake | `layerfs-overlay-owner` voluntary switches ≈ 8–20; workers, mount, fence threads 0; nonvoluntary ≈ 0 everywhere |
| H4 | A2's 183 ms comes from a cheaper-wake regime | One arm-P C01 sample in the R7 harness. Flat 21 µs wakes predict about 300 ms; about 183 ms means P's wakes cost about 10 µs, and its two threads' wait per timeslice and switch split show where |
| H5 | Wake latency rises with idle time | Harness probe in the style of `roundtrip_diag.py`: pipe ping-pong where the responder works d = 0, 5, 10, 20, 40, 80 µs; plot round trip minus d. Flat refutes it. |
| H6 | Allocator or page-fault churn inflates the thread | `minflt` (`/proc/1/task/<tid>/stat` field 10) per request ≈ 0; several per file means trim-and-refault. VmHWM is bimodal across samples (`LEDGER`, step 8 notes). |
| H7 | Client and daemon share no CPU | Last CPU (`stat` field 39) differs; nonvoluntary switches of `fuser-0` ≈ 0 |

**Where to add the observation, with no product change.**
- The daemon is PID 1 of the container, and `runtime::snapshot` already runs `cat /proc/1/status` as root (`core/benchmark/r7-runtime/src/runtime.rs:171-215`, shell body at `:174`). Add per-thread `comm`, `schedstat`, the two `ctxt_switches` lines of `status`, `stat`, and `io` (`syscr`/`syscw` give read and writev counts without strace). Add `/sys/fs/cgroup/cpu.stat` and `uname -r`.
- Parse in `snapshot_observations` (`core/benchmark/fs-bench-pro/r7/runner.py:603-627`).
- Send one `observe` immediately before and one after the measured command; `observe` already exists (`core/benchmark/r7-runtime/src/session.rs:164`, `runner.py:1080,1161`). Both are outside the timed span.
- If the command runs in the same cgroup, client CPU = cgroup `usage_usec` minus daemon thread CPU, and client wake latency = wall − client CPU − `fuser-0` (CPU + wait). I did not check whether it shares the cgroup.
- `strace` is not referenced anywhere under `core/benchmark` and its presence in the pinned image is unverified; do not plan on it. Per-task `schedstat` needs `CONFIG_SCHED_INFO`; if it is absent, the context-switch counts and `stat` utime/stime remain.

## 4. Task 4 — proposals, ranked by expected saving in C01

All savings are estimates.

**P0. Measure first** (harness only): the section 3 observables on arm L, plus one arm-P C01 sample. This decides whether the target is reachable and whether P1–P2 or P3–P7 come first.

**P1. Take RELEASE's close off the next request's path. About −20 to −40 µs per file (−5 to −9 % of the command).**
- Change: reply to RELEASE first. Then probe once with `poll(fd, POLLIN, 0)`. If a request is pending, keep the close as this thread's kept step and return to the loop; the next request is served first and the close runs after its reply, in the window while the client wakes. If nothing is pending, run the close now, as today.
- Before → after for the following GETATTR: about 40 µs of close ahead of it → 0 to 20 µs of overrun after it. One extra syscall per RELEASE.
- Invariant: the RELEASE stays an admitted dispatcher slot until its close job publishes, so drain and forced unmount count it as they count any kept step (`F/dispatch/task.rs:113-155`, `F/dispatch/admission.rs:98-131`). One submission, no retry. A request on the same file served before the close sees one more open owner, which is the conservative answer.
- Files: `F/request/callbacks.rs` (release), `F/dispatch/task.rs`, `F/dispatch/admission.rs:286-297`, `F/session/startup.rs` (the fd is ours from `open_device`, `:54`). No fuser edit.
- Risk: medium. It needs a check of notes 75/76 for "RELEASE replied means close applied", which I did not read. The single probe is not a loop, but whether it falls under the no-polling rule is a judgement for the lead.
- Without the probe it would need a hook before the loop blocks. That is a fuser change outside the authorized lifecycle patch; I do not propose it.

**P2. Answer GETATTR from a revision-checked record. −1 owner job per file (5 → 4), about −15 µs per file; also C02's GETATTRs.**
- Change: the publishing job returns the changed parent's stat as well as the child's. Keep (serial → stat, revision) in engine memory beside `ReplyTickets`. A GETATTR whose record's revision equals the namespace's current revision is answered with no job.
- Before → after for GETATTR: 21 locks → ~6, 14 allocations → ~3, 4 clock reads → 0, service 11 µs → under 1 µs.
- Invariant: today a read reply is computed by a job that runs after its request was received (`F/coherence/reply_order.rs:6-9`). It becomes "computed at revision R, and R is still current at receipt", the same visibility. That is a contract change in note 75.
- Files: `layerfs-overlay` (publication result, revision mirror), `layerfs-workspace` (outcome), `F/operations/lookup.rs`, `D/service/filesystem_port.rs`.
- Risk: medium. This is a daemon-side record, not a longer kernel lifetime.

**P3. Straight-line path for a first step that gets its turn. About −2 to −4 µs per request (−10 to −20 µs per file).**
- Change: decide before any effect. If the connection is free, nothing is queued, maintenance is not due and the fence is open, call the owner job directly and take the outcome by value. That removes the boxed futures, `Task`, waker, `Cell`, `Job` box, `Outcome` box and notification slot.
- Before → after per request: locks 21 → ~7, allocations 12–19 → 2–4, clock reads 4 → 2, diagnostics copies 6 → 1, atomics ~50 → ~15, polls 1 → 0.
- Minimum registration that keeps accounting exact:
  - the lane's `received → admitted → completed` counts and `receiving` (`F/dispatch/queue.rs:62-75`, `task.rs:180-198`), under one lock each way;
  - the owner's `busy` turn, `outstanding` and `credited_bytes` (`D/overlay/owner.rs:344-350`).
- Fallback: any other outcome (turn held, admission full, base facts needed, failure to retain) builds the existing `Task` with the same custody and continues on today's path.
- Files: `F/dispatch/{admission,task,queue}.rs`, `F/request/{state,mutate,reply}.rs`, `F/ports.rs` (one synchronous "serve now" method), `D/service/filesystem_port.rs`, `D/overlay/owner.rs`.
- Risk: high. Two paths must account identically; retained failures need the upgrade to a `Task`.

**P4. Fewer owner-lock rounds on today's path** (a subset of P3 that keeps `Task`). About −0.5 to −1 µs per request.
- Call `try_submit` first and reserve a notification slot only on `AdmissionFull` (removes the 3 slot locks and the pre-check at `D/overlay/admission.rs:146`).
- Skip `stopped()` on a turn just taken under the lock (`D/overlay/owner.rs:503` against `:280`).
- Merge `sql_progress`, `progress` and `pass` into one end-of-turn lock (`D/overlay/queue.rs:261-279, 372-389, 454-472`).
- Owner locks 7 → 3, slot locks 3 → 0. Risk: low. Do this first.

**P5. Build request services lazily.** About −0.3 to −0.5 µs per request.
- `StorePorts`, `CanonicalClient` and the scoped `Workspace` are built on every kernel request (`D/store/operation.rs:26-36`) but used only on a base read or a serial refill.
- 3 allocations, 1 mutex, 1 rwlock → 0 on the common path. Demand-failure custody is unchanged when a demand is made. Risk: low.

**P6. Diagnostics per turn, not per job.** About −1 µs per job outside service, and −5 to −8 µs per file inside it.
- Keep the connection's cumulative counters. Compute per-job deltas only for jobs whose submitter reads `Completion::work()`; the native path reads `result()` only (`D/service/filesystem_port.rs:109`).
- Drop the two clock reads per statement, or keep one pair per job.
- Risk: low to medium. Per-statement `elapsed_ns` in numeric schema sections 2–3 would change or go; that is a receipt schema decision.

**P7. Reply before bookkeeping** (with P3). Moves about 1.5–2 µs per request to after the reply; no CPU saved.
- Invariant: publication still precedes the reply. Owner aggregates are settled before the turn is passed, so observers of the connection still see them current (`D/overlay/owner.rs:432-434`).

**P8. Stop waking the owner thread at every job end while maintenance is merely pending.** 0 in C01; unlink cells.
- Notify only when `maintenance_due()` or when nothing else will run (`D/overlay/queue.rs:376-387`). Removes a futex syscall per job and the collisions behind C03's owner wait.
- Invariant: bounded maintenance delay stays at 8 turns (`queue.rs:190-192`). Idle reclamation still needs a wake when the last job ends with the hint set; with a serial client that is the hard part, the same "is anything pending" question as P1.

**P9. Use the window after each reply for the predictable next request.** Unmeasured.
- After a negative LOOKUP: reserve the serial and bind the CREATE statements. After CREATE: P2's record. The budget is about 20 µs per reply.
- Speculative work must be effect-free and discarded on a miss.

**Considered and not proposed.**
- Reply before COMMIT: breaks publication before reply.
- Per-slot reuse of `Task` and future storage: P3 covers it.

**Context [inference].** On a 1-vCPU target the wake term disappears; the experiment's CF contract lists one (`core/docs/issues/305/CF-FSBENCH-CONTRACT.md:36`). There, the 3–5 µs of fixed cost per request and the service time are the whole cost, so P3–P7 weigh far more than on this 8-vCPU VM.