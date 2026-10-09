# R7: the two scheduler regimes of a sample

> **Status:** Research; informative and not a product contract. Read-only analysis by a research subagent, 2026-10-09.

Read-only research, 2026-10-09. Nothing was built, run, edited or measured. Product source was read in the
main checkout at `96f689d62`; every product file cited below is byte-identical at the sampled commit
`47ca8f80e` (`git diff --stat 47ca8f80e HEAD` over `layerfs-fuse/src`, `layerfs-daemon/src/overlay`,
`service`, `vendor/fuser-0.18.0/src` shows only `overlay/commands.rs`). A2 material was read with `git show`
from `codex/phase7-experiment-305`. Receipts are those under
`core/docs/issues/307/checks/r7-optimization-20261009/`.

Evidence grades used below:

- **[receipt]** computed from retained receipt fields or snapshot artifacts.
- **[source]** read in this repository at a cited line.
- **[kernel-cited]** checked today against the Linux **v6.12** tag (`fs/fuse/dev.c`, `kernel/sched/fair.c`,
  `core.c`, `features.h` on raw.githubusercontent.com/torvalds/linux/v6.12) through a summarising fetch: the
  logic and short conditions were confirmed, whole bodies were not quoted to me. The VM runs **6.12.76**
  stable, which I could not read; later EEVDF backports may differ.
- **[memory]** from memory only.
- **[inference]** my reasoning; not observed.

## 0. Short answers

1. **Your reading is right about what the rare regime is, and wrong only in what causes it to be chosen.**
   It is the client and `fuser-0` on one vCPU, the client preempting the daemon at every reply, the daemon
   never sleeping in `read()`. Both FUSE wakes are plain `wake_up()` in 6.12, so the kernel never asks for a
   same-CPU wake; the stacked state is an accident of where the two tasks sit when the command starts, and
   in these receipts it started almost **only when `fuser-0` had last run on CPU 0** (8 of the 9 samples with
   any stacking). Why it then persists for thousands of wakes while other vCPUs look idle is **not explained by
   the v6.12 source I could check**; three cheap observations would settle it (section 1.6).
2. **A2 was timed inside the container, unpinned, on all 8 vCPUs, with no CPU limit, same kernel and VM**,
   two receive loops, one record per cell. Its receipts carry no scheduler data. Its C01 costs 21.7 µs per
   request above native, against 37.8 for its own C03, 48 for its own unpinned A2S C01 and 39–47 for the
   same passthrough shape today: indirect evidence that the C01 record was mostly stacked. It cannot be
   proved.
3. **At the sampled commit the product wakes no other thread per request in C01.** One step per request on
   `fuser-0`, zero dispatcher wakes, every other daemon thread at zero switches and zero run time. A
   "handoff" is a request entering the dispatcher, not a thread switch.
4. **No legitimate product change makes the cheap regime more likely.** One legitimate change removes
   product-caused cross-thread wakes elsewhere: C09/C10 hand every READ to a worker "before provider I/O"
   although the receipts show zero Store object reads (all cache hits). That is 512 avoidable worker wakes
   per cell. It does not touch C01.
5. **Label by the receive thread's involuntary share**, compare wall time only within a label, and use
   counts and owner service time across labels. Note for the ledger: of C01's 379.5 → 183.8 ms, about 54 ms
   is the product (owner service 151.1 → 97.5 ms for 10,000 fewer Lease statements); the rest is regime.

## 1. Is the reading right?

### 1.1 What the receipts establish [receipt]

Per-thread deltas of `fuser-0`, with the CPU it last ran on before and after the command (snapshot
artifacts named by `daemon_thread_snapshots.before/after`, `THREAD` rows, last column):

| Receipt | Cell | commit | vol : invol | invol share | run ms | runqueue wait ms | wait per invol µs | CPU before → after | command ms |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | --- | ---: |
| 987 | C01 | c263919e7 | 3998 : 1 | 0.000 | 222.0 | 0.10 | — | 6 → 7 | 379.5 |
| **1047** | C01 | 47ca8f80e | 12 : 3991 | 0.997 | 119.3 | 16.29 | 4.1 | **0 → 0** | 183.8 |
| 1011 | C07 | c263919e7 | 1549 : 2 | 0.001 | 347.3 | 0.14 | — | 3 → 3 | 475.7 |
| **1071** | C07 | 47ca8f80e | 18 : 1532 | 0.988 | 322.6 | 35.12 | 22.9 | **0 → 0** | 411.6 |
| 1083 | C10 | 47ca8f80e | 1034 : 3 | 0.003 | 200.8 | 0.08 | — | 4 → 4 | 289.9 |
| **963** | C10 | 45bec69af | 33 : 1014 | 0.968 | 180.8 | 21.31 | 21.0 | **0 → 0** | 248.0 |
| **1023** | C10 | c263919e7 | 14 : 1022 | 0.986 | 169.8 | 19.64 | 19.2 | **0 → 0** | 240.4 |
| 815 | C03 | 3558fb2be | 4954 : 2080 | 0.296 | 318.1 | 9.87 | 4.7 | **0 → 0** | 694.6 |
| 767 | C06 | eb170b029 | 192 : 326 | 0.629 | 161.4 | 11.90 | 36.5 | **0** → 2 | 233.7 |
| 887 | C06 | 989bfa887 | 431 : 87 | 0.168 | 165.7 | 3.48 | 40.0 | **0** → 2 | 235.7 |
| 1051 | C02 | 47ca8f80e | 3833 : 1173 | 0.234 | 204.0 | 5.31 | 4.5 | **0** → 7 | 862.3 |
| 831 | C07 | 3558fb2be | 759 : 790 | 0.510 | 341.6 | 10.57 | 13.4 | 3 → 3 | 432.6 |

What this fixes:

- **Same CPU.** A thread that is switched out while still runnable once per request, and waits about as long
  as the client needs between two requests (4 µs for the `echo` loop, 20 µs for `cp`'s 128 KiB copies), is
  sharing its CPU with the client. On separate CPUs nothing preempts it.
- **The daemon never sleeps in the rare regime.** 12 voluntary switches in 5,001 requests: every `read()`
  finds the next request queued, because the client ran in between. So one of the two wakes per request
  (client → daemon) does not happen at all, and the other (daemon → client) is a local preemption with no
  idle exit and no IPI.
- **The CPU was not shared with anything else.** 1047: `fuser-0` ran 119.3 ms and waited 16.3 ms = 135.6 ms,
  which is the whole in-container command (183.8 ms span minus about 45 ms of Docker launch,
  `floor-passthrough-63c48d8dc.md:9-13`). A third runnable hog on that CPU would have taken a share.
- **The receive thread's own CPU time is regime-dependent.** 222.0 ms (44 µs per request) usual against
  119.3 ms (24 µs) stacked for C01, across a 10,000-statement change worth about 54 ms (section 5.3). Going
  to sleep and coming back from an idle vCPU is charged to the woken thread; this matches the floor probe's
  11.9 µs of daemon CPU for a request with no work (`floor-passthrough-63c48d8dc.md:14-18`).
- **Usual is cross-CPU.** Runqueue wait is 20–130 ns per wake in every usual sample, which is the signature
  of a wake onto an idle CPU through the remote wake list, not of a local wake that waits for the waker to
  block (that would be 0.3–1 µs per wake).

### 1.2 Where it starts: CPU 0 [receipt]

Over all 72 `L` samples that carry thread snapshots (747 … 1091):

| `fuser-0` CPU before the command | samples | whole command stacked | part stacked | none |
| --- | ---: | ---: | ---: | ---: |
| CPU 0 | 16 | 4 (963, 1023, 1047, 1071) | 4 (767, 815¹, 887, 1051) | 8 |
| CPU 1–7 | 56 | 0 | 1 (831, CPU 3) | 55 |

¹ 815 is C03: `bash` (5,000 requests) then `rm` (about 4,000). Its 2,080 involuntary switches are about one
of the two client processes, and `fuser-0` was still on CPU 0 at the end: the only part-stacked sample that
ended stacked.

- Whenever `fuser-0` started on CPU 0 and was **not** stacked, it left CPU 0 (0 → 6, 5, 4, 3, 1, 6, 5, 5).
  There is no sample with "before 0, after 0, usual". So in the 16 samples something was on CPU 0 at the
  first request every time.
- [inference] The simplest reading is that the `docker exec` child usually starts on CPU 0, and the cell is
  stacked only if `fuser-0` happens to be there too. The client's CPU is not recorded, so this is not
  observed.
- The mixed samples divide by client process (C02: `bash` then 1,000 `stat`; C03: `bash` then `rm`; C07:
  `dd` then `cp`), which fits "the regime is fixed when a client process meets the daemon and rarely changes
  afterwards". C06 (one `dd`) changed once in mid-process in two samples, so it can break.
- The product does not choose this CPU. `fuser-0` had exactly one timeslice before the command in all 72
  samples: its creation by fuser (`core/vendor/fuser-0.18.0/src/session.rs:305-319`), placed by the
  kernel's fork balance.

### 1.3 The FUSE wake paths [kernel-cited, v6.12 `fs/fuse/dev.c`]

- Request queued: `fuse_dev_queue_req()` → `fuse_dev_wake_and_unlock(struct fuse_iqueue *fiq)` →
  `wake_up(&fiq->waitq)`. The function takes only the queue; **there is no `sync` parameter and no
  `wake_up_sync` / `__wake_up_on_current_cpu` anywhere in the file.**
- Daemon side waits with `wait_event_interruptible_exclusive(fiq->waitq, !fiq->connected ||
  request_pending(fiq))` in `fuse_dev_do_read()`: it sleeps only if nothing is pending.
- Reply: `fuse_request_end()` → `wake_up(&req->waitq)` for a foreground request ("Wake up waiter sleeping in
  request_wait_answer()"). A background request (RELEASE) wakes nobody.
- [memory] Kernels 6.4–6.11 passed `sync=true` from `__fuse_request_send` to a `wake_up_sync` in
  `fuse_dev_wake_and_unlock`; the 6.12 request-queue rework dropped it. I could verify only the 6.12 end
  state. It matters little: a sync hint still goes to an idle previous CPU (1.4).

So in 6.12 neither wake carries WF_SYNC or WF_CURRENT_CPU. Placement is entirely the ordinary fair-class
wake path.

### 1.4 The wake placement path [kernel-cited, v6.12 `kernel/sched/fair.c`, `core.c`]

`try_to_wake_up()` → (if the task is still on a runqueue: `ttwu_runnable()`, which wakes it **in place**,
never calls `select_task_rq` and never changes its CPU) → else `select_task_rq_fair()`:

1. `record_wakee()`: the waker's `wakee_flips` is incremented when it wakes a different task than last
   time, and halved once a second.
2. `WF_CURRENT_CPU` → return this CPU (not set by FUSE).
3. `want_affine = !wake_wide(p)`. `wake_wide` is true only if the smaller flip count ≥ `sd_llc_size` **and**
   the larger ≥ smaller × `sd_llc_size`. A strict client ↔ daemon ping-pong keeps both near zero, so it is
   never "wide" with an 8-CPU LLC. (With no LLC domain the factor is 1 and almost every pair is wide.)
4. If affine and waker CPU ≠ wakee's previous CPU: `wake_affine()`. `wake_affine_idle()` returns, in order:
   this CPU if it is idle and shares cache with prev (prev if prev is idle too); this CPU if `sync` and it
   runs one task; **prev if prev is idle**; otherwise undecided, and `wake_affine_weight()` compares loads.
5. `select_idle_sibling(p, prev, target)`: target if idle; **prev if idle and cache-sharing**; a per-CPU
   kthread shortcut; `recent_used_cpu` if idle; **`if (!sd_llc) return target`**; then
   `select_idle_cpu()`, which scans the LLC from `target + 1` for the first idle CPU, limited by SIS_UTIL
   (`nr = nr_idle_scan + 1`, and no scan at all when that is 1, i.e. LLC utilisation ≥ about 85 %); if
   nothing is found, target.
6. On the chosen CPU `check_preempt_wakeup_fair()` preempts only if `pick_eevdf()` picks the wakee.
   `pick_eevdf()` drops `curr` if it is not eligible, and otherwise keeps `curr` while RUN_TO_PARITY's
   slice protection holds (`curr->vlag == curr->deadline`). Defaults in 6.12 `features.h`: PLACE_LAG,
   RUN_TO_PARITY, PREEMPT_SHORT, DELAY_DEQUEUE, DELAY_ZERO, WAKEUP_PREEMPTION, SIS_UTIL, TTWU_QUEUE,
   WA_IDLE, WA_WEIGHT, WA_BIAS all true; NEXT_BUDDY false. Base slice 0.75 ms × (1 + ilog2(min(ncpus, 8)))
   = 3 ms here.

### 1.5 Why the usual regime is stable, and why the stacked one is fast [inference from 1.3–1.4]

- **Usual.** Client on A, daemon asleep on B. The client's wake finds B idle → B (step 4 or 5). The daemon's
  reply finds A idle → A. Neither task ever has a reason to move, and each wake is an idle exit of another
  vCPU: the 18 µs per wake measured by the pipe probe (`floor-passthrough-63c48d8dc.md:114-130`), which in
  a guest includes the hypervisor waking the vCPU thread [memory: WFI traps to the host; not measured
  here]. Flat in the peer's idle time up to 80 µs (same file, H5).
- **Stacked.** The reply wakes a client whose previous CPU is the daemon's own. If the client lands there,
  it has had far less service than the daemon (4 µs against 24 µs per cycle), so the daemon is ineligible
  under EEVDF, loses its slice protection and is preempted at once. The client runs to its next request and
  blocks; the daemon, still runnable, resumes inside `writev`, loops to `read()` and finds the request
  queued. No idle exit, no IPI, no client → daemon wake. That is the 2.5 µs pipe and 5.2 µs FUSE round trip
  that #305 measured with both sides on one CPU, against 42–44 µs unpinned
  (`codex/phase7-experiment-305:core/docs/issues/305/A2-WALK-REPORT.md:135-146`).
- **Conditions for entering it.** Client and daemon must share a previous CPU at the first request, and the
  wake must not move either away.

### 1.6 What I cannot explain, and what would settle it

By the v6.12 source, with an 8-CPU LLC domain and idle vCPUs, step 5 moves the wakee to an idle CPU the
first time it is woken onto a busy one. The stacked regime should therefore last one wake. It lasted 3,991.
I worked through the delayed-dequeue path (`ttwu_runnable` wakes a `sched_delayed` task in place) and it
does not rescue this: a client that uses 14 % of a shared CPU is eligible when it sleeps, so it is really
dequeued and its next wake goes through `select_task_rq_fair` again. So one of these holds, and I could not
tell which from this side of the VM:

| Hypothesis | Predicts | Against it |
| --- | --- | --- |
| H-a: no LLC domain in this kernel (`CONFIG_SCHED_MC` unset on arm64, so `sd_llc` is NULL and step 5 returns target) | Stacking sticks on any CPU once the pair meets | `fuser-0` changes CPU without stacking in about half of the usual samples, which needs an idle search; the floor probe saw 0–4 involuntary switches in about 45 passthrough arms |
| H-b: the other vCPUs were not idle (another worktree's build or test in the same VM; SIS_UTIL stops scanning) | Stacked samples coincide with VM load; `/proc/stat` shows cpu1–7 busy | Why always CPU 0 (possibly: interrupt load makes CPU 0 look fuller, so hogs settle on 1–7) |
| H-c: 6.12.76 differs from 6.12.0 in the wake or EEVDF path | — | Not checkable here |

Decisive, read-only, outside the timed span (harness, not product):

1. Once: `/sys/devices/system/cpu/cpu*/topology/{core_siblings_list,cluster_cpus_list}`,
   `cpu0/cache/index*/shared_cpu_list`, `/proc/schedstat` `domain` lines (or
   `/sys/kernel/debug/sched/domains/cpu0/*/{name,flags}`), `/sys/kernel/debug/sched/features`,
   `zcat /proc/config.gz | grep -E 'SCHED_MC|CONFIG_HZ='`. Settles H-a.
2. Per sample, in the existing before/after snapshot: the eight `cpuN` lines of `/proc/stat` and
   `/proc/loadavg`. Settles H-b and shows on how many vCPUs the cell ran. The floor probes recorded
   loadavg 0.96 0.84 0.90 at their time (`711-floor-C01/N/snapshot-before.stdout`): about one busy task in
   the VM besides the probe.
3. Per sample: `se.nr_migrations` from `/proc/1/task/<tid>/sched` for `fuser-0`.

### 1.7 Why it is rare here [receipt + inference]

It needs the client to meet the daemon on one CPU at the first request (16 of 72 had the daemon on CPU 0)
and then to survive the first wakes (4 of 16 for the whole command, 4 of 16 for part of it). Overall 4 whole
and 5 part of 72.

## 2. How A2 was measured

All paths are on branch `codex/phase7-experiment-305` unless marked main.

| Question | Answer | Where |
| --- | --- | --- |
| What A2 is | "the promoted cached passthrough profile; enforces permissions", ext4 passthrough, no store | `core/docs/issues/305/CF-FSBENCH-CONTRACT.md:88`; main `core/docs/issues/307/HANDOFF-R7-BEAT-A2-20261009.md:43-45` |
| Clock | Python `time.monotonic()` **inside the container**: `t=time.monotonic()` before `subprocess.Popen(['/bin/bash','-o','pipefail','-c',COMMANDS[a.test]], …)`, `phases['exec']=time.monotonic()-t` after `wait_exit`, which waits on a pidfd | `core/experiment/real-tree/runner.py:275-280`, `:109-117`; contract "`runner.py` now waits on a pidfd (`os.pidfd_open` + `select`)" `CF-FSBENCH-V2-CONTRACT.md:68-71`; receipt field `timer: "monotonic; phase ends at pidfd readiness"` |
| What the span holds | bash start, body, exit. No Docker launch | main `core/docs/issues/307/r7-open/floor-passthrough-63c48d8dc.md:9-13`, `:420-421` |
| CPUs | 8, unpinned: receipt `pinned_cpu: null`, `daemon_cpus: "0-7"`, `command_cpus: "0-7"` for all twelve A2 rows | `core/docs/issues/305/CF-FSBENCH-V2-RECEIPTS.json` (C01/A2 is the third record); "Docker Desktop VM on macOS, 8 CPUs; N1 and A2S1 pin to one CPU" `CF-FSBENCH-REPORT.md:27-28` |
| cpuset / `--cpus` / cgroup limit | None. `reproduction_command` is `docker run --rm --pull never --network none --privileged --security-opt no-new-privileges -e … -v …`; no `--cpus`, `--cpuset-cpus` or `--cpu-*` in any of the 60 records. Pinning exists only in arms N1 and A2S1 (`os.sched_setaffinity(0,{cpu})`, `taskset -c`) | same JSON, `host_identity.reproduction_command`; `runner.py:175-180`, `:216`, `:265` |
| Kernel / VM | `kernel: "6.12.76-linuxkit"`, this machine's Docker Desktop VM, 2026-10-04/05 | same JSON; `CF-FSBENCH-REPORT.md:27-28` |
| Processes | Three in one container: the Python runner (parent, asleep in `select` on the pidfd), the passthrough daemon, the command | `runner.py:262-280` |
| Receive loops | Two: `c.n_threads = Some(2); c.clone_fd = false;` | `core/experiment/real-tree/src/main.rs:765-766` |
| Samples | One record per cell, fresh mount, `sync` + VM `drop_caches=3` directly before the timers, uid 1000, no warm-up | `CF-FSBENCH-REPORT.md:417-420`; `runner.py:247-249`, `:78-81` |
| Interference | "none deliberately introduced; shared Docker VM, three other owners' containers present and at 0–1.6% CPU" | `CF-FSBENCH-REPORT.md:435-437` |
| Scheduler data | None. `daemon_stats: {}`, `callback_costs: {}` for A2 | same JSON |

Main-checkout references to the envelope: `core/target/r7-summary.py:5-6` ("another envelope … a reference
line, not a matched ranking"), `HANDOFF-R7-BEAT-A2-20261009.md:83-87` and `:221` ("Do not pin CPUs to make a
sample pass: A2 ran unpinned").

**Was A2 taken in the same-CPU regime?** No direct evidence exists. Indirect, from the v2 receipts:

| Row | Exec ms | requests | native N ms | (Exec − N) / request µs |
| --- | ---: | ---: | ---: | ---: |
| A2 C01 | 183.4 | 7,001 | 31.8 | **21.7** |
| A2 C03 | 410.8 | 10,011 | 32.5 | 37.8 |
| A2 C12 | 189.3 | 3,674 | 33.1 | 42.5 |
| A2S C01 (same binary, unpinned) | 225.1 | 4,003 | 31.8 | 48.3 |
| A2S1 C01 (pinned to CPU 7, instrumented) | 129.6 | 4,003 | 25.7 (N1) | 26.0 |
| Today, same shape (two loops, FLUSH answered), in-container | 312.8–326.3 | 7,000 | 15.5 | 42.5–44.4 |

- A2 C01 sits at the pinned arm's cost per request, not at the unpinned one. The #305 report itself records
  the anomaly and leaves it open: "A2S did not help create. C01/A2S is 225 ms against A2's 183 ms with 4,003
  requests against 7,001. The same order appeared in v1. Unexplained" (`CF-FSBENCH-REPORT.md:113-115`). So
  does its walk report: "A2O's 29.5 µs per request is lower than this probe's figure and that difference is
  not explained" (`A2-WALK-REPORT.md:149-150`).
- The floor note already reached "H4 … Supported" (main `floor-passthrough-63c48d8dc.md:378`).
- Caution: the v1 record of the same cell was also fast (229.6 ms on a 51 ms grid). Two fast records out of
  two is more than a 1-in-8 accident would give, so A2's shape (two loops, FLUSH answered, the Python parent
  on a third CPU) may favour stacking. Not known.
- A2 C03 and C12 look like the usual regime. The A2 column is therefore a mix of regimes, cell by cell,
  one sample each.

## 3. What the product does per request

### 3.1 The receive loop [source]

- One loop per mount: `RECEIVE_SLOTS = 1` (`core/crates/layerfs-fuse/src/dispatch/types.rs:6`),
  `options.n_threads = Some(RECEIVE_LOOPS)` (`session/startup.rs:89`).
- It blocks in a plain `read()`; no poll, epoll or timeout:
  `nix::unistd::read(&self.0, buffer)` (`core/vendor/fuser-0.18.0/src/channel.rs:30-32`), called from
  `receive_retrying` in the loop (`session.rs:546-571`).
- A reply is one `writev` on whichever thread calls it: `nix::sys::uio::writev(&self.0, bufs)`
  (`channel.rs:86-87`).
- No timer and no periodic wake exists in the request path. The only timed wait is `wait_until`
  (`dispatch/queue.rs:147-160`), used by drain observation. A search of `layerfs-fuse`, `layerfs-daemon`,
  `layerfs-overlay` and `layerfs-workspace` for `notify_*`, `unpark`, `wait_timeout`, `sleep`, `eventfd`
  finds nothing else on the path.

### 3.2 Dispatch [source]

- `Permit::handoff` runs the request's **first step on the calling (receive) thread**
  (`dispatch/admission.rs:253-299`: "A request whose prerequisites are all immediately available therefore
  completes there, with no hand-off").
- A worker is woken only by `wake_worker`, and only if one is idle: `if state.idle != 0 {
  self.runnable.notify_one(); }` (`dispatch/queue.rs:136-140`). Callers: a parked request made runnable by
  another thread (`task.rs:83-91`), a step that asked for another turn (`task.rs:238-245`), and
  `release_woken` (`task.rs:147-155`).
- Observers are notified only if one waits: `if state.observers != 0` (`queue.rs:130-134`).
- **A "handoff" is `Disposal::Handoff`**: "An owned bounded unit entered the shared dispatcher with its
  reply. Its first step, and with it the reply, may still run on the loop"
  (`request/accounting.rs:54-56`, counted at `request/state.rs:85-88`). 5,000 handoffs for 5,001 requests
  means 5,000 requests took a dispatcher slot and one (FLUSH, answered ENOSYS) was `refused`. It says
  nothing about threads. The thread-relevant counters are `MountWork.steps` and `MountWork.wakes`
  (`dispatch/diagnostics.rs:14-15`; receipt `observations.mount_work`, values[10] and [11], field order at
  `core/crates/layerfs-daemon/src/application/diagnostics/native.rs:46-62`).

### 3.3 The owner [source]

- The submitting thread serves its own job when the connection is free and nothing else is queued:
  `if !held && state.work.queued == 1 && !state.maintenance_due() { state.take() }` →
  `serve_submitted` (`core/crates/layerfs-daemon/src/overlay/owner.rs:355-366`).
- The owner thread is woken only when a free connection has a job this thread did not take
  (`owner.rs:371-376`), or when a turn ends with something queued or maintenance pending
  (`overlay/queue.rs:382-387`, `:362-366`).
- The job's completion wakes a thread only if one is parked on it (`service/completion.rs:87-92`) and the
  credit release only if an admission wait is reserved (`overlay/admission.rs:86-88`).

### 3.4 C01 counted [receipt + source]

Per file: LOOKUP, CREATE, WRITE, RELEASE, GETATTR (1,000 each), plus one FLUSH refused.

| Per request | From source | Receipt 987 (usual) | Receipt 1047 (stacked) |
| --- | --- | ---: | ---: |
| Dispatcher steps | 1, on `fuser-0` | 5,000 / 5,000 handoffs | 5,000 / 5,000 |
| Dispatcher wakes (`MountWork.wakes`) | 0 | 0 | 0 |
| Owner jobs | 1, served inline | 5,003 admitted in the window (4 → 5,007) | 5,003 (4 → 5,007) |
| Wakes of another daemon thread | 0 | all of `layerfs-overlay`, `layerfs-fuse-0..3`, `layerfs-mount-2`, `layerfs-fence-2`, `layerfs-daemon`: 0 switches, 0 ns run | same |
| Kernel wake of `fuser-0` | 1 if it sleeps | 3,998 (one request per file is found queued: RELEASE is asynchronous) | 12 |
| Kernel wake of the client by the reply | 1 per foreground request (4 per file) | 4,000 | 4,000, all local preemptions |
| Order | reply (`writev`) is the last thing a completing step does; after it only uncontended mutex updates (`task.rs:289-297`) | — | — |

`layerfs-control` shows one switch and about 1.5 ms: the closing status call of the snapshot itself.

**So H3 of the floor note is settled for C01: there is no hidden per-request wake.** The product does not
make itself "wide" and does not push the kernel anywhere.

### 3.5 Where the product does wake other threads [receipt, batch 1047–1091 at 47ca8f80e]

| Cell | requests | steps | dispatcher wakes | worker switches | owner-thread switches |
| --- | ---: | ---: | ---: | ---: | ---: |
| C01, C02, C06, C07, C08 | — | = handoffs | 0 | 0 | 0 |
| C03 | 9,021 | 9,075 | 55 | 38 | 1 |
| C04 | 5,342 | 5,351 | 10 | 10 | 10 |
| C05 | 5,889 | 6,460 | 581 | 329 | 110 |
| **C09** | 517 | 1,032 | 516 | **513** | 0 |
| **C10** | 1,546 | 2,060 | 516 | **515** | 0 |
| C11 | 517 | 520 | 4 | 0 | 0 |
| C12 | 3,436 | 3,460 | 25 | 13 | 5 |

C09/C10: every one of the 512 READs goes receive thread → worker → reply from the worker. The four idle
workers are woken in rotation (128 each), because a futex wakes its longest waiter [memory].

## 4. Legitimate changes

**Nothing legitimate makes the cheap regime more likely.** User space cannot ask `/dev/fuse` for a
same-CPU wake, the product already wakes nobody in C01, and the remaining levers are the forbidden ones
(affinity, spinning, extra loops). Fewer synchronous requests remains the only honest way to buy back wake
cost; that is request-count work, not regime work.

Candidates that remove product-caused cross-thread wakes:

### 4.1 Leave the receive thread only when a provider wait is really needed (recommended to investigate)

- **Where.** `core/crates/layerfs-fuse/src/operations/read.rs:80-83`:
  `// Provider reads never run on the loop that received the request.` /
  `crate::LeaveReceiver::default().await;` / `Some(self.services.base().await?)`. The same pattern at
  `operations/lookup.rs:240-242` and `operations/directory.rs:262-264`.
- **What the receipts show.** C09 (1079) and C10 (1083): 512 READ, `reader grants 512`,
  `store {'object_batches': 0, 'object_ids': 0}`, `class B object demands 0`, `immutable cache
  {'cache_hits': 3118 / 3160}`. No provider I/O happened, yet all 512 requests were handed to a worker.
- **Change.** Take the hand-off only when the base read cannot be answered from what is already resident;
  answer a cache-resident window in the first step, as a mutation already is.
- **Why generic.** It is the dispatcher's own stated rule (`admission.rs:248-252`) applied to reads; it
  depends on residency, not on a workload. Nothing is pinned, spun, added or timed.
- **Proof by count.** In C09/C10: `mount_work` steps = handoffs and wakes = 0; `layerfs-fuse-*` voluntary
  switches 0 (now 513/515); in a cold-Store case the counts stay as now.
- **Size.** 512 × one extra cross-CPU wake ≈ 9 ms at 18 µs, of 100 ms (C09) and 290 ms (C10). Arithmetic,
  not a measurement.
- **Risk.** (a) Needs a way to know residency before acquiring the reader grant; if the grant itself can
  wait, the step is no longer bounded. (b) The decode and copy of a 128 KiB window then runs on the one
  receive loop of the mount: about 41 µs per READ of worker CPU today (21 ms over 512), against 162 µs a
  WRITE already spends there. Concurrent clients of that mount wait behind it. (c) It reverses a stated
  design line, so it is the owner's or the lead's call, not a silent edit.

### 4.2 Wake the most recently parked worker, not the longest-parked one (weak)

- **Where.** `dispatch/queue.rs:136-140` (`runnable.notify_one()`), waiters at `:120-128`.
- **Change.** Per-worker condition variables and a stack of idle workers. No thread or loop is added.
- **Proof by count.** In a serial read cell one worker carries all the switches instead of four sharing them.
- **Risk / value.** The pipe probe found wake cost flat in the peer's idle time up to 80 µs (floor note,
  H5), so the gain may be nil. Superseded by 4.1 where 4.1 applies. Not recommended on its own.

### 4.3 Considered and rejected

- **Do not notify the owner thread when no maintenance is pending.** Already the case
  (`queue.rs:382-387`, `:362-366`); C01's owner thread has zero switches. Deferring a pending-maintenance
  wake further would trade reclamation promptness; C03 is already at 1 owner switch.
- **Reply before any other wake.** Already the case for requests that complete in their first step. For a
  handed-off request the reply does not exist yet when the worker is woken.
- **One fewer syscall per request.** The loop is `read` + `writev`. Rust's `Condvar::notify_*` issues a
  futex call even with no waiter [memory], but every notify on the path is already guarded by a waiter
  count.
- **A second receive loop.** Measured not to help a serial client (floor note, headline 6), and it is an
  extra loop.

## 5. Honest reporting

### 5.1 Label [receipt fields only]

From `observations.daemon_threads.value.threads[comm == "fuser-0"]`:
`s = nonvoluntary_switches / (voluntary_switches + nonvoluntary_switches)`.

| Label | Rule | Observed range in 72 samples |
| --- | --- | --- |
| SEPARATE | `s < 0.02` | 0.000–0.004 (63 samples) |
| STACKED | `s > 0.90` **and** `runqueue_wait_ns / nonvoluntary_switches` between 1 and 100 µs | 0.968–0.997 (4 samples), 4–23 µs |
| MIXED | anything else | 0.168–0.629 (5 samples) |
| CONTENDED (define now, none seen) | `s > 0.02` with wait per involuntary switch above 100 µs | — |

- The gap between 0.004 and 0.168 is empty, so the thresholds are not delicate.
- Guard: if the voluntary + involuntary total is under about 100, print the label as unavailable.
- Print `s`, the CPU before → after, and wait per involuntary switch beside every command time.
- A MIXED row is not comparable with anything; say so in the row.

### 5.2 What to compare

1. **Counts first**: requests by opcode, owner jobs by class, statements by family, `mount_work` steps and
   wakes, voluntary switches of every thread other than `fuser-0`. They are identical across regimes.
2. **Owner service ns** (`owner_work`, the summary's "service ns"). Regime-insensitive in the pairs
   available: C07 308.2 (usual, 1011) against 303.1 ms (stacked, 1071) at equal statements; C10 155.7
   (stacked, 1023), 165.7 (stacked, 963), 171.3 ms (usual, 1083). Spread about ±5 %, not ordered by regime.
3. **Wall time only within one label**, same cell.
4. **Do not use** `fuser-0` `run_ns`, cgroup `usage_usec` or `system_usec` across labels: all three carry
   the sleep/wake cost (C01: 222 → 119 ms, 337 → 202 ms, 157 → 74 ms).
5. A derived figure that is honest across labels: `command − owner service − launch` divided by blocking
   wakes (`fuser-0` voluntary switches) gives the environment's price per wake in a SEPARATE sample; it is
   not a product quantity and should be reported as environment.

### 5.3 The C01 comparison as it should read

| | 987 (c263919e7, SEPARATE) | 1047 (47ca8f80e, STACKED) | Difference |
| --- | ---: | ---: | ---: |
| Statements | 54,002 | 44,002 | −10,000 (Lease 19,000 → 9,000) |
| Owner service ms | 151.1 | 97.5 | **−53.6** (5.4 µs per statement removed) |
| Command ms | 379.5 | 183.8 | −195.7 |
| Attributable to the product | | | about −54 |
| Attributable to the regime | | | about −142 |

A SEPARATE sample of 47ca8f80e C01 should be expected near 325 ms, not 184 ms. The A2 number for C01
(183.4 ms) equals the stacked time and is, on the evidence of section 2, probably a stacked record itself.

## 6. Limits

- The kernel analysis is for v6.12.0; the VM runs 6.12.76. I did not read the VM's configuration, topology
  or scheduler features; section 1.6 is open because of that.
- The client's CPU is not in any receipt. "The client starts on CPU 0" is an inference from where the
  daemon had to be for stacking to occur.
- One sample per cell and identity. The 4-of-16, 8-of-16 and 9-of-72 frequencies are descriptive.
- Section 4.1's saving is arithmetic from a wake price measured on another day.
- I fetched kernel source from the public v6.12 tag for citation; no file was downloaded or stored.
