# R6 audit findings (input to the R6 plan)

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Two read-only audits at `a7ef29672`, taken while R5's proof tracks ran. They
ran no Cargo, Docker or test command. The lead verified H-A and H-B against
source; the rest is to be checked while planning. SPEC is
`S8-SPECIFICATION-20261008.md`, PLAN is `S8-PROOF-PLAN-20261008.md`, UM is
`303/workspace-api/unmount.md`.

## A. Hazards in the Commit path under concurrent requests

- **H-A, install can deadlock on owner slots (verified in source).** A queued
  install holds back later Source-class jobs
  (`layerfs-daemon/src/overlay/queue.rs` `Lane::take`), and those parked jobs
  count against the Workspace's 16 ordinary slots
  (`overlay/owner.rs` `try_submit`, `jobs_per_namespace`). A mount admits 16
  requests; each takes its source first and then needs a further ordinary
  job. Sequence: request H holds a source; Commit queues install (1 slot); 15
  new requests queue Source jobs (15 slots, parked). H's next job gets
  `AdmissionFull` and waits for a credit that can never free: install waits
  for H's release, the Source jobs wait for install. Candidate fix: count
  Source-class jobs in their own per-lane counter so parked acquisitions never
  consume the slots their predecessors need. Capture has no such cycle: its
  dependencies are already-admitted Mutation jobs and Lifecycle-class
  `ReplyAttempted`.
- **H-B, Commit never waits for admission (verified).** `BoundWorkspace::job`
  and every synchronous captured port use `try_submit`; with 16 ordinary
  credits in use `Capture` is refused before effect as `Capacity`, a
  construction port call is a definite settled refusal, and the same refusal
  at `InstallPrepared` comes after publication and leaves `LocalFailure`
  custody. Fuse requests instead wait through
  `OwnerClient::submit_when_available` (`overlay/admission.rs`), a
  before-attempt readiness wait. Candidate fix: the Commit thread waits for
  admission the same way (a blocking wait on the `Admission` future), which is
  a readiness wait, not a retry. Registration slots: 288; Fuse budgets 16 per
  mount.
- **H-C, install has no bound.** It parks until `base_readers == 0`. A request
  that ends `Retained` keeps its source and fences its mount
  (`layerfs-fuse/src/dispatch/task.rs`), so an install queued behind it never
  runs. Force "refuses active control producers before any effect", so the
  only exit today is `Owner::stop`. No recovery path is documented.

## B. Forced teardown: nothing exists in source

Required sequence (SPEC 865-975, UM 122-168): before-effect guard (Busy while
Commit, Attach or another control producer runs; refusal when the abort
control is not bound) → Stopping → one one-byte write on the abort descriptor
bound at Attach → wake receive-capacity waiters, complete unattempted parked
replies, preserve attempted results → local consumer drain → exactly one plain
umount → five-part drain predicate → revoke → Close → `ForceUnmounted { token,
outcome }` or `Retained(TeardownCustody)`. Post-abort EBUSY is
`Retained/aborted-but-still-mounted`, never the reversible Busy. Stopped
unknown Commit custody needs explicit `relinquish_unknown` and stays Unknown
in the receipt. Forbidden: MNT_FORCE, lazy detach, retry, guessed connection
number, process kill, waiting for output drain.

Source today:

- `layerfs-fuse/src/mount/syscalls.rs` `abort_control` opens
  `/sys/fs/fuse/connections/<mountinfo minor>/abort` at Attach
  (`session/startup.rs`), keeps it in `NativeSession.abort`; `NotFound` gives
  `None`; nothing ever writes it. `abort_bound` was false in every recorded
  run because fusectl is not mounted in the container. No test or tool mounts
  fusectl.
- `session/drain.rs` `drain()` refuses unless already detached and covers
  parts 1–2 of the SPEC predicate. `MountQueue::stop_admission` wakes waiters;
  no terminal reply for already-parked unattempted requests exists.
- Normal unmount: `control/unmount.rs`, `control/detach.rs`. `idle()` refuses
  Uncertain and LocalFailure, so it cannot be the forced guard as written. A
  second attempt after a Retained first returns the stored custody. R3 record:
  a Workspace that reaches `Retained` at unmount has no path back; "Both
  recoveries are R6".
- Wire: request tags 1–10 used, 11 free; reply tags 1–13 used, 14 free. SPEC
  D-1 decides `ForceUnmount` as an additive tag. `TeardownCustody` has no
  abort disposition or publication knowledge; `NativePhase` has no `Stopping`;
  `TeardownStage` has no `Abort`.
- Sandbox container setup (`layerfs-sandbox/src/backend/docker/container.rs`)
  has no fusectl or `/sys` mount; topology inspection requires exactly its
  current set.

Product gaps, with owner: abort write and classification (Fuse
`mount/syscalls.rs`, small); forced session sequence (Fuse `session/`,
medium); abort/detach facts and a drain stage (Fuse `session/state.rs`,
small); terminal disposal of parked unattempted requests (Fuse `dispatch/`,
medium to large); wire records and tags (Bridge, medium); SDK `force_unmount`
(small); daemon forced admission, `relinquish_unknown`, sequence and outcome
(daemon `control/`, medium to large); aggregate drain gauges (daemon, medium
to large); fusectl availability (owner undecided); recovery out of Retained
(unspecified).

Hook-free holds reachable through public APIs: a Commit held before Save
finish with `Service::execute(&Request::Commit, blocking closure)`; before
publication and before local install with a blocking `HistoryCatalog` wrapper
(new test support beside `tests/support/history_boundary.rs`); an external
held Store writer (`tests/support/held_writer.rs`); all Store read leases
taken by the test through `Store::read_ticket(..).wait()`; a source held
through the public owner (`tests/native_custody.rs`).

Silences, with the conservative reading: drain order (local drain before
detach, full predicate after); existing tag 12 stays byte-identical; new enum
values in existing records need care; missing abort capability is a typed
before-effect refusal with the Workspace left Ready; any abort write return
other than 1 is Retained; no silent daemon mount of fusectl; force from
phases other than Ready returns existing custody or a refusal;
`relinquish_unknown` is a required explicit field and false gives an Unknown
refusal before effects; no second abort or umount after Retained.

## C. Leftover proof rows

| Row | State | Hook-free route |
| --- | --- | --- |
| FP-2 fusectl half | NOT_RUN | Mount fusectl in the test container before Attach; compare `max_background`, `congestion_threshold` with the receipt; assert `abort_bound` |
| FP-8 | component only | Sibling half: a parked install on A with many callers, B completes. Same-mount half: all read leases held by the test, a lease-free request completes |
| FP-9 | NOT_RUN | Leases held, a buffered cold read and a second process's cold read; kernel queueing from fusectl `waiting`; needs fusectl |
| FP-15 header identity | PARTIAL | None without accounting of flagged WRITE headers; Commit inclusion closed by R5-9 |
| FP-16 fixture half | NOT_RUN | No real-resource park after the attribute is sampled |
| FP-17 | halves passed in R2 and R3 | Nothing new to stage; needs a whole-row verdict |
| FP-19-FS | none | Park a request (leases held), force, the syscall returns an error with no signal, no Close until the held job ends |
| FP-21 | PARTIAL | A lease-parked request when detach is known; a test-held Store consumer is not seen by Close |
| FP-22-FS | none | External cwd, descriptor and mapping give Busy; a reference on A never holds B; force on A leaves the holder alive |
| FP-23-FS | none | FP-32's refusal plus FP-33's success path; caller PID alive throughout |
| FP-24 | none | Two Branches plus a fork on one daemon; isolated writes; a stale token refused |
| FP-26 | none | Plateau over cycles (below). Refusal half (`mount:debt`, stopped maintenance visible and refusing mounts) is not implemented |
| FP-27 | component only | Quarantine a reader as the component test does, then read cold through the mount |
| FP-29 | PARTIAL | A lease-parked READ on one descriptor while a sibling handle closes and the name is unlinked |
| FP-31 | PARTIAL | Many lookups, cgroup reclaim, `forget_units`; detach with K outstanding |
| FP-32 | none | Force refused Busy in each of the three Commit holds; unknown custody with and without relinquish |
| FP-33 | none | Needs fusectl; one abort write, one plain umount; cwd held by an external process gives aborted-but-mounted Retained |
| FP-34 | component only | 16 admitted plus 2 received under a parked install; terminal wake needs force |
| P-1 EAGAIN | NOT_RUN | Held Store writer, `touch` fails `EAGAIN`, name absent, one reservation attempt; no low-water value exists in source |

## D. Fairness and sustained cleanup

- SQL owner: one job per poll, rotating Workspaces then six classes; 16
  lanes, 16 ordinary and 2 lifecycle jobs each, 8 MiB credited bytes. Fuse
  workers: K = read handles + 2, one step per turn, rotating mounts. Store
  reads rotate Workspace lanes. Existing fairness proofs are component scope
  (`finite_service.rs`, `owner.rs`, `store_read_service.rs`,
  `layerfs-fuse/tests/dispatch.rs`).
- Maintenance is driven by the owner thread only: one turn after 8 foreground
  jobs or when idle, alternating live and closed work; the first failure
  stops maintenance for good. Observables: `Command::Resources { global: true }`
  → `StoredCounts` (`wait_refs, namespaces, inode_rows, directory_entry_rows,
  payload_cells, payload_bytes, shrink_rows, operation_record_rows,
  operation_record_bytes, orphan_rows, owner_rows, source_rows, owner_details,
  reply_tickets, retire_targets, maintenance_targets, ready_targets`),
  `database_pages`, `free_pages`; `OwnerWork.closed_namespaces`,
  `maintenance_jobs`, `maintenance_rows`; `maintenance_failure()`.
- A plateau over N mount/write/Commit/unmount cycles can state: every global
  `StoredCounts` field returns to its post-first-cycle baseline once idle;
  `closed_namespaces` equals the unmounted Workspaces; `database_pages` stops
  growing after an early cycle; no maintenance failure. Monotone by design:
  Workspace ids, generations, cumulative counters, the database high-water
  mark, Store history and objects (no GC).
- Output-backpressured caller, filesystem side: a process blocked writing to
  a full pipe while holding a descriptor, cwd and mapping in the mount; Commit
  still completes with its earlier writes; normal unmount is Busy and leaves
  it alone; after it exits unmount succeeds.

## E. Candidate R6 rows

R6-1 install under slot pressure (H-A); R6-2 Commit against 16 or more
writers on one Workspace (H-B); R6-3 Commit on A while sibling B writes; R6-4
FP-8 and FP-34; R6-5 FP-9 and FP-2 with fusectl; R6-6 finite fair arrivals on
two Workspaces by counts; R6-7 plateau over cycles, live and idle; R6-8
pipe-blocked caller; R6-9 FP-24; R6-10 P-1; R6-11 FP-21 stages, FP-29,
FP-31; R6-12 FP-27 at mount scope; R6-13 forced teardown rows FP-19-FS,
FP-22-FS, FP-23-FS, FP-32, FP-33; FP-16 fixture and FP-15 header identity
stay unrun unless a real-resource hold is found.
