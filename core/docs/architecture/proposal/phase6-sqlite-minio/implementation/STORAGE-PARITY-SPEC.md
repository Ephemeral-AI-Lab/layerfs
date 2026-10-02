# SP1 — preserve existing content/storage behavior on MinIO

> **Status: Current planning checklist; no release candidate exists.**

SP1 is prospective and unimplemented; no new test or benchmark is claimed.
Published parent at specification: bfbf48ea7694e8450bd47a3bf2c297fdac36badc.
S2 edits remain uncommitted/unverified. This checkpoint is separate from daemon
population/shared-engine and C1 unfinished-state work.

## Dependency and order

Bring the current S2a edits to a coherent checked checkpoint before changing
lanes. Prioritize SP1 read/write parity before large S2 end-to-end measurement,
complete repository import or storage qualification; those must not measure
FULL-only experimental packing as the intended replacement profile. S2 metadata
work and storage parity share frozen locator/identity contracts, not one giant
source change. No unchanged benchmark campaign rerun follows from this spec.

## Reused mechanisms and owned boundary

Keep C1 CDC/chunk identities/construction/edits and C2 codec/framing/pack/depth/work
policy. Existing Phase 4.5/Phase B receipts retain their exact source/scope/status.
Current experimental packing calls encode_full directly and reader supplies no
base. Existing C2 selection/resolver use SQLite Connection/locator/pack access;
there is no already-complete generic MinIO Store backend to claim or substitute.
Refactor the narrow lookup/pack/base access boundary needed to use the existing
selection and reconstruction algorithms. No second independent selector/codec,
SQLite BLOB pack spool, host construction return, dependency patch or new format.

One coordinated owner covers core C2 selection/resolver access plus phase6-live
packing/read_window/objects/locator protocol and associated contracts/tests.
Daemon mutable inode/name/extent changes remain S2-owned. Freeze actual interface
and profile/schema allocation from source before edits; do not invent tags or
base-reference columns. Base identity is authoritative in physical record bytes;
any dependency index is derived and must agree. Global catalog retains thin
trusted registration/publication, not candidate tree/provider re-certification.

## Smallest complete path

First enable and prove existing delta reconstruction against MinIO pack lookup,
with exact intermediate/final canonical authentication and existing chain limits.
Then feed finalized objects/predecessor provenance through existing dedup/FULL/
PREFIX selection; preserve full framed-cost comparison and exactly one eligible
trial. Missing/ineligible/unprofitable candidates select FULL by policy; codec,
I/O and integrity errors remain failures, never automatic alternate routes.

Upload complete immutable packs before locator registration, and register only
valid completed dependency closure before publication. Preserve first valid CAS
locator choice, alternate pack races, base custody/pins and Unknown outcomes.
No live base may be reclaimed because its original Commit disappeared. Runtime
GC/migration remains unsupported until its owning gate passes.

## Exit proof

Use independently sealed old/new content and existing expected C1 identities:
identical objects reuse; similar changed CDC chunks actually select PREFIX;
unrelated/ineligible/depth-bound objects select FULL as specified; small whole
file and small/large transition preserve logical identities; base and dependent
in separate real MinIO packs reconstruct old/new bytes with authenticated chain
limits; missing/corrupt base fails without publication or guessed cleanup.
Include public SDK generic Exec/FUSE -> construct -> storage -> C5 -> retained
history/cleanup for the smallest affected path, no command recognizer.

Record actual full/prefix/duplicate counts, encoded and physical pack bytes,
base GET/decoded-chain work, same limits/workers and actual cleanup/custody.
Ordinary read doesn't require the discarded SQLite pack-storage backend. Cover
owning locked C2 and adapter tests, format/Clippy/boundary policy, exact source
seals and per-commit product LOC. Reuse unaffected checks/benchmark scopes;
measure only prospective affected mechanisms, preserve nonpassing outcomes.
No compression/page-size/worker/timeout/quota campaign. No final FULL-only parity
claim or promised identical physical pack IDs for a changed placement topology.

## Remaining separate gates

S2 population/paged verifier/shared daemon engine; C1 draft accumulation removal;
non-pausing current-successor install; independent Exec/Workspace ownership;
complete repository/syscalls, physical memory/cache/storage, import/cutover and
cloud durability remain explicit. SP1 preserves storage behavior; it does not
complete those goals or qualify an unbounded-size profile.

The detailed [implementation specification](sp1/IMPLEMENTATION.md) and
[minimal tests/history guidance](sp1/test.md) form the SP1 handoff packet.

Historical stride10/3/1 storage targets and receipt hashes are inventoried in
[HISTORY-STORAGE-BASELINE](HISTORY-STORAGE-BASELINE.md). Post-SP1 qualification
uses prospectively declared MinIO/SQLite accounting and preserves the three
independent schedules; no original-source benchmark was rerun for this inventory.

Owning implementation issue: [#295](https://github.com/Ephemeral-AI-Lab/layerfs/issues/295), native sub-issue of #293, created 2026-10-02.
