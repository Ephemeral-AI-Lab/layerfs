# R6 completion: concurrency, forced teardown and failure scope

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

R6 makes a mounted Workspace hold up under concurrent callers and Commits, adds
the forced terminal unmount of one Workspace, and scopes a failed cold read to
the request that made it. This record states what is wired, each proof row with
its verdict, every failed attempt, the decisions that need owner review, how
subagents were used and the source-size comparison for each commit. Design is
in architecture [80](../../architecture/80-forced-teardown.md),
[75](../../architecture/75-native-request-service.md) and
[21](../../architecture/21-daemon-owner.md); the plan, its lead rulings and
Amendment 1 are in the
[deepest-file plan](checks/r6-concurrency-teardown-20261009/01-deepest-file-plan.md).

- Start: `66e4e7ab1` (R5 record). Final product source identity: `2b4dc28a6`.
  The commit that adds this record changes documentation and receipts only.
- Local `main`, one checkout, nothing pushed, no pull request.
- Global Store profile: Disposable / WAL / synchronous=OFF only. Durable:
  `NOT_RUN — disabled by owner until explicit reauthorization`.
- Evidence is functional results and counted work. No timing, cold-cache,
  storage or resident-memory claim is made anywhere in R6.
- Four product defects were fixed inside the stage: one found by the audit
  (install deadlock on owner slots), one by a proof track (a failed cold read
  fenced its whole mount) and two by the closing reviews (a Commit reply that
  could not be encoded; the first failed-demand cause being overwritten).
- **Owner direction 2026-10-09: stop after R6.** R7, the optimization
  checkpoint, R8 and R9 are not started. The next step is the owner discussion
  of which benchmarks to run and the target of each.

## What is wired

Production LOC from the pinned `tools/production_loc.py --files` at
`2b4dc28a6`. Only the files R6 added or changed are listed, so every bracket
covers the listed files only. Tests live under each crate's `tests/`.

```
layerfs-daemon/src/                 [2857]  (part)
  control/                          [1597]  (part)
    attach.rs                         217   (abort control bound at Attach is reported)
    detach.rs                         164   (take with a phase; retain with forced facts and the leaving observer)
    failure.rs                        242   (unresolved Commit answers Unknown, without a moved-head state)
    force.rs                          317   (new: guard, admission, abort, drain, Revoke, Close, observation)
    mod.rs                             20   (declarations)
    operations.rs                     226   (ForceUnmount routed)
    serving.rs                        163   (Kept carries forced facts)
    status.rs                          93   (observation copied; its completion dropped before the reply)
    unmount.rs                        155   (Close split into closed and depart)
  overlay/                           [834]  (part)
    owner.rs                          466   (Source class admitted against its own counter)
    queue.rs                          368   (per-lane Source counter)
  service/                           [426]  (part)
    filesystem_port.rs                426   (gated acquiring calls; request-scoped base demand classification)
```

```
layerfs-fuse/src/                   [3983]  (part)
  ports.rs                            215   (Fence, Fenced, BaseDemandFailed, FailedDemands)
  dispatch/                          [432]  (part)
    admission.rs                      272   (fence, stop_service)
    queue.rs                          160   (parked tasks requeued at a stop)
  mount/                             [142]  (part)
    mod.rs                              5   (declarations)
    syscalls.rs                       137   (abort control binding; the one abort write)
  operations/                       [1006]  (part)
    directory.rs                      357   (fenced, base_demand, relinquish)
    lookup.rs                         231   (same)
    mutation.rs                       339   (same; never releases a publication)
    read.rs                            79   (same)
  request/                          [1600]  (part)
    callbacks.rs                      658   (terminal dispositions)
    directory.rs                      237   (same)
    inline.rs                          73   (same)
    mod.rs                             12   (declarations)
    mutate.rs                         230   (same)
    reply.rs                          159   (same)
    state.rs                          176   (same)
    terminal.rs                        55   (new: the one ENOTCONN or EIO reply, then release)
  session/                           [588]  (part)
    drain.rs                          119   (quiesce and release split; receipts carry failed demands)
    force.rs                           51   (new: abort, force_drain)
    mod.rs                              9   (declarations)
    startup.rs                        163   (abort control bound at mount)
    state.rs                          246   (DetachAttempt, ForceRefusal, Forced, receipts)
```

```
layerfs-bridge/src/                 [1134]  (part)
  control.rs                          148   (reexports)
  control_forced.rs                   119   (new: ForcedFacts, ForcedOutcome, ForceUnmounted)
  control_native.rs                   294   (NativePhase::Stopping, TeardownStage::Abort, custody with forced facts)
  control_reply.rs                    294   (reply tags 14 and 15)
  control_request.rs                  151   (request tag 11)
  control_types.rs                    110   (CommitKnowledge)
  lib.rs                               18   (declarations)
```

```
layerfs-api/sdk/src/                 [217]  (part)
  control/                           [191]  (part)
    connection.rs                     191   (pairs ForceUnmounted and forced Retained with their call)
  workspace/                          [26]  (part)
    unmount.rs                         26   (WorkspaceApi::force_unmount)
```

What R6 added to the product, in the order a caller meets it:

1. **Owner slots.** A lane has three counters: 16 ordinary, 16 Source and 2
   Lifecycle. Source acquisitions parked behind a queued install no longer use
   the ordinary slots their predecessors need, which removes the install
   deadlock the audit found.
2. **Failed cold read.** A canonical base demand that fails ends its own
   request with one `EIO`, releases what the request holds and leaves the
   mount serving. The mount keeps a count with the first and the latest
   original cause; the connection's drain receipt carries it.
3. **`ForceUnmount { token, relinquish_unknown }`.** One registry step refuses
   before any effect or admits. Admitted, each step is made once: one abort
   write on the connection's own control, the terminal fence, the local drain,
   one plain `umount2`, Revoke, Close, one cleanup observation. A step that is
   not established stores the exact owner as `Retained` with the forced facts.
4. **Wire, additive.** Request tag 11, reply tags 14 and 15, phase value 7,
   stage value 9. Tags 1 to 13 keep their bytes for existing values.

Not wired by R6: any exit from `Retained`; forced teardown in the Sandbox
topology (no abort control is bound there, so every Force is refused);
`mount:debt` refusal and low-water serial refill; a daemon-wide drain.

## Proof rows

Verdicts are for the scope stated. A partial-scope receipt does not close a
whole row. Receipts are in
[`checks/r6-concurrency-teardown-20261009/`](checks/r6-concurrency-teardown-20261009/).
The `Z-attempt1-linux-*` receipts are the evidence receipts: one `--nocapture`
run of each R6 proof binary at the final identity. Numbers quoted are from
them unless a receipt is named. Mounted rows ran only in the pinned Linux
image, with Store, Overlay and scratch on the container's own filesystem.

| Row | Verdict | Scope and basis |
| --- | --- | --- |
| R6-1 | PASS at owner scope; mounted "no stall observed" | Owner scope: with one source held, an install queued and 15 Source jobs parked, the holder's next job is admitted and completes; a 17th Source job is `AdmissionFull`; after release the install and all 15 sources complete on the new root. Reproduced as a deadlock on the unchanged source first (`A-attempt1`). Mounted: 24 external processes read and write through three Commits; every Commit returns, every process exits 0, `admitted` never exceeds 16 and `received` 2. The mounted run does not show the exact interleaving. `install_slots`, `mounted_concurrency` |
| R6-2 | PASS | A Commit against 20 external writers is not refused. Its root holds an exact prefix of each writer's sequence (116 of 841 files, 5 or 6 per writer); the mount shows all 841; the next Commit holds the other 725; a fresh mount is equal over 933 paths. While it ran the owner admitted 30,327 jobs, 7,160 of them filesystem-only. `mounted_concurrency` |
| R6-3 | PASS | Sibling B's `completed` rises from 393 to 3,251 across A's Commit; A's root holds none of B's names and B's view none of A's; B's later Commit is exact. `mounted_concurrency` |
| R6-6 | PASS, counts only | Eight processes writing 400 blocks of 128 KiB on A and two processes making 200 operations on B both finish; every owner class advanced; nothing retained; 52,428,800 bytes exact. No share ratio is asserted: none is declared. `mounted_concurrency` |
| FP-8 | PASS for the two writes; one recorded limit | With four cold readers parked behind held Store leases, an append on the same mount and an append on a sibling both complete. Limit: a READ of bytes written wholly through the mount also parks, because every READ window takes a Store reader (`a_read_of_locally_written_bytes_still_waits_for_a_store_reader`). The plan had assumed it would not. `mounted_parking` |
| FP-34 | PASS, both halves | Admission: with 20 cold readers, 193 observations never exceed 16 admitted and 2 received; a sibling progresses. Terminal: on Force, 16 fenced replies and 2 receive units end, all 20 readers return `ECONNABORTED` by their own status and none is signalled. The one detach then either succeeds or meets `EBUSY`; see decision 2. `mounted_parking`, `forced_unmount` |
| FP-9 | PASS | Own connection's `waiting` goes 0, 1, 2 and stays 2 over 100 observations while the mount handed off one READ; an append exits 0 meanwhile; both readers return 614,523 exact bytes after release. "One READ" is one handoff: status has no per-opcode count while mounted. `mounted_fusectl` |
| FP-2 fusectl half | PASS, named part | `abort_bound` is true; own `max_background` and `congestion_threshold` equal each Ready receipt, on two mounts. Observed maximum request size: NOT_RUN. `mounted_fusectl` |
| R6-7 / FP-26 | PASS, plateau half | Twelve mount, write, Commit, unmount cycles beside an anchor, once idle and once beside a running sibling writer: every `StoredCounts` field after cycle 2 equals cycle 1, closed namespaces equal unmounted Workspaces, database pages stay 77 with 8 free, no maintenance failure, 0 deviations. Refusal half (`mount:debt`): NOT_RUN, not implemented. `mounted_cycles` |
| R6-8 | PASS, filesystem side | A process blocked on a full stdout pipe with a descriptor, cwd and mapping in the mount: Commit includes its earlier writes, Unmount is `Busy` at `unmount:kernel`, the process is alive and unsignalled, and after the pipe is read it exits 0 and Unmount drains. Stream delivery is the runtime's row. `mounted_cycles` |
| FP-24 | PASS | A Branch, a fork and a committed descendant mounted together show only their own bytes, on the mount and through a fresh bind. A token of an unmounted Workspace is `Missing`; a wrong namespace is `Invalid` and moves no epoch. `mounted_cycles` |
| FP-27 | PASS after a product fix | With one Store reader quarantined, the observing read is `EIO`, eight later cold reads are exact, the quarantined reader's statement count stays 82, nothing is retained and both Unmounts drain. Failed on the unfixed source (`P2-attempt1`, `attempt2`). The marker is proven at a real mount for the lookup and open step only. `mounted_failure_scope`, `cold_failure_scope` |
| P-1 | PASS, exhaustion half | `touch` with no local serial range under a held Store writer is `EAGAIN`, the name is absent, one reservation was attempted and none succeeded; after release a later `touch` succeeds. Low-water early refill: NOT_RUN, not implemented. `mounted_failure_scope` |
| FP-21 | PASS, owner-completion half | A RELEASE parked behind both held Lifecycle credits keeps a normal Unmount `Draining`; credits returned inside the window give `Unmounted`, cleanup `Queued` then `Gone`, counts back to baseline. Not returned: `Retained` at `Requests`, engine mount `Live`. A Store consumer held by the test: not staged. `mounted_drain` |
| FP-29 | PASS, remaining part | A READ parked on reader admission spans a sibling RELEASE and the removal of its name; after release it returns 70,000 exact bytes, forced `statx` link count 0, same inode, name `ENOENT`. The file was first made local to the mount so the unlink asks the base for nothing. `mounted_drain` |
| FP-31 | PASS, mounted halves | Partial: lookup count 1, 2, 1 (row kept), 0 (row returned). Batched: 2,022 names over 2,021 inodes; 1,969 units equal the engine decrement; after a detach with 1,011 lookups outstanding, retirement runs in turns of at most 64 rows with no full scan. Foreign incarnation and underflow stay at component scope. `mounted_drain` |
| FP-17 | PASS, whole row | `native_mount` 3 of 3 and `native_mutation` 2 of 2 in the Linux final suite at `2b4dc28a6`. |
| FP-33 | PASS for the outcomes staged | Clean: `ForceUnmounted` with abort `Written` and detach `Detached`; mount row and own connection entry gone; token `Missing`. Held cwd or descriptor: `Retained` at `Detach` with `Written` and `Busy`; the row remains; the holder is alive and its next call is `ENOTCONN`; a later Force, Unmount and Attach return equal custody and change nothing. "One abort, one detach" is the product's receipt and single call sites; no syscall trace exists. `forced_smoke`, `forced_unmount` |
| FP-32 | PASS except one unstaged case | Force is `Busy` at `force:admission` with no effect while a Commit is held before Save finish, before publication and before install; each Commit then ends with its own outcome. Unknown custody without the flag: `Unknown` at `force:custody`. With it: `ForceUnmounted`, `commit: Unknown`, cleanup `Held`. Known publication: `commit: Published` equal to the registry's. Held ticket: `Retained` at `Requests`, no detach. NOT_RUN: a cold-read consumer held inside a provider read. `forced_producers` |
| FP-19-FS | PASS | A cold reader parked before its demand returns `ECONNABORTED` by its own status, unsignalled; `fenced` is 1; `ForceUnmounted` returns while the test still holds the leases. `forced_unmount` |
| FP-22-FS | PARTIAL by design | A holder in A never holds B, and Force on A kills nothing. Proven in the test container's one mount namespace, not in the Sandbox topology, where Force is refused. `forced_unmount` |
| FP-23-FS | PASS | Union of FP-32 and FP-33. |
| capability | PASS | With no abort control bound, Force is `Failed` at `force:capability`, the Workspace stays Ready, a read works and normal Unmount succeeds. `forced_unavailable` |
| FP-15 header identity | PARTIAL, unchanged from R3 | No accounting of flagged WRITE headers exists and none was added. |
| FP-16 fixture half | NOT_RUN | No real-resource hold exists after the attribute is sampled. |

### Final suites at `2b4dc28a6`

One run per test binary, each under its own 100 s limit, all 13 active
packages, `LAYERFS_CONSTRUCTION_WORKERS=1`.

| Side | Binaries | Exit 0 | Not zero |
| --- | --- | --- | --- |
| Host macOS arm64, `cargo +1.85.1 --locked` | 232 | 230 | `complete_installed_roots`, `host_handoff` |
| Linux, pinned image | 232 | 230 | `complete_installed_roots`, `shared_processes` |

The three non-zero binaries are the same unsupplied-precondition cases as in
R4 and R5: each needs an installed Store or a second process the suite does
not supply. They are failures of the run, not passes, and their full output
is kept beside the summary. R6 added 16 binaries over R5's 216.

Also at this identity: Clippy `-D warnings` on all 13 packages, host and
Linux, clean; `fmt --check` clean; the boundary guard passes over 850
production files; 49 tooling self-tests pass
(`final-tooling-selftests.txt`); the fuser provenance check verifies the
authorized patch.

## Every failed attempt

All receipts are kept under their original names.

| Receipt | What failed | Cause |
| --- | --- | --- |
| `A-attempt1-install_slots` | The install deadlock | Intended reproduction on the unchanged source |
| `A-attempt2-install_slots` | Same test after the fix | The test shadowed its own held handle |
| `P1-attempt1`, `attempt2-linux-mounted_parking` | FP-8 local read | The plan's assumption was wrong: every READ takes a Store reader. Recorded as a limit, not relabelled |
| `P2-attempt1`, `attempt2-linux-mounted_failure_scope` | FP-27 | Product defect: a failed cold demand fenced its mount. Fixed in `9f5e75dd3` |
| `U-attempt5`, `attempt6-cold_failure_scope` | Port-scope proof of that fix | The test's own teardown expectation; attempt 6 also ran a stale binary |
| `U-attempt8-linux-mounted_failure_scope` | FP-27 sibling mount | Instrument assumption: the sibling's objects were already in the shared cache, so its demand is a length demand |
| `P3-attempt1-linux-mounted_drain` | 3 of 5 | Two wrong test expectations and one 20 s reclaim bound that expired without printing its counters |
| `P4-attempt1-linux-forced_unmount` | 1 of 5 | The test's own staging |
| `R1-attempt1-native_control` | No test ran | The lead passed the wrong binary name |
| `R1-attempt2-native_control` | Moved-head reply | Intended reproduction: the reply could not be encoded (`control refusal fields`) and the channel closed. Fixed in `2b4dc28a6` |

No test reached its wall limit in R6.

### Corrections of the lead's own statements

- The plan said a READ of locally written bytes needs no Store reader. Source
  takes a read ticket for every READ window.
- Commit `9f5e75dd3` classified every non-History port failure as a failed
  base demand, including a poisoned lock, the admission table bound and a
  foreign reader. The Fuse review found this; `2b4dc28a6` narrows it to a
  provider read failure and to `NoReaders` or `Stopped` admission.
- The same commit kept only the most recent failed-demand cause and gave it a
  reader no product code used. Both are corrected in `2b4dc28a6`.
- Architecture 80 said a refusal changes nothing. An `Unknown`-coded refusal
  still ends the caller's control connection; the document now says so.
- The R5 block that rewrites a Commit reply to `Unknown` had the same
  moved-head flaw as the R6 block. Both are fixed together.

### Not verified

- The failed-demand marker at a real mount beyond the lookup and open step,
  and a failed release after a failed demand.
- That a poisoned lock, `Capacity`, `InvalidLimits`, a concurrent demand and a
  foreign reader stay `Retained`: classified by source, not staged.
- The moved-head reply with a real moved head. The proof returns the error
  from a constructor closure at control scope; a stale candidate over a moved
  head is otherwise Committed (R5-2), so no product route was found.
- The Close-stage custody now reading the leaving observer: no hook-free stop
  at `Revoke` or `Close` after a detach exists, so no test reaches it.
- Lifecycle credits returned inside the drain window of a forced teardown
  (FP-21 proves the normal path only), and an idle mount stopping at `Revoke`.
- A short or failed abort write, and an abort refused after admission.
- Two Status jobs in flight at the instant of a Revoke or Close.
- "One abort, one detach" by an independent syscall trace.

## Decisions that need owner review

Each was taken as the conservative reversible option. The plan's table has
the alternative for D1 to D18.

1. **Sandbox topology is unchanged (D4).** The Sandbox container does not
   mount the fusectl filesystem, so no abort control is bound and every Force
   is refused at `force:capability`. Forced teardown therefore works only
   where the daemon's environment mounts it. Mounting it in the Sandbox is a
   topology change and exposes other containers' connections to the daemon's
   namespace; that is the owner's call.
2. **`EBUSY` at the one detach with many blocked callers.** With 20 callers
   blocked in the mount, the one plain `umount2` met `EBUSY` in two of four
   runs (`P4-attempt2` and `attempt3`; it detached in `P4-attempt1` and in the
   final evidence run `Z-attempt1`). The abort had released the callers but
   they had not all left the kernel. When that happens the Workspace is
   aborted, still mounted and `Retained`, with no product exit, although every
   caller returned. With one caller the detach succeeded every time. The test
   accepts either outcome and asserts the facts of the one it got. This is
   inside the one-detach contract. The owner may want a bounded wait before
   the one detach, or a different contract.
3. **No way out of `Retained` (D2).** A retained request, a held reference or
   a refused Revoke or Close leaves the entry `Retained` until the daemon
   stops. No relinquish operation exists.
4. **Force only from Ready (D10).** An `Unattached` Workspace with unknown
   Commit custody has no terminal exit: Unmount is `Unknown` and Force is
   `Invalid`.
5. **`relinquish_unknown` leaves the engine capture `Held` (D11).** The
   namespace is closed and its rows are not reclaimed in that daemon.
6. **Forced Revoke and Close do not wait for a Lifecycle slot (D15).** A
   retained Commit failure that keeps both slots makes Force end `Retained` at
   `Revoke` or `Close`, after the abort and the detach.
7. **Status no longer holds its slot until the reply (review fix).** A small
   change to control behaviour from R3. A Status job in flight can still
   refuse a Revoke or Close.
8. **`Unknown`-coded refusals park the caller's connection.** Force without
   the flag on an uncertain Workspace is refused `Unknown`, and the
   application then closes that control connection and keeps its slot, as for
   every `Unknown` answer since R3. The refusal carries no custody. Repeating
   it `limits.connections` times exhausts control. Not changed: it is the
   uniform existing policy. The reviewer's fix is to stop treating a refusal
   with no completions as uncertain.
9. **A failed cold read is request-scoped (product fix).** Taken from the
   specification's wording. Which causes count is the lead's reading: a
   provider Storage failure, and `NoReaders` or `Stopped` admission.
10. **Every READ takes a Store reader.** A read of bytes that are wholly local
    waits behind cold readers when all readers are leased. A candidate for the
    optimization checkpoint.
11. **Status under load mostly has no engine fields.** During the R6-2 Commit,
    297 of 310 Status samples had the engine observation refused, because it
    does not wait for a Lifecycle slot.
12. **Admission waits are unordered and unbounded.** A Commit job waits for a
    credit among filesystem requests with no priority; 30,327 owner jobs were
    admitted during one Commit.
13. **Wire additions (D3, D7, D8).** A peer older than R6 refuses phase 7 and
    stage 9 and does not know tags 11, 14 and 15.
14. **`ENOTCONN` is the terminal reply errno (D17).** Callers observed
    `ECONNABORTED` from the kernel's own abort or `ENOTCONN`.
15. **The normal unmount gets no terminal fence (D12); the forced drain runs
    before the one detach (D13); a short or failed abort attempts nothing
    further (D14).**
16. **`mount:debt` and low-water refill are not built (D18).** They have no
    value in source and no hook-free proof.
17. **The clean-case cleanup value is not fixed.** One observation reads
    `Held`, `Queued` or `Gone` depending on the maintenance turns before it.
18. **A benchmark bound needs revisiting before R8.**
    `core/benchmark/fs-bench-pro/shared/evidence_engine.py:104` bounds
    `peak_queued` by the old per-lane ceiling of 18; a lane now admits 34.
    Not edited.
19. **RELEASEDIR under a stopped fence** replies `ENOTCONN` and leaves its
    handle row to revocation, while RELEASE still closes its file.

## How subagents were used

One checkout and no worktree throughout. Only the lead ran git, counted LOC,
edited the ledger and this record, and took decisions.

- **Audit.** Two read-only audits while R5's proofs ran produced
  [`00-audit-findings.md`](checks/r6-concurrency-teardown-20261009/00-audit-findings.md).
  The lead verified H-A and H-B against source before planning.
- **Plan.** One planning subagent drafted the deepest-file plan; the lead
  ruled on its open points (section 10) and committed it before any product
  edit. Design checks for tracks U and W led to Amendment 1.
- **Implementation in disjoint tracks.** A (owner slots), W (wire and SDK),
  U (Fuse session, fence and ports), D (daemon control). Each had an explicit
  file list. The lead read `force.rs` whole after track D reported that it had
  overwritten four of its own files with lock output and rebuilt them.
- **Proof tracks.** P1 (concurrency and parking), P2 (cycles, isolation,
  failure scope), P3 (fusectl, drain, FORGET), P4 (forced rows). P2 found the
  FP-27 product defect and stopped; the fix was a separate track.
- **Review.** Two fresh-context read-only reviewers at the end, one for the
  daemon control and wire, one for the Fuse fence and failure scope. Neither
  built or ran anything.

Review findings and what was done:

| Finding | Action |
| --- | --- |
| Daemon 1: unencodable moved-head reply | Reproduced and fixed |
| Daemon 2: Status holds a Lifecycle slot past its answer | Fixed; the in-flight window is recorded |
| Daemon 3: `Unknown` refusals park the connection | Not changed; decision 8 |
| Daemon 4: Close-stage custody contradicts itself | Fixed in `retain`; unproven by a test |
| Daemon 5: `InFlight` now answers `Unknown` | Not changed: unreachable through control admission, and the registry already records it uncertain |
| Daemon 6: document corrections | Applied to architecture 80 |
| Fuse 1: first cause overwritten; reader used only by a test | Fixed; the drain receipt reads it |
| Fuse 2: admission failures misclassified | Fixed by narrowing |
| Fuse 3: fence check and submission share no lock | Documented in architecture 75; nothing is lost, so no code change |
| Fuse 4: `failed_base_read` could drop the step's error | Fixed |
| Fuse 5: count taken before release | Document corrected |
| Fuse 6: directory relinquish clears its read token before the awaited release | Not changed: it copies `dispose`, which has done so since R2 |
| P3: custody `work` is read live though the type's comment said otherwise | Comment and architecture 80 corrected; behaviour kept |

Both reviewers listed what they checked and found correct: the before-effect
guard under one registry lock, single effects, Commit knowledge, bounds, wire
byte compatibility, custody on both `Complete` paths, `stop_service`, the
forced session and slot accounting.

## Production LOC per commit

Pinned `tools/production_loc.py` (SHA-256 `c0fe7f36…24adb`), first parent
against the committed tree; first-party product Rust and shipped SQL;
comments, blanks, tests, docs and tools excluded. The excluded predecessors
(38,878), excluded integration (3,996) and root reference (65,417) did not
change in R6.

| Commit | Subject | Before | After | Delta | Active core after |
| --- | --- | --- | --- | --- | --- |
| `d5e9312b0` | Plan | 183,237 | 183,237 | 0 | 74,946 |
| `f7eafddeb` | Track A: Source slots | 183,237 | 183,238 | +1 | 74,947 |
| `803d05b46` | Amendment 1 | 183,238 | 183,238 | 0 | 74,947 |
| `aef00b084` | Track W: wire and SDK | 183,238 | 183,428 | +190 | 75,137 |
| `057d443a2` | Track U: Fuse session and fence | 183,428 | 183,831 | +403 | 75,540 |
| `36f72ebd7` | Track D: daemon ForceUnmount | 183,831 | 184,191 | +360 | 75,900 |
| `e60d134dd` | Track P2, part | 184,191 | 184,191 | 0 | 75,900 |
| `d76b92d07` | Track P1 | 184,191 | 184,191 | 0 | 75,900 |
| `236a173cc` | Ledger: owner hold | 184,191 | 184,191 | 0 | 75,900 |
| `9f5e75dd3` | Fix: request-scoped cold failure | 184,191 | 184,274 | +83 | 75,983 |
| `067d226d2` | Track P4 | 184,274 | 184,274 | 0 | 75,983 |
| `8b6c287fd` | Track P3 | 184,274 | 184,274 | 0 | 75,983 |
| `2b4dc28a6` | Review fixes | 184,274 | 184,297 | +23 | 76,006 |

The commit that adds this record, the final-suite receipts and the evidence
receipts changes no product source: 184,297 to 184,297, delta 0.

R6 total: 183,237 to 184,297, delta +1,060, all in active core (74,946 to
76,006). Core as a whole is 118,880. This is new product code, not a
relocation; nothing was retired.

## Not claimed

- No latency, throughput, cold-cache, storage or resident-memory result.
- No Durable execution of any kind.
- No qualification of S8 or S10, no fairness share, and no forced teardown in
  the Sandbox topology.
- No proof beyond the scope stated in each row; the NOT_RUN and PARTIAL rows
  above stay open.
- R7 (retiring the excluded predecessors), the optimization checkpoint, R8 and
  R9 are not started.
