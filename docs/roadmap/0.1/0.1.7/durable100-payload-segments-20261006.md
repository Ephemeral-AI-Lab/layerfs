# Durable100 immutable payload candidate

> **Status:** Owner-selected prospective candidate, 2026-10-06.

The owner explicitly selected implementation of the
[reviewed proposal](../../../../core/docs/issues/307/DURABLE100-NEXT-LAYOUT-PROPOSAL-20261006.md)
and a new candidate identity. This selects the engineering experiment; it does not
declare parity or replace the retained Monolithic v2 FAIL.

Run exactly one `phase7-sqlite-init-100-acquisition-payload-segments-v1` candidate
at a clean committed source in the owned init-entry-performance worktree. Reuse
the original `phase7-sqlite-init-100-acquisition-v2` reference receipt at7edddbdb8e:
41,992,917ns and7,372,800B. Do not rerun the unchanged reference. It remains the
competitive Phase4.5 MEMORY/OFF public Service route, not a matched Durable layout.

The source is the original100-file/2-directory/102-path,5,000,000B seed1 prepared
fixture, manifest9b97114260decac04edb93b288b2c1b6037b2d9d7f4127c21186cc21a43201e3.
Reuse closed source preparation; create a fresh measured Store. Keep four supported
Init constructors, `LAYERFS_CONSTRUCTION_WORKERS=1`, A1 global acquisition,
WAL/FULL/fullfsync/checkpoint-fullfsync, automatic checkpoint1000,4KiB pages,
2MiB SQLite cache, mmap0 and current publication/read windows. The new explicitly
selected Store layout is PayloadSegments with acquisition, user_version10.

Build needed locked release examples within30s. Complete performance command,
including cold source attestation, is at most30s; independent sampled proof at
most19s. The entire product clock pays fresh DB/directory creation and synchronization,
Init construction/acquisition, immutable segment write/sync/checked close, catalogue
publication, required final checkpoint/allocation release and final close. No setup
warmth, pre-touch, helper construction lane or deferred product work is removed.
Use the same native cold helper and whole-source residency attestation; unknown
cache state remains INELIGIBLE. No lifetime RSS or source hint substitutes for a
phase residency proof.

Speed gate: `10*candidate_ns <= 11*41,992,917`. Final allocation gate:
`candidate_DB+WAL+SHM+payload_directory+all_segments <= 7,372,800B`. Include
unreferenced retained bodies if any. Keep raw ns/B, exact arithmetic and independent
speed/storage verdicts. The standard separate namespace verifier reopens the new
layout, checks every102 paths/kinds/directory metadata and the declared53 files /
3,354,003B sample. It is sampled content, not a full-payload oracle.

Use the normal nonblocking worktree-local lock, local Cargo target, fresh append-only
outputs, pre/post source/dependency/binary/helper/fixture seals, exact commands and
original reference hash. Archive closed raw stores before supplemental proof changes
writable SHM. Record all failures/unrun selections and the four unchanged unrelated
containers. VFS traffic and direct segment counters have separate coverage; request
bytes are not device bytes. Any instrumented count diagnostic is ineligible for
speed admission and gets its own fresh result. Final qualification is one arm only.

Public external tests cover old/new schema creation/reopen, canonical bytes, strict
and scoped reads, definite rollback and retained custody, read-only mutation refusal,
missing/tampered/short/symlink/hardlink/replaced files, extent bounds, physical
admission, singleton bounds and first-wins conflict. Native uncertain-close proof
uses an external observer, preserving actual synchronization and file operations.
No public API signature, third-party dependency, default profile or acquisition
placement is changed. Local commits and #307 evidence updates are authorized;
push/release/deployment and S10–S13 are outside this selection.
