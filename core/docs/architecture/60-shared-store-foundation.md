# Shared Store platform, contention and seal

> **Status:** Current general guide.
> Source update after `122b8c1b3`, 2026-10-08; provider implementation,
> not daemon/control/Exec integration or performance qualification.

Current #327 format update (product `ce41af000`, 2026-10-15): new Store schema
versions are 16–18, or 22–24 with acquisition, and history identity format is 3.
Old Disposable Stores are rejected without migration. Connections retain a bounded
64-entry prepared statement cache. One Store-totals row records acknowledged
logical history, unique canonical locator lengths, complete pack lengths and
metadata-domain bodies, maintained inside original insert/publication transactions.
Read statistics add occupied SQL-page overhead without an object scan. `file` is
logical SQLite pages including pages in WAL, not extra WAL allocation. Definitions
and the passed retention screen are in [the backend guide](../../../api/docs/backend.md#storage-figures)
and [counter receipt](../issues/327/COUNTER-SCREEN-20261015.md).

The macOS 3.51.0 component verifier observed CannotOpen on native read-only open
of a sealed WAL file with no sidecars. Its independent verification copy provisions
WAL ownership explicitly before opening the read-only reader. This is not a sealed
read-only capability claim or permission to retry failed product operations.

[`Handles`](../../crates/layerfs-persistence/src/store/handles.rs) opens one
validated session on macOS or Linux. macOS links system SQLite; Linux enables
`bundled` on the existing pinned rusqlite dependency. The database remains
distinct from every daemon's local overlay.

Disposable/WAL/OFF is the sole authorized execution profile until the owner
explicitly reauthorizes Durable. Retained Durable code selects FULL, with full
synchronization on macOS; it may compile but must not execute. Disposable
supplies no kernel/VM-crash guarantee.
Identities are `sqlite-wal-full-v2` and `sqlite-wal-off-v2`. Shared creation selects WAL;
open reads and verifies it without conversion. Existing memory-journal Stores
are refused. Profile selection across processes is a provisioning fact rather
than a persisted cross-profile lock. Page size4096, foreign keys1, checkpoint
interval1000, journal limit4194304, cache−2048, mmap0, temporary storage2,
DEFENSIVE and checked busy timeout 0 are retained.

The connection applies the selected `synchronous` and `fullfsync` settings
before page/acquisition setup, WAL selection or schema creation. Selecting WAL
on a new file can itself write the main file and rollback journal; validating
OFF after that transition cannot establish Disposable startup behavior. The
external [Disposable VFS regression](../../crates/layerfs-persistence/tests/disposable_startup_sync.rs)
delegates actual SQLite VFS operations and checks that public create, Save,
seal and writable reopen issue no `xSync` calls across all three layouts with
and without acquisition tables. Its separate non-Store control proves the
observer forwards and counts `xSync`. This is a persistence profile correction,
with no new performance qualification or change to existing receipt verdicts.

Owner supersession 2026-10-12 for issue #319 requires concurrent Workspaces to
keep independent Commit state while sharing short atomic Store transactions.
Every mutation still makes one `BEGIN IMMEDIATE` attempt with busy timeout 0.
Busy/Locked/Protocol lock codes retain typed `Busy` through Persistence, Storage
and History: no body, transaction or rollback follows a refused begin.

Before SQL, each session admits one caller through a FIFO connection turn.
Since #352 F-040 (fix record 038) the wait has no time limit and no count
limit: queued correct work waits its turn and is never refused `Busy` or
`Capacity` for waiting. Each queued ticket belongs to one blocked thread, so
the queue is bounded by the threads the process runs. A waiter leaves without
its turn only when the session is closed (`Handles::close`, typed `Closed`) or
the queue's mutex is poisoned (`Unknown`); it removes its ticket and later
callers remain runnable. SQL counters are unchanged by admission itself.

On Linux, a writable transaction also acquires an exclusive advisory flock of
an independently opened description of the existing Store parent directory,
after its FIFO turn, in one blocking kernel wait. It never opens an additional descriptor to the main,
WAL or SHM files. Closing any main-file descriptor outside SQLite can cancel all
of that process's POSIX locks on that inode; SQLite documents this
[lock-corruption hazard](https://www.sqlite.org/howtocorrupt.html#_posix_advisory_locks_canceled_by_a_separate_thread_doing_close_).
The directory inode is a coarse coordination key: multiple Store files in the
same directory share the short transaction lane. A relative bare filename uses
`.`. No sidecar is created. Existing Store placement requires stable names and
parent directory authority; renamed/reparented active stores are not qualified.
The blocking wait is readiness before the single BEGIN, not transaction
replay; a signal that interrupts it returns typed `Cancelled`, and a holder
process that exits releases the lock to the next waiter. Separate handles and daemons on the same local named VM volume share
this kernel admission. The lock covers one atomic transaction through its
known COMMIT or definite ROLLBACK, never a Save, construction or whole Commit.
FIFO is provided within a session; across sessions and processes the kernel's
flock wake order applies, with no FIFO guarantee. Each session has at most one
thread in that wait, behind holds that are one short transaction each. An uncooperative SQLite
writer still produces the original exact Busy at BEGIN.

macOS has the local session FIFO only. Its flock interacts with SQLite's
open-file locking, so no main-file flock, extra sidecar or cross-session/process
coordination is introduced there; a separate macOS writer retains exact SQLite
Busy. Host Init remains sole-owner. The Linux admission scope requires a local
filesystem with independent flock/fcntl semantics, as does the existing shared
Store placement contract; host shares and network filesystems are unsupported.

The two-second deadline is an agent decision pending owner review, using the
retained 46,750 ns serial transaction upper observation and 9,558,915 ns
three-transaction Commit upper observation, with room for peers, large
publications and VM interference. These inclusive observations are not exclusive lock maxima or
performance qualification. Historical refusals remain failed.

Definite failures retain existing rollback handling. Unknown outcomes and
poisoned unwinds retain the advisory owner with the quarantined session until
SQLite connection teardown, which precedes advisory release. Known release is
checked explicitly; a release failure retains ownership and returns uncertainty.
Linux sole-owner seal also owns admission during checkpoint and checked close;
macOS seal is unchanged. Separate reader sessions take WAL snapshots without
global writer admission. No caller retries a failed SQL operation.

The old macOS allocation/extent-release owner, associated counters/result
fields and public checkpoint API are removed. Host `Handles::seal` consumes
its handles, refuses retained provider ownership, takes the sole session,
checkpoints TRUNCATE once, checks close, and verifies that no WAL/SHM/journal
sidecars remain. It returns file path/bytes, provisioning profile and the
linked SQLite version. A failure causes no guessed deletion or retry. New
openers must be excluded by the provisioning lifecycle through handoff.

The [owner-selected seal correction](../../../docs/roadmap/0.1/0.1.7/macos-seal-allocation-stride1-20261007.md)
adds one narrow macOS completion step after checked SQLite close and sidecar
absence. If physical allocation exceeds logical length, the safe, already
locked nix0.31.3 `F_TRANSFEREXTENTS` API transfers unused extents beyond EOF to
an exclusively created empty sibling, then closes and removes that sibling.
The original file is never replaced or copied. Regular-file, single-link,
device/inode and length checks fence the operation; symlinks/hard links are
refused. Transfer or temporary-close uncertainty retains the owned sibling;
cleanup failures remain uncertain, without a second attempt. No preallocation,
vacuum, payload scan, persistent allocation owner or new unsafe boundary is
introduced. Linux has no extent-transfer step. The shared daemon Store stays
open and does not run this sole-owner host seal operation.

Apple's SQLite keeps persistent WAL by default. The
[owner-approved wrapper](../issues/307/SEAL-PERSIST-WAL-DECISION-20261007.md)
sets this file-control flag to0 and reads it back, only in the macOS seal path.
One audited FFI module is allowed; all other Persistence modules remain covered
by the unsafe-code boundary. Linux's path is unchanged by this exception.

The macOS system SQLite 3.51.0 cannot start a read-only connection to a freshly
sealed WAL file with absent sidecars (raw SQLite returns code14/CANTOPEN).
A writable opener creates the runtime sidecars first. The daemon's required
startup order is therefore write handle then read set; no read-only-to-writable
fallback exists. Read-only operations remain refused before mutation. Seal is
host provisioning, never a daemon maintenance or Commit operation.

Acquisition and bounded page reclamation remain host Init facilities. Their
abandoned-operation model is not safe for multiple independent importers and
is not exposed through daemon Store ports. No collection runs under writers.
Save reservation batching and combined stage/publication remain F8. Physical
Store placement requires local kernel locking/shared memory: a named VM volume
or container-local disk, never the repository host share.

[F1–F4 receipts](../issues/307/PRE-S8-F1-F4-20261007.md) cover Disposable
functional checks and retained failures. New Durable execution stays disabled
until explicit owner reauthorization.
Historical allocation/speed results keep their original profiles and verdicts;
mechanism retirement supplies no speed or storage improvement claim.

## WAL throughout and the accepted host Init baseline

The [owner supersession](../issues/307/PRE-S8-WAL-BASELINE-DECISION-20261007.md)
keeps WAL throughout both Init and Commit. The private MEMORY creation and
promotion path is removed, while ordinary checkpoint/close seal and its
approved macOS file-control wrapper remain. The existing198720291ns host
Disposable1000 observation is the accepted baseline for its exact scope.
Its original1.10× comparison against retained155291459ns MEMORY evidence
remains FAIL; there is no new speedup, matched pair, Commit latency or complete
resource claim. All raw receipts and the withdrawn private-route history remain.

Both operations reuse canonical construction, Storage Save and WAL/history
implementation with separately owned input and mutable producer state. Each
Storage handle permits one active Save; concurrent producers use separate
handles over the same Store. No mutex spans a whole Commit, and Commit never
runs Init acquisition/abandoned-operation cleanup. These concurrency and
publication obligations still require their own proof.


## Host acquisition schema (#320, 2026-10-14)

Initial acquisition uses HostIdentity/host_path in its Storage port and Project
adapter. SQLite stores init_host_file with init_host_file_identity, host_path
and the packed host_identity column. Query/method names use put_host_file,
bind_host_file, remove_host_file and discard_host_file. Identity/evidence byte
widths, bounded windows, counters and acquisition behavior are unchanged.

The renamed acquisition layouts are Store schema versions 13–15; 7–9 without
acquisition tables retain their identities. Old 10–12 files refuse at open,
without migration or inferred cleanup. Exact source/definition validation and
the independent census oracle follow the new SQL. The owner permits this format
change. Existing sealed benchmark originals were inspected read-only before
the change: empty (163840 bytes) and tier 1 (102072320 bytes) both have schema 10
and the retired acquisition names. They require fresh preparation; their bytes
and receipts remain preserved pending the handoff's specific preparation approval.
