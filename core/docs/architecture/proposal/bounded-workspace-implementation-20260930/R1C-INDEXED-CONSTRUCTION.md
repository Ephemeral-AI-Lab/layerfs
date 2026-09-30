# R1c indexed construction metadata

> **Status: Current source description; partial R1 delivery.**
> Source in this commit against parent
> `06fe8363d5c317c49876d5189d374c1a34cc6010`, 2026-09-30.
> Actual source/check outcomes belong to the append-only
> [implementation log](../../../issues/287/IMPLEMENTATION-LOG.md).
> Global SQLite, whole-operation memory, native protection, Linux and benchmark
> qualification are separate unrun gates; StrictServerMemory remains disabled.

## Common canonical path and exact selected authority

[C1 update](../../../../crates/layerfs-content/src/filesystem/update.rs) supplies
[DirectoryRoots](../../../../crates/layerfs-content/src/filesystem/state/directory_roots.rs)
to the same canonical build/update algorithm for both public compatibility and
Server's prepared path. Capacity is checked against the declared directory count
before canonical output. The producer retains at most128 typed records, appends
strictly increasing serial keys, acknowledges the exact batch, then freezes an
independently accumulated seal. It drops the producer allocation before consuming
pages. The cursor accumulates a separate ordered digest and checks exact count,
bytes, final cursor and EOF; errors stay terminal without a second request.
Point lookup checks its exact selected key. C1 never opens a database or file.

[Selection](../../../../crates/layerfs-content/src/filesystem/state/selection.rs)
issues a checked nonzero process token in a private live Arc and binds the full
selector32 to one exact owner before sharing. Raw token bytes cannot recreate the
capability. A dropped prior clone still prevents later binding; phase0/unbound
scope and zero selector/binding refuse. Native C2 bindings include fresh nonce,
parent/directory/file device/inode/uid/mode, full selector and token. Resident
compatibility uses an explicitly logical association and its fallibly allocated
declared population, including retained independent profiles above64KiB.

Only DirectoryRoots is implemented. Its key is token8/phase8/table1/serial8=25;
root32 plus framing6 makes a63-byte record. Full selector and binding are in the
81-byte scope and seal transcript. Seal130, append-header88 and page-header163
yield8152/8227 bytes for128 records. Both128 records and65536 encoded bytes
include their headers. Vec capacities/descriptors and SQLite borrowed rows are
additional live allocations. The private seal uses existing BLAKE3 with domain
`layerfs/indexed-state/v1\0`; it changes no FileSet/result SHA-256 wire contract.

Other validation, graph, draft, reference, count and final-row populations remain
future R1d responsibilities. This replaces the real DirectoryRoots authority;
it does not claim a complete paged namespace construction or v2 certification.

## Native C2 metadata owner

[Construction state](../../../../crates/layerfs-storage/src/construction_state/mod.rs)
admits the declared logical count/bytes and a bounded configured owner slot
before dependent native/SQL effects. Each selected DirectoryRoots session keeps
one16MiB physical class and at most65536 rows (4128768 record bytes before SQL
pages). The configured owner count comes from the existing persisted Store writer
budget; its default remains2. Future full-name/draft shapes are not admitted by
this class. The owner table is fixed and includes retained failures/Unknown.

The supplied parent is an absolute, real same-effective-user directory without
group/other write permissions. A fresh random `.lfcs-<nonce>` subdirectory is
created0700; each file is created exclusively0600 relative to owned directory
FDs. Descriptor/path device/inode/uid/mode are checked through SQL open and native
cleanup. No existing scratch, foreign pathname or unknown leftover is adopted.
Actual native preallocation uses the same safe platform primitive as Store,
preserving logical file bytes/length and checking allocated blocks separately.
Required authority/provider capability failure is explicit, without fallback.

[Runtime SQL](../../../../crates/layerfs-storage/sql/construction_scratch.sql)
selects LFCS application_id0x4c464353/version1. The192-byte exact owner header is
separate from canonical CAS. STRICT WITHOUT ROWID tables store the selected
header/seal and ordered key/ordinal/root facts. Checked key/length grammar and
PK point/range queries avoid repeated prefix scans, sorting or full collections.
Ordinals plus the exact sealed count establish EOF without an extra129th record
or repeated COUNT query. SQL BLOB widths are checked while borrowed before copy.

The private connection verifies4KiB pages/max4096 pages/cache512KiB/mmap0,
MEMORY journal/temp, synchronousOFF, foreign keysON and busy0. Safe SQLite limits
bound length/SQL/columns/expression/compound/VDBE/functions/attachments/variables/
triggers/workers and are read back. This does not enable SQLite global32MiB
enforcement or establish engine dirty-page/journal/RSS/cache progress. Those are
R1e owning proofs. Store schema/packs/capacity/producer count are unchanged.

## Cleanup, failure custody and Server composition

C1 logical release closes further selected-table access. It does not close/unlink
the native session or refund disk/FD/engine credits. C2's adapter retains the
original typed StorageError; taking it once never reactivates a quarantined owner.
One explicit physical release marks its attempt before close/unlink and refunds
only after verified removal. Failed returned connections/owned files are retained;
Unknown denies access and destructive cleanup. Drop transfers a live resource
into its bounded authority without rollback/retry/unlink. Last-authority loss may
preserve handles until process exit; aggregate factory lifetime credit/drain is
still an R1e/R7 gate, not a physical-memory claim.

[Server construction](../../../../crates/layerfs-server/src/service/construction.rs)
keeps a lazy authority per configured Store. The known prepared shape and captured
context select phase1 before begin_save or a body read. Pure catalog/read paths
do not initialize it. Failed initialization stays refused, with no automatic
retry after an external condition changes. A refused oversized shape does not
poison an otherwise valid owner. Actual C1 construction borrows the admitted C2
adapter; typed Save and state failures retain their custody. Checked native
cleanup precedes known Save finish and C5 stage publication. Failed cleanup
blocks the candidate result. Scratch's allocation/I/O-free quarantine observation
selects exact retained custody instead of attempting denied destructive cleanup
and inventing an Integrity cleanup failure. Every pre-finish build error explicitly
calls C2 Save abort; that owner decides its own known/quarantined/attempted state.
Scratch Unknown cannot hide a real known-Save cleanup error or reclassify another
owner. Previously finished input roots are outside that Save's cleanup predicate.
Unknown owners receive no guessed rollback/refund/adoption.

The old v1 response can summarize one cleanup code. The complete versioned result
owner and phase-scoped composite C2 credit are R3-dependent; this checkpoint does
not invent a completed-root list or change wire tags. Catalog native protection,
global byte permits, engine envelopes and concurrent enablement remain disabled.

Independent C1 vectors, real C2 metadata/SQL/native tests and the real occupied
Server Save/catalog test are the owning exits. Their actual commands/failures/
platform gaps are recorded in the log. No performance row or #288 campaign is run.
