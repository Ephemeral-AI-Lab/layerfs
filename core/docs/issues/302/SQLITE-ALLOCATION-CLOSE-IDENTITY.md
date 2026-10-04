# Allocation close identity cause diagnostic

> Status: prospective instrument; allocation lifecycle unchanged, no new speed claim.

Based on76c066138. Before changing descriptor lifecycle, completion reports the
actual descriptor/device/inode/logical bytes used by its allocation owner and
nested F_TRANSFEREXTENTS/scratch-close clocks. The first-party public-close
observer records fixed bounded descriptor+device+inode rows and actual return/
errno, preserves original call/result/errno, and signals omitted identities.
Match all three identity fields; filename class alone is not sufficient.
Historical symbol-coverage limitations remain; this instrument specifically
checks that the real owned allocation fd's close is observed successfully.

The tracked run_sqlite_close_identity.py prospectively pins its orchestration,
observer, archived release driver, fixture, source/harness and cold helper,
uses a fresh owned output and15s cause envelope, one invocation. Independent
verification is SKIPPED for this count-only diagnostic; the original qualified
namespace proof and root cross-check are explicit. No result supplies an
ordinary speed gate or replaces the failed10k arm. Source/main/scratch closures
and allocation release remain inside the complete lifecycle.

Five allocation-release checks, owning persistence/project all-target Clippy,
Core fmt and441-file production boundary PASS. A synthetic standalone external
writer probe on current SQLite3.51.0 remains SQLITE_BUSY (rc5) after an extra RW
fd closes while a connection owns BEGIN IMMEDIATE. This supports evaluating a
short-lived handle on this qualified runtime; it does not substitute for complete
provider concurrency/custody checks or claim all runtime/filesystem lock styles.

User authorizes evaluating/implementing a temporary allocation handle opened
only when needed with exact file identity, exclusive ownership and checked close,
then (if needed) the original bounded preallocation-before-pack-insert algorithm
for Disposable. Reference source is unmodified sqlite/reservation.rs. Keep
canonical/physical/queue/cache/worker bounds and final allocated-storage gate.
Do not assume preallocation removes unused extents. Durable remains independently
qualified; no descriptor leak, skipped release or delayed out-of-timer close.
