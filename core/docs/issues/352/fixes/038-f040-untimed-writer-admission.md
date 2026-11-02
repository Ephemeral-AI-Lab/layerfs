# #352 · F-040: queued writers wait their turn instead of failing `Busy` at 2 s

> **Status:** Current fix record. Product fix in `layerfs-persistence`.

| | |
| --- | --- |
| Trigger | Range 15 (measured P3 replay) runs parallel Branches whose lanes commit thousands of small files at once. A Commit queued behind other large Commits can wait over 2 s, and today fails `Busy` (audit A8, watch-out S2/S3). Owner rule 2026-10-18: no `Busy`/`Capacity` on a wait or count that is not a resource limit |
| Kind | product: Store writer admission (`layerfs-persistence/src/backend/sqlite/`) |
| Branch | `f040-writer-admission` off `dsh-history-replay` `0609aaeaa` |
| Commits | `c1502c9b3` failing tests; the fix commit (below) |
| Production LOC | 102,819 → 102,798 (delta −21), all in `core` (94,128 → 94,107) |

## Old design

- `admission.rs`: `WINDOW` = 2 s and `WAITERS` = 1,024. `Admission::enter`
  took a deadline; a waiter past it removed its ticket and returned `Busy`.
  The 1,025th waiter returned `Capacity` (audit A11).
- Every call site computed `now + WINDOW`: `Session::run`, `read_mapped`,
  `read_immutable` (`transaction.rs`), `hold_lane` (`lane.rs`) and the
  Linux seal (`seal.rs`).
- Linux `Writer::acquire` polled `flock(LOCK_EX|LOCK_NB)` with 1 ms sleeps
  until the same deadline, then `Busy`; `EINTR` was `Busy`.
- `Busy` reached History as `HistoryError::Busy`. A mount's inode serial
  refill turned it into `Declined::Contended` → `EAGAIN` on `create`.

## New design

- `Admission::enter(kind)` has no deadline. A queued caller blocks on the
  queue's `Condvar` until it is at the front and the turn is free (FIFO,
  unchanged). No count bound: each ticket is one blocked thread, so the
  queue is bounded by the threads the process runs. Tickets use
  `wrapping_add`; they stay unique while fewer than 2^64 wait.
- Linux `Writer::acquire(path)` takes one blocking `flock(LOCK_EX)` after
  the Session's FIFO turn, so at most one thread per Session waits in the
  kernel. No sleep-poll.
- Named exits without the writer, each typed and each leaving the queue
  clean (ticket removed, turn released, later waiters runnable):

  | Exit | When | `BackendError` | Storage | History |
  | --- | --- | --- | --- | --- |
  | Store closing | `Handles::close()` before the waiter's turn; wakes every waiter at once | `Closed` (new) | `Refused { status: "Closed" }` | `OwnershipUnavailable` |
  | Cancelled | a signal interrupts the kernel `flock` wait (`EINTR`) | `Cancelled` (new) | `Refused { status: "Cancelled" }` | `OwnershipUnavailable` |
  | Self-entry | the thread already holds the turn outside a lane, so waiting would deadlock on itself | `Integrity` | `Malformed` | `Integrity` |
  | Poisoned queue | a holder panicked with the queue's mutex held | `Unknown` | `Uncertain` | `UnknownOutcome` |

- An owner that is gone does not strand waiters: an in-process holder that
  unwinds releases its turn in `Turn`'s and `LaneHold`'s drops, and a
  holder process that exits releases its kernel lock. The waiter then
  proceeds; it does not fail.
- `close` lets the call holding the turn, and its lane's nested work,
  finish. A waiter already inside the kernel `flock` wait finishes that
  wait (bounded by another process's one short transaction) and then runs
  its job; only in-process queued and later callers fail `Closed`.
- The serial refill now queues like any writer. `writer_contended` in
  `filesystem_port.rs` is kept for an exact SQLite `Busy` at the refill's
  one `BEGIN IMMEDIATE`, which only a writer outside the cooperative lock
  can cause; its comment says so. Admission itself never returns `Busy`.
- Short holds and ordering are unchanged: same lock order (turn, then
  `flock`), same lane semantics, one SQL attempt per operation.

### `WAITERS` (audit A11)

Removed rather than resized. The 8 KiB of tickets was never the resource;
the waiting threads are, and they exist before they queue. A count bound
would only turn correct queued work into `Capacity`. The real limits are
the process's thread limit and, upstream, the daemon's and SDK's own
admission counts (audit A12–A15), which this fix does not change.

### Not covered

- Cross-session and cross-process order is the kernel's `flock` wake order,
  not FIFO. Each Session contributes at most one kernel waiter, and holds
  are single short transactions, so barging is possible but bounded per
  hold; no starvation guarantee is claimed.
- No product path calls `Handles::close` yet: daemons and the SDK end a
  Store by process exit, which ends its waiters with it. `close` is the
  typed exit for an owner that closes a live Store.
- In-process waiters have no per-request cancellation token; none exists in
  the Store call path. A cancelled caller's thread stays queued until its
  turn or `close`.
- `AcquisitionProvider::epoch` (host Init) still returns `Busy` on a
  contended `try_lock`; it is not writer admission and is out of scope.

## Proof

`core/crates/layerfs-persistence/tests/writer_admission.rs`:

| Test | Asserts | Platform |
| --- | --- | --- |
| `queued_writers_outlive_the_old_window_and_all_succeed` | 4 writers queued 2.5 s behind a held turn all succeed (failed `Busy` ×4 before) | both |
| `a_cross_process_waiter_outlives_the_old_window_and_succeeds` | a writer queued 2.5 s behind another process's lock succeeds (failed `Busy` before) | Linux |
| `queued_writers_are_admitted_first_come_first_served` | 5 writers arriving 100 ms apart get increasing pack ids | both |
| `closing_the_store_fails_every_queued_writer_typed_and_lets_the_holder_finish` | 3 queued writers fail `Refused "Closed"` within 2 s while the holder still holds; the holder succeeds; later Storage and lane calls fail typed | both |
| `writers_queue_behind_a_held_publication_lane_without_deadlock` | a lane held 2.5 s runs its own nested write; 3 writers and a second lane queue behind it and all succeed | both |
| `a_signalled_kernel_waiter_is_cancelled_and_leaves_the_queue_cleanly` | `SIGUSR1` (no `SA_RESTART`) to the kernel waiter gives `Refused "Cancelled"`; the next queued writer takes the turn and succeeds after release | Linux |
| `a_holder_process_that_exits_hands_the_lock_to_the_waiter` | killing the lock-holding process lets the waiter succeed | Linux |
| `a_thread_reentering_its_own_turn_is_refused_instead_of_deadlocking` | a read plan that writes inside its own turn gets `Malformed`; the turn is then released | both |

The lock child's and held read plan's own safety timeouts rose from 3 s to
10 s so they outlast the 2.5 s hold. Linux tests need the `signal` feature
of the existing `nix` crate as a Linux dev-dependency; no new crate.

## Checks

One run each, ≤ 120 s limit, at the fix commit's source.

Host: macOS, `cargo +1.85.1 --locked`. Linux: pinned image
`sha256:378b799e…`, `-j2` (`CARGO_BUILD_JOBS=2`), FUSE flags, each target
in its own container, `timeout 115`, `LAYERFS_CONSTRUCTION_WORKERS=1`.
Load is the host's 1-minute average at start.

| Check | Where | Result | Test time | Wall | Load |
| --- | --- | --- | --- | --- | --- |
| `writer_admission`, before the fix | host | 1/2 (expected failure: `Busy` ×4) | 2.5 s | — | 7.8 |
| `writer_admission`, before the fix | Linux | 3/5 (expected failures: `Busy`) | 2.6 s | — | 12.6 |
| `layerfs-persistence` suite | host | 115 passed, 0 failed, 1 ignored | — | 27 s | 16.4 |
| `layerfs-storage` suite | host | 75 passed, 0 failed | — | 3 s | 12.7 |
| `layerfs-store-service` lib and 30 test binaries | host | 61 passed, 0 failed, 2 ignored | — | 67 s | 11.4 |
| `layerfs-store-service` `scale_lineage_cap`, `sdk_merge_scale` | host | 5 passed, 0 failed, 9 ignored | — | 47 s | 10.7 |
| `writer_admission` | Linux | 11/11 | 2.7 s | 22 s | 11.8 |
| `store_commit` | Linux | 10/10 | 0.2 s | 86 s (incl. rebuild) | 11.0 |
| `mounted_commit` | Linux | 9/9 | 1.2 s | 12 s | 10.3 |
| `concurrent_commits` | Linux | 3/3 | 0.1 s | 7 s | 9.5 |
| `mounted_install` | Linux | 4/4 | 1.8 s | 15 s | 9.1 |
| `mounted_commit_failures` | Linux | 4/4 | 1.2 s | 9 s | 8.2 |
| `clippy -p layerfs-persistence --tests -D warnings` | Linux | clean | — | 20 s | — |
| `clippy -p layerfs-persistence -p layerfs-daemon --tests -D warnings` | host | clean | — | — | — |

The store-service suite does not fit one 120 s run under this load (its two
scale binaries take 35 s and 22 s), so it ran as two selections covering
every binary. An earlier whole-suite attempt reached the 120 s limit after
63 passes with no failure; its log shows the two remaining binaries had
not started, not a hang. The Linux `store_commit` wall includes rebuilding
the daemon, because the persistence test's `nix` features differ when the
daemon package is selected alone.

## Deleted tests

None.
