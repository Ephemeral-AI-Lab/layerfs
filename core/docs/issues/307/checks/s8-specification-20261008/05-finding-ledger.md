# S8 specification: disposition of review findings

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Written 2026-10-08 by the specification owner on local `main` at `32d969776`.
> It records what was done with every finding of the three authorized read-only
> reviews. The [S8 specification](../../S8-SPECIFICATION-20261008.md) owns the
> decisions; this file only says where each finding went and why.

Reviews, retained verbatim: [kernel/FUSE](02-review-kernel-fuse.md) (`K`),
[cache, concurrency and lifecycle](03-review-cache-lifecycle.md) (`C`),
[historical evidence and proof](04-review-evidence-proof.md) (`E`). Identifiers
below are the reviewers' own section letters and numbers with that prefix.

Dispositions: **Accepted** (now in a document, cited), **Rejected** (with the
reason), **Deferred** (a ranked candidate behind a gate, S10, or an owner
choice). The reviewers ran nothing; neither did the owner of this ledger. A
kernel statement the reviewer marked as not line-verified stays a requirement
with a native oracle, never a fact.

Spot checks by the specification owner against source and originals before
accepting: the fuser `INTERRUPT` reply, receive-buffer size and unmount handle
in `core/vendor/fuser-0.18.0/src/{request,read_buf,session,mnt/mod}.rs`; the
blind reader counter in `store/open.rs`; `Pending::wait` in
`service/completion.rs`; `Entry::Binding` handling in `control/operations.rs`;
the `Held` close path in `lifetime/close.rs` and the reclaim phases in
`maintenance/reclaim.rs`; the Persistence `try_lock`; the Stage B receipt
(`status FAILED`, cause "separate verification exceeded 10 s", exit code 0); and
the fixture totals and manifest hash. Everything else is accepted on the
reviewer's citation and labelled accordingly in the specification.

## 1. Kernel/FUSE review

### 1.1 Findings (c)

| ID | Finding | Disposition | Where, or why |
| --- | --- | --- | --- |
| K-c1, K-c2 | `Session::new` mounts and blocks in the handshake; fuser's mount falls back to a setuid helper silently | Accepted | D-2: first-party `mount(2)` and `Session::from_fd` |
| K-c3 | fuser's unmount handle is consumed on first `EBUSY`, later reports success, and may detach lazily | Accepted | D-2; §11.2 uses `umount2` directly |
| K-c4 | `run()` spawns `n_threads` loops and joins them | Accepted | §6.1, mount session owner |
| K-c5 | 16 MiB + 4 KiB zeroed receive buffer per loop; residency unverified | Accepted | §7.4 per-mount native domain; H-3, H-15. Sizing buffers to negotiation is unavailable in the pinned library and is not patched (§13) |
| K-c6 | Reply types are `Send`; a dropped reply becomes `EIO` | Accepted | I-7: no path relies on drop |
| K-c7 | `INTERRUPT` answered `ENOSYS`; a task in a parked request cannot be killed until the reply | Accepted | I-7, §5.2, §10.4; FP-19 |
| K-c8 | FORGET forms carry no reply; `batch_forget` loops | Accepted | D-6: FORGET is a counter update |
| K-c9 | Notifier surface | Rejected for S8 | D-14: no notifications. See K-c20 |
| K-c10 | Negotiation surface and limits | Accepted | §8.1; FP-2 |
| K-c11 | The patch changes only timestamp decode | Accepted | §1 scope; no further third-party change |
| K-c12 | Per-element behaviour of the candidate profile | Accepted | §8.1, §8.3 |
| K-c13 | A short READ shrinks `i_size` | Accepted | I-10 |
| K-c14 | Hard-link aliases share one kernel inode | Accepted | §8.3; FP-14 |
| K-c15 | Stale attribute replies are discarded by inode version | Accepted | I-6, §5.2; FP-16 |
| K-c16 | An `ENOENT` error is an uncached negative dentry | Accepted | §8.3; cached negatives are a ranked candidate |
| K-c17 | Truncate and `O_TRUNC` behaviour without `ATOMIC_O_TRUNC` | Accepted | §8.3; FP-13 |
| K-c18 | Mapped WRITE: per-request flag, no credentials, first writable handle, clipped to size | Accepted | §8.3, M-7; FP-15 |
| K-c19 | `O_APPEND` offset is resolved by the kernel; the active write accepts `Position::End` | Accepted | I-10, §8.3: the kernel's offset always |
| K-c20 | `inval_inode` under cached I/O self-deadlocks with an in-flight partial-page WRITE | Accepted | I-9, D-14, §2 reconciliation |
| K-c21 | Readiness: `mount(2)` queues INIT; early requests queue | Accepted | I-3; FP-1 |
| K-c22 | No DESTROY at unmount; loops see `ENODEV` | Accepted | §11.2, §2 reconciliation; FP-21 |
| K-c23 | No FORGET after the superblock is inactive; abort ends queued RELEASE and mapped WRITE | Accepted | D-8 group retirement; FP-21 |
| K-c24 | Mount-namespace copies can keep the connection alive | Accepted as a requirement, not a fact | D-12, §10.3; FP-22 is in the first checkpoint because the reviewer did not line-verify propagation |
| K-c25 | Each mount has a new connection, superblock and `st_dev`; `st_ino` is the server's | Accepted | §9; FP-4 |
| K-c26 | With writeback off the daemon owns times; no `FATTR_CTIME` | Accepted | §2 reconciliation, §9; FP-15, FP-17 |
| K-c27 | The retained fractional-minimum timestamp FAIL is independent of the profile | Accepted | Kept as a platform limitation; not rerun |
| K-c28, K-c29, K-c30 | Kernel access rule, `SessionACL`, fusectl ownership | Accepted | D-11, §10.2; FP-5 |
| K-c31 | Statements in #303 the sources contradict | Accepted | §2 reconciliation table |

### 1.2 Hazards (d)

| ID | Hazard | Disposition | Where, or why |
| --- | --- | --- | --- |
| K-d1 | Background head-of-line with `max_background` 1 | Accepted | §8.2; FP-9, H-9. Raising the depth is a ranked candidate and owner choice P-6, not a silent profile change |
| K-d2 | Unkillable process while a reply is parked | Accepted | I-7; FP-19 |
| K-d3 | Commit misses mapped stores still in the kernel | Accepted as a stated boundary | §5.2, §8.3: capture is the published frontier. A drain fence is K-e15, deferred |
| K-d4 | Guessed append handle on a mapped WRITE | Accepted | §8.3; FP-15 |
| K-d5 | Past-EOF bytes in the last mapped page | Accepted | FP-15 compares cache and daemon after an extension |
| K-d6 | Stale-view size reply truncates the cache | Accepted | I-6; FP-16 |
| K-d7 | Notification deadlock | Accepted | D-14 |
| K-d8 | fuser unmount trap | Accepted | D-2 |
| K-d9 | A reader may see a page of a write the daemon then rejects, until the reply | Accepted as a stated boundary | §8.3 write row; no mechanism removes it without writeback changes that are rejected |

### 1.3 Recommendations (e)

| ID | Recommendation | Disposition | Where, or why |
| --- | --- | --- | --- |
| K-e1 to K-e3 | First-party mount and unmount; readiness definition; exact negotiation | Accepted | D-2, I-3, §8.1 |
| K-e4 | Never park a background request on a guard a parked request holds, or raise the depth | Accepted in the stronger form | I-8/I-9: no request parks on another request at all. The depth stays a candidate (P-6) |
| K-e5 | Explicit reply in daemon-owned time; replies before waiting for exit | Accepted | I-7, §5.2, §11.2 |
| K-e6, K-e7 | Exact READ and WRITE replies; mapped WRITE handling | Accepted | I-10, §8.3 |
| K-e8 | Attribute replies from the published frontier; lookup counts incremented at the reply attempt | Accepted in part | I-6. The count half is superseded by D-6: there is no per-inode lookup count to increment |
| K-e9 | No notifier; refuse non-FUSE view changes; install must be proved identical or refused | Accepted | I-4, D-14, §14 (install is S10) |
| K-e10 | Terminal sequence | Accepted | §11.2 |
| K-e11 | State the propagation contract; slave into the Exec namespace | Accepted | D-12, §10.3 |
| K-e12 | `SessionACL::All`; xattr family `ENOSYS` | Accepted | D-11, D-14, §8.4; FP-18, H-18 |
| K-e13 | State the mode boundary, the `fsync` no-op and unsupported locks | Accepted | §6.4, §8.4, §13 |
| K-e14 | PARALLEL_DIROPS, EXPLICIT_INVAL_DATA, CACHE_SYMLINKS | Deferred | Lower-ranked candidates, one mechanism per arm. PARALLEL_DIROPS changes the locking FP-12 relies on |
| K-e15 | fusectl `waiting == 0` drain fence for Commit; `max_background` read-back | Deferred / Accepted | The fence is not selected: it would make Commit wait on kernel state and is not required by the owner's frontier model. The read-back is in FP-2 |
| K-e16 | `statfs` with nonzero free space | Accepted | §6.4 STATFS row: fixed declared values, no capacity claim |
| K-e17 | Forced teardown through `MNT_FORCE` or fusectl abort | Accepted | §11.2 forced path; FP-23 |

### 1.4 Oracles (f) and owner decisions (g)

| ID | Item | Disposition | Where, or why |
| --- | --- | --- | --- |
| K-f1 to K-f12 | Negotiation, coherence matrix, alias, reply ordering, mmap, head-of-line and kill-while-parked, readiness, unmount, two mounts, descriptor hygiene, xattr and attribute counts, timestamps | Accepted | FP-2, FP-10 to FP-18, FP-9, FP-19, FP-1, FP-20 to FP-23, FP-4, FP-5, H-17, H-18. The second-mount git read count is H-14 and S10-dependent |
| K-g1 | Access model under O-24 | Decided as engineering | D-11. Making the Bash user the mount owner would let it abort the connection, which contradicts I-13; only one option satisfies the owner's existing requirements |
| K-g2 | Must a Commit after Exec exit include mapped stores of exited processes? | Decided from the owner's model; fence deferred | The root guide already states that Commit captures the published frontier and that Bash exit proves nothing about dirty mappings. §5.2 and §8.3 state the boundary; FP-15 proves exactly that |
| K-g3 | no-new-privileges for Exec | Owner choice | P-2 |
| K-g4 | Uniform Bash identity across daemons | Decided as engineering | D-11: required equal across daemons sharing a Store, because it reaches committed tool state. A deployment that cannot guarantee it is unqualified, not silently accepted |
| K-g5 | Kernel scope | Decided as engineering | §1: specified against the recorded 6.12 line only; another kernel is unqualified |

## 2. Cache, concurrency and lifecycle review

### 2.1 Findings (c) and contradictions

| ID | Finding | Disposition | Where, or why |
| --- | --- | --- | --- |
| C-c1 | What survives a mount cycle; blind reader selection; cache hit and insert under the mutex; unused serials consumed | Accepted | §3, §7.1; mechanism ledger §2. The serial refill is lazy at first create, so read-only per-call mounts consume none |
| C-c1 (Ready) | Extend `Binding` rather than add a registry | Accepted | D-1, §5.1 |
| C-c2 | Root bind cost: one history read, a depth-shaped walk, one `Open` job | Accepted | I-1; H-1, H-2 |
| C-c3 | Copies and locks on the base path; internal `Arc` entries are compatible | Deferred | Ranked candidate 1, gated on H-13. See C-e6 |
| C-c4 | Concurrent identical misses; flight placement, bounds, cancellation, failure | Deferred | Ranked candidate 4. The two binding rules are already contract in §7.3 and D-15 |
| C-c5 | Memory domains | Accepted | §7.4; proof plan §5 |
| C-c6 | Overlay scheduler: fair by job count, not weight; every port blocks; up to five jobs per lookup | Accepted | §6.3 notifier, D-4, §6.5; H-4, H-10, H-12 |
| C-c7 | Cold admission: starvation by a scanner; one global LRU | Accepted / Deferred | R-1 and R-2 are mandatory. Role-segmented admission is a lower-ranked candidate |
| C-c8 | Lifecycle: `Held` forever on unreleased rows; maintenance stops on first error; admission not coupled to debt | Accepted | D-8, D-13, §11.4; FP-21, FP-26, H-16 |
| C-c9 | Identity: serial as inode number; directory link count is not POSIX; three kinds only | Accepted | §9, §8.4 |
| C-c10 | Reuse list | Accepted | Implementation plan §3 reuse rows |
| C-C1 | 06 §4 says the write handle sits behind a mutex; source uses `try_lock` and returns `Busy` | Accepted | §2 reconciliation; D-9; P-1 |
| C-C2 | Unmount `Busy` is required for live Execs and handles; source arbitrates only Commit | Accepted | §11.1 gauges; FP-20 |
| C-C3 | Compound reads must be one consistent owner job | Accepted | D-4 |
| C-C4 | Parked waiters must not hold workers | Accepted | D-3 |
| C-C5 | Bounded aggregate debt and admission are required and absent | Accepted | D-13 |
| C-c11 | Optional refinements | Deferred | Mechanism ledger §4.6 |

### 2.2 Hazards (d)

| ID | Hazard | Disposition | Where, or why |
| --- | --- | --- | --- |
| C-H1 | A mount, or a first create, refused `Busy` by the same daemon's Commit | Accepted / Owner choice | R-4 removes the mount case. The create case is P-1 |
| C-H2 | One failure poisons a shared operation scope | Accepted | R-3; FP-27 |
| C-H3 | A dead reader stays in rotation | Accepted | R-2; FP-27 |
| C-H4 | Unmount with outstanding kernel references | Accepted | D-6 and D-8; H-8, FP-21 |
| C-H5 | Debt outruns reclaim | Accepted | D-13; FP-26 |
| C-H6 | A scan flushes another Workspace's navigation pages | Deferred | Observed through H-11; segmentation is a candidate gated on it |
| C-H7 | Duplicate cold acquisition occupies every reader | Deferred | Counted by H-11; coalescing is candidate 4 |
| C-H8 | A third concurrent mount is refused `Capacity` on the shared lifecycle lane | Accepted in part | §6.2: the lane is sized by an explicit readiness value. A `Mount` that still meets a full lane keeps its exact before-effect `Capacity` result; a control operation is not queued |
| C-H9 | An uncertain `Open` leaves an unobservable placeholder | Accepted | §5.1: `Retained { bind }`, observable through `Locate`; FP-25 |
| C-H10 | A demand exceeding the 32 MiB window fails after fetching and retains nothing | Deferred | Recorded as a visible cost in mechanism ledger §2; H-11 records batch identities and bytes against the window. The daemon never refetches on a request's behalf |

### 2.3 Recommendations (e), obligations (f) and owner decisions (g)

| ID | Item | Disposition | Where, or why |
| --- | --- | --- | --- |
| C-e1 | Per-request `StorePorts` scope | Accepted | R-3 |
| C-e2 | Native state and gauges on `Binding`; `Ready`; unmount gauges | Accepted | D-1, §11.1 |
| C-e3 | Completion and credit notifiers | Accepted | D-3, §6.3 |
| C-e4 | One compound owner job per read-class reply | Accepted | D-4 |
| C-e5 | Counted lookup acquire/release keyed by serial, plus bulk terminal retirement | Accepted in part | Bulk retirement is D-8. The counted per-serial model is rejected: it is state proportional to the visited tree (I-16) and a write per LOOKUP. D-6 states the resulting boundary and FP-29 proves it |
| C-e6 | `Arc` cache entries — reviewer: mandatory | Rejected as mandatory; deferred as candidate 1 | #314 keeps cache-internal changes optional without a demonstrated requirement. No S8 contract fails without it; H-13 supplies the gate |
| C-e7 | Store-level single-flight — reviewer: mandatory under concurrent dispatch | Rejected as mandatory; deferred as candidate 4 | #314 lists miss coalescing as an optional investigation. Correctness does not depend on it; R-1 bounds the interference. H-11 counts duplicates |
| C-e8 | Idle, healthy reader selection | Accepted | R-2 |
| C-e9 | Fair cold-batch admission per Workspace | Accepted | R-1 |
| C-e10 | Mount snapshot through a reader | Accepted | R-4, D-9. Not an owner choice: K30 already forbids the alternative |
| C-e11 | Debt and maintenance state in status; admission on debt | Accepted | D-13, §4.3 |
| C-e12 | Size lifecycle slots, or park on `AdmissionFull` | Accepted, both | §6.2 |
| C-e13 | Deficit-weighted lanes | Deferred | Candidate; enters only if H-12 fails |
| C-e14 | Length-fact cache; role-segmented allowance | Deferred | Candidate 2 and a lower-ranked candidate |
| C-e15 | Synthesis rules for link count, ownership, times, generation; refusal for unsupported kinds | Accepted | §9, §8.4 |
| C-f O1, O7, O9, O10, O11 | Warm mount counts; unmount with `K` lookups; churn plateau; no parked dispatch thread; quarantined reader gets nothing | Accepted | H-2, H-8 with FP-21, FP-26, H-10, FP-27 |
| C-f O2 | Warm LOOKUP: at most two owner jobs | Accepted, tightened | H-4: exactly one read-only job |
| C-f O3, O4 | Single-flight counts and shared failure identity | Deferred | Only if candidate 4 is selected; the rules are D-15 |
| C-f O5 | A peer's scan against a warm path | Accepted as an observation | H-11 |
| C-f O6 | Zero bytes copied under the cache mutex | Deferred | H-13 records the baseline; the zero target belongs to candidate 1 |
| C-f O8 | Service ratio under sustained unequal weights | Accepted | H-12 |
| C-f O12 | Overlapping refill, mount and Commit outcomes | Accepted for mount; owner choice for refill | R-4; P-1 |
| C-g1 | In-process write-handle contention | Reduced to one owner choice | K30 already rules out a gate, and R-4 is engineering. What remains is P-1 |
| C-g2 | Single-flight fate-sharing | Decided as engineering | D-15, binding only if candidate 4 is selected: all Workspaces of a daemon read under one validated Store authority, and provenance stays exact |
| C-g3 | Lookup ownership model; is bulk retirement an authorized engine change | Decided as engineering | D-6, D-8. Retirement is required correction of a path that today leaves a namespace `Held` forever, within the existing schema ownership |
| C-g4 | Normal unmount with live handles or Execs | Already decided by the owner | The unmount contract requires `Busy`; §11.1 implements it |
| C-g5 | Maintenance failure policy | Decided as engineering | D-13: the S6 no-replay rule stays, the stop becomes visible, and new mounts are refused. Continuing silently is the option I-14 and U9 exclude |
| C-g6 | Directory link count and unsupported kinds | Decided as engineering | §9 and §8.4 give the synthesized values and errnos; both are visible in FP-3, FP-4 and FP-18 |

## 3. Historical evidence and proof review

| ID | Item | Disposition | Where, or why |
| --- | --- | --- | --- |
| E-A | Artifacts read, read in part, and unavailable; distinction checks; the stateless-arm headline miscount | Accepted | Proof plan §2; mechanism ledger §6. The miscount is recorded and the historical report is not edited |
| E-B.0 | Shared envelope of #305 records, status rule, timer erratum | Accepted | Proof plan §2.1; mechanism ledger §6 |
| E-B.1, E-B.2 | Contract rows and per-cell values | Accepted as context | Proof plan §2.2. Values stay in the retained review; none is an arm, a target or a threshold |
| E-B.3 | #306 rows | Accepted | Proof plan §2.2: no arm, not a ranking |
| E-B.4 | Main-branch component rows | Accepted | Proof plan §2.2; mechanism ledger §2 |
| E-C | Workload-to-oracle mapping; manifest scope lacks ownership, times, link count and inode identity; no post-unmount oracle exists for the product | Accepted | Proof plan §2.3 |
| E-D | Dispositions to preserve verbatim | Accepted | Mechanism ledger §6 |
| E-E 1–6 | Lifecycle, full-root, affected-state, identity, coherence and confinement proofs | Accepted | FP-1, FP-3, §2.3 oracles, FP-4, FP-10 to FP-18, FP-5 |
| E-E 7 | Survival through Commit and a fresh mount | Deferred | S10 |
| E-H1 to E-H3 | Bounded bind; warm bind zero demands; no database per mount | Accepted | H-1, H-2 |
| E-H4, E-H5 | Opcode counts against a prediction; GETATTR counted with permissions retained | Accepted | H-17 |
| E-H6, E-H7 | Duplicate acquisitions; bytes under the cache mutex | Accepted as recorded baselines | H-11, H-13 |
| E-H8 to E-H10 | Progress while another scans; residency back to baseline; debt to zero | Accepted | H-11, H-12, H-15, H-16 |
| E-H11 | Second-mount `git status` issues no content READ | Deferred | H-14, S10 |
| E-E classes | Three cache classes, never pooled | Accepted | Proof plan §6 |
| E-F | Budget analysis | Accepted as stated conflicts | Proof plan §8. No threshold proposed and no bound changed |
| E-h1 | Timer inflation before v2 | Accepted | Proof plan §2.1, §2.2 |
| E-h2 | Git stat identity | Accepted / Deferred | §9 identity; the committed-index half is S10 |
| E-h3 | Oracle location | Accepted | Proof plan §2.3 |
| E-h4 | Shared VM; a VM-wide cache drop touches protected containers | Owner choice | P-5 |
| E-h5 | Stage B ran on the zero-lifetime profile | Accepted | Proof plan §2.2; mechanism ledger §6 |
| E-h6 | Faster follow-up arms removed permissions | Accepted | Mechanism ledger §4.6 and rejected list |
| E-h7 | A "read" case may rewrite `.git/index` | Accepted | Proof plan §2.3, E04 row |
| E-h8 | Old binaries and the prototype are not reusable artifacts | Accepted | Proof plan §8, controls row; P-4 |
| E-h9 | Headline miscount | Accepted | Mechanism ledger §6 |
| E-h10 | The fixture holds no file above 4 GiB | Accepted | §13: files above 4 GiB stay `NOT_RUN` under the original waiver; FP-3 uses the large declared root below it |
| E-g1 to E-g3 | Full-fixture command scope; long-workload bounds; full-byte or scoped oracle | Owner choice | P-3 |
| E-g4, E-g5 | New matched controls; what "materially better" compares against | Owner choice | P-4 |
| E-g6 | Cache-drop permission and residency mechanism | Owner choice | P-5 |
| E-g7, E-g8 | E09; E18 and E19 before S10 | Owner choice | P-7 |

## 4. Summary

- Accepted into the specification, plan or proof plan: every correctness,
  ownership and lifecycle finding, and every historical disposition.
- Rejected: per-inode counted lookup state (C-e5, in part); notifier use
  (K-c9); the reviewer's "mandatory" label on shared cached bytes and on miss
  coalescing (C-e6, C-e7), which remain ranked candidates.
- Deferred to a gate, to S10 or to the owner: the ranked candidates, the Commit
  drain fence, Commit survival and index fast paths, and choices P-1 to P-7.
- Reviewer "owner decisions" settled as engineering because an existing owner
  requirement leaves one option: K-g1, K-g2, K-g4, K-g5, C-g2, C-g3, C-g5, C-g6.
  C-g4 was already decided by the owner. If the owner disagrees with any of
  these readings, the specification section cited is the place to change it.
