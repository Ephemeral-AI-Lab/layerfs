# Bounded signature UPSERT treatment

Status: Mechanism count proof passes; new competitive speed unmeasured.
Based onf3913c845. Retained1000 diagnosis issued939 individual signature INSERTs;
source backend/sqlite/publish.rs loops one statement per signature. Public-API
512-row reproduction first failed:514 statements, expected one mutation plus
BEGIN/COMMIT. Failure log target/phase7-agent/signature-batch-before.log retained.

Signature UPSERT now uses ordered VALUES pages bounded by actual SQLite variable/
SQL-length limits and the existing512-object bound. Scalar/binding descriptors
are bounded;32-byte object IDs/signatures are borrowed, not cloned. The enclosing
atomic publication and>=stamp conflict predicate are unchanged. Empty signature
input does no work and imposes no unused statement capacity requirement.

Count proof512 signatures:3 statements/1 write commit.513:4 statements/1 write
commit. Stale stamps ignored; equal-stamp later publication replaces as before.
Second-page foreign-key failure rolls back preceding512 signatures and the earlier
body INSERT, with exactly one rollback and no acknowledged write commit. Full
publication readback and existing first-wins behavior pass. No format, row/byte,
cache, worker, durable profile or failure-handling bound changed.

Final checks10PASS:9 sqlite_publication tests and project init_sqlite full100/1000
namespace oracle. Scoped persistence/project all-target Clippy-Dwarnings, core
fmt, product boundary and23 self-testsPASS. Older unchanged workspace suites not
repeated. No CI/preflight claim. Test adjustments cover empty-signature and
cross-page behavior; intermediate logs remain, not speed samples.

Prospective measurement: one release/locked matched1000-file Init-v2 pair with
current sealed native cold helper, same15s complete/9.5s separate proof, fresh
DB creation/import/checkpoint/close product clock, exact roots and allocation
<=matched reference. Earlier failure and timeout receipts unchanged. No replay
at the same source/harness identity. Other tiers and histories remain required;
this count proof is not speed parity or all-seven completion.
