# Store concurrency model: one global database, one writer, short holds

> **Status:** Current, owner decision 2026-10-18. Binding design rule for the
> global Store and every writer of it (daemons and, after #352 F-020, the SDK
> process). It records a decision, not new code: the locks it names exist
> today (`core/AGENTS.md`, writer admission). Root AGENTS.md carries the
> one-paragraph rule. The database itself: [Global SQLite Store](84-global-sqlite-store.md).

## 1. Decision

1. **One global database.** All committed history, content and Branch heads
   live in one SQLite Store file on a volume the writers share. No sharding by
   Branch, project or object role, no second database, no objects outside the
   database (the latter per the 2026-10-13 direction).
2. **One writer at a time.** The Store has a single write lane: SQLite's own
   single-writer rule (WAL lets readers run beside it), coordinated across
   processes on Linux and queued within a process. No concurrent-writer engine,
   no patched SQLite, no `BEGIN CONCURRENT`, no per-Branch writer.
3. **Short holds are the performance model.** Expensive work (construction,
   encoding, compression, object saves) runs before the lane is taken; the lane
   is held only for the step that must be atomic: check the Branch head, merge
   if it moved, publish. Reads never take the writer.
4. **Concurrent Commits are expected to be uncommon in real use.** Agents work
   through tool calls separated by model latency (seconds), so even several
   agents produce a few Commits per second, and two Commits on the same Branch
   at the same instant are rare. The design optimizes the uncontended Commit
   and keeps contention correct and bounded, not fast at any cost.

## 2. The locks, and why each exists

| Layer | Scope | Why it exists |
| --- | --- | --- |
| SQLite file lock (WAL) | the Store file | SQLite allows one write transaction per database; readers proceed alongside |
| Directory lock (Linux) / named kernel mutex (Windows) | all writer processes of one Store | Writers in different processes (each Sandbox's daemon, the SDK process) take turns instead of meeting `SQLITE_BUSY`; on Windows the mutex is named per Store directory and an abandoned mutex is taken by the next waiter |
| Writer admission queue | threads of one process | Threads take turns on the process's one writer connection, first come first served, before touching SQLite |
| Publication lane | one Commit's publish | Head check, merge and publish are one atomic step, so two Commits never both publish on the same old head |
| Open/teardown holds | one Store's connect, writable setup and close | The connect-time statement block (profile pragmas, format reads — where a cleaned WAL `-shm` is rebuilt under an exclusive recovery hold), the writable setup block (held publication lane, nested turns take no second lock) and a coordinated close (`StoreLane`/`Handles::shutdown`; the service's `close` stops overlays first, then drops the sessions under the lane) run under the same cross-process lane: SQLite's close-time checkpoint is writer work like any other and must never run beside another opener's first attach (no-retry rule) |
| Workspace lock | one Workspace's overlay database | Separate concern: guards that Workspace's own overlay connection (root AGENTS.md) |

The lane's Windows name is SHA-256 of the resolved directory's encoded
bytes truncated to 64 bits — a version-stable digest, never
`DefaultHasher`, whose algorithm may change between Rust releases and
silently split one directory onto two lanes across builds (K3 review F1).
The mutexes live in the `Local\` namespace: one login session's lanes are
invisible to another (DIRECT_STORE.md §6).

The queue and the directory lock replace failure, not add serialization: the
repository forbids automatic retries and busy handlers (one attempt per
operation, exact failure), so a writer waits its turn before its single SQL
attempt rather than failing with `Busy`. The wait has no time limit (#352
F-040): queued correct work waits its turn. Its only exits without the writer
are typed: the Store closing (`Closed`), a signal interrupting the
cross-process wait (`Cancelled`) and a poisoned queue (`Unknown`).

## 3. Consequences for design and review

- **Measure and optimize the hold, not the lock.** A change that lengthens the
  lane hold (work that grows with tree size, history or Branch count inside the
  lane) is a defect, as #352 F-025 found (a merge's path search walked the whole
  tree inside the lane).
- **Uncontended cost is the product cost.** A call no other Commit overlaps is
  what a user sees per tool call; it must stay flat as history grows.
- **Contention must be correct, bounded and exact:** every publish applies once
  or refuses with its outcome; no lost update; no retry.
- **Do not propose** sharding, multiple databases, concurrent writers or
  objects outside the database to fix throughput. Propose shorter holds or less
  work per Commit first; a real need beyond that is an owner decision.

## 4. Replay and measurement (#352)

The history replay's headless player has no delays (SPEC §2 "Maximum speed,
always"): every agent's tool calls fire back to back, which compresses time
and makes contention far more frequent than real use (20–30% of fs Commits
merged in the lane at ranges 9–11, against ~1% when calls were serialized).
That is a useful stress test and stays, but it is not the user's experience.
So the replay reports each operation's latency in two series:

- **Uncontended:** calls that no other Commit or merge overlaps in time. This is
  the product cost; the speed gates judge it.
- **Contended:** calls that overlap another Commit or merge. Reported beside the
  first and watched for growth: it trips when the cost of the contended work
  itself grows (for example merge cost with tree size), not because the
  compressed replay queues calls.

Merges are never removed from either series; they are classified by overlap,
which is a property of the input. A realistic-pacing control run (sampled gaps
of a few seconds per agent) may run at a milestone to show real contention.
