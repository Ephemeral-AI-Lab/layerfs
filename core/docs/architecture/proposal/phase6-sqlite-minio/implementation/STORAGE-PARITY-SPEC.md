# SP1 — preserve storage behavior with SQLite metadata and MinIO payloads

> **Status: Current planning checklist; no release candidate exists.**

SP1 is prospective and unimplemented; no new test or benchmark is claimed.
Strict-split revision source: 8c926b9392f3636ae156236dc26d0510ee069d8d.
The earlier all-object MinIO proposal is superseded by the owner decision below;
its published source and historical evidence remain unchanged.
S2 edits remain uncommitted/unverified. This checkpoint is separate from daemon
population/shared-engine and C1 unfinished-state work.

## Dependency and order

SP1 can proceed in a clean worktree from the published strict-split packet;
local S2 changes are separate unverified work, not a prerequisite to import.
Prioritize SP1 read/write parity before large S2 end-to-end measurement, complete
repository import or storage qualification. S2 and SP1 share frozen identity and
locator contracts; SP1 does not own daemon population or C1 draft redesign.
No unchanged benchmark campaign rerun follows from this spec.

## Owner storage split, 2026-10-02

The daemon has one SQLite engine for scoped live Workspace rows. Global SQLite
stores all committed canonical filesystem metadata, including directory/inode/
attribute/file-mapping records and pooled metadata values/groups, alongside
locators, history and conditional publication. MinIO stores file-content packs
only: whole-file payloads and CDC chunk payloads using FULL/PREFIX/STORED.
Ordinary filesystem metadata and PooledMetadata groups are never uploaded to
MinIO. Encoded/canonical metadata BLOBs in SQLite are allowed; a SQLite shadow
Store for file payloads is forbidden.

Placement uses logical producer/caller provenance, not ObjectRole alone:
attribute values can reuse content-object grammar. The same canonical identity
may be required in both domains; freeze domain-aware lookup/dedup without
changing canonical hashes. SQLite indexes/pages/cache handle growing metadata;
no custom population pager/index or quadratic scan moved into SQL.

## Reused mechanisms and owned boundary

Keep C1 CDC/chunk identities/construction/edits and C2 codec/framing/pack/depth/work
policy. Existing Phase 4.5/Phase B receipts retain their exact source/scope/status.
Current experimental packing calls encode_full directly and reader supplies no
base. Existing C2 selection/resolver use SQLite Connection/locator/pack access;
there is no already-complete generic MinIO Store backend to claim or substitute.
Refactor the narrow domain-aware lookup/pack/base access boundary needed to reuse
existing selection and reconstruction algorithms. Metadata encoding/pooling stays
in SQLite; payload encoding/packing stays in MinIO. No second independent selector/
codec, file-payload SQL spool, host construction return or dependency patch.
Freeze required provider/schema allocations from source before implementation.

One coordinated owner covers core C2 selection/resolver access plus phase6-live
packing/read_window/objects/locator protocol and associated contracts/tests.
Daemon mutable inode/name/extent changes remain S2-owned. Freeze actual interface
and profile/schema allocation from source before edits; do not invent tags or
base-reference columns. Base identity is authoritative in physical record bytes;
any dependency index is derived and must agree. Global catalog retains thin
trusted registration/publication, not candidate tree/provider re-certification.

## Smallest complete path

First enable and prove existing delta reconstruction against MinIO payload lookup
and SQLite canonical/pooled metadata lookup,
with payload-chain intermediate/final canonical authentication, existing pooled
group/edge/requested-leaf authentication plus independent reads of each retained
pool leaf, and unchanged chain limits.
Then feed finalized objects/predecessor provenance through existing dedup/FULL/
PREFIX selection; preserve full framed-cost comparison and exactly one eligible
trial. Missing/ineligible/unprofitable candidates select FULL by policy; codec,
I/O and integrity errors remain failures, never automatic alternate routes.

Upload and ACK complete immutable file-content packs, then stage metadata and
locators in bounded SQL batches; register only valid completed dependency closure
before short conditional C5 publication. No transaction spans construction/upload. Preserve first valid CAS
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
Metadata reads use global SQLite and issue no MinIO metadata GET. Payload reads
use MinIO; the strict adapter does not restore a SQL file-payload shadow Store. Cover
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
