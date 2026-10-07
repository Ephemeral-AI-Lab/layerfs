# Shared Store platform, contention and seal

> **Status:** Current general guide.
> Source update after `727476a4d`, 2026-10-07; provider implementation,
> not daemon/control/Exec integration or performance qualification.

[`Handles`](../../crates/layerfs-persistence/src/store/handles.rs) opens one
validated session on macOS or Linux. macOS links system SQLite; Linux enables
`bundled` on the existing pinned rusqlite dependency. The database remains
distinct from every daemon's local overlay.

Both profiles use WAL. Durable selects FULL, with full synchronization on
macOS; Disposable selects OFF and supplies no kernel/VM-crash guarantee.
Identities are `sqlite-wal-full-v2` and `sqlite-wal-off-v2`. Shared creation selects WAL;
open reads and verifies it without conversion. Existing memory-journal Stores
are refused. Profile selection across processes is a provisioning fact rather
than a persisted cross-profile lock. Page size4096, foreign keys1, checkpoint
interval1000, journal limit4194304, cache−2048, mmap0, temporary storage2,
DEFENSIVE and checked busy timeout0 are retained.

Every mutation makes one `BEGIN IMMEDIATE` attempt. Busy/Locked/Protocol lock
codes retain a typed `Busy` through Persistence, Storage and History. A busy
begin executes no body, starts no transaction, performs no rollback and leaves
the session healthy. Definite body failures retain rollback handling; unknown
outcomes retain quarantine. There is no writer gate, sleep or application retry.
Separate read connections use WAL snapshots while a writer is active. Each
session still permits only one caller at a time; daemon read-set composition
belongs to the later Store-adapter checkpoint.

The old macOS allocation/extent-release owner, associated counters/result
fields and public checkpoint API are removed. Host `Handles::seal` consumes
its handles, refuses retained provider ownership, takes the sole session,
checkpoints TRUNCATE once, checks close, and verifies that no WAL/SHM/journal
sidecars remain. It returns file path/bytes, provisioning profile and the
linked SQLite version. A failure causes no guessed deletion or retry. New
openers must be excluded by the provisioning lifecycle through handoff.

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
functional checks and retained failures. New Durable execution stays deferred.
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
