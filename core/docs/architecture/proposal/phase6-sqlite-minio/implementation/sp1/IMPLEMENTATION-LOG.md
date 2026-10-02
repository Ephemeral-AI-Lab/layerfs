# Strict SP1 implementation log

> Status: Dated planning checkpoint; not release evidence or a product contract.

Entries are append-only. The prior stopped task's fixture-only log belongs to
its original source; it is not imported or relabeled as strict-split proof.

## L1 — scoped C2 access and real strict reader, 2026-10-02

Parent `285dd3f4a54a896344e389b51a8495300b98b4ca`, owned worktree
`/Users/yifanxu/.codex/worktrees/phase6-sp1-strict/layerfs`, branch
`codex/phase6-sp1-strict`. Published source and LOC are recorded in the issue
checkpoint after exact staged/committed tree confirmation.

Owned scope: C2 provider-neutral access/selector/pool/index seam; experimental
global SQL catalog and strict reader; independently sealed external oracles.
Existing C1 IDs/CDC/codec/selection/depth/work policies are unchanged. The
pooled selector is moved into one shared helper, and the standalone SQLite
Store delegates to it. Pack and group eligibility checks precede cache answers.
Qualified physical depth keys retain the prior byte allowance by reducing
entry capacity. Growing authoritative metadata stays indexed in SQLite.

Independent source acquisition uses archived unchanged parent C1/C2, an owned
isolated target, source/binary/input hashes and opt-in ignored acquisition
tests. The published81f2 fixture evidence is reused only in its original scope.
The strict oracle adds three content/portable-metadata states, exact depth/work
outcomes and real-producer provenance: first CDC bytes[0,16396) ofN200000,
canonical Chunk ID
`a0210f01dd9a468b515cc918df5c77c5960bd47bfad53c4353e634c7e6bbd399`.
Ordinary sp1/opaque attribute construction emits exactly that ID/bytes.
Its mappings differ from the full large file; shared-mapping use parity is a
writer gate, not inferred from Chunk equality.

[Real reader evidence](evidence/strict-reader-r001/manifest.json) retains exact
source inventory, test binary, provider identity, commands and outputs. Local
HTTP MinIO is pinned by immutable image digest/version, isolated owned disk,
512MiB/1CPU provider profile; global SQL stays on the host. Correctness
diagnostics carry unknown page-cache state and no numeric speed/RAM claim.

Actual reader component outcomes:

- Payload FULL/PREFIX chains reconstruct seven requested canonical objects in
  demand order/cardinality; retained chain maxdepth2 and six aggregate edges.
- Every retained pooled leaf independently read/hashed; isolated SQL pool
  reads issue MinIO GET delta0.
- The actual CDC dual-use ID works in both shared-cache read orders; omission
  of either required domain placement refuses after warming the other domain.
- All25 sealed SQL metadata/mapping/attribute canonical objects are read and
  authenticated after reopen, with MinIO GET/PUT delta0. This canonical inventory
  witness does not stand in for integrated per-use reference graph validation.

Checks: affected locked C2 tests passed after extraction and pool/depth changes;
new provider access tests passed; boundary359 files/self-tests9 passed; Core
format check passed. The full initial C2 suite passed before the subsequent
candidate persistence extraction. All-target Clippy initially found fixture
style errors, corrected by fixture owner; adapter's first check found explicit
rusqlite SQL integer-conversion errors, corrected at the boundary. Every failed
attempt is retained in the worktree output/log. Final owning checks and candidate
seam coverage are separate handoff work, not implied by earlier checks.

Disposition: PARTIAL. Functional strict reader is delivered at component scope;
writer/private pooling/refusal matrix/public SDK-FUSE-C5/resource gates are in
progress. Dependent public enablement remains off. No storage parity, speed,
release, multi-W, non-pausing, import/GC/cutover or cloud durability claim.
Next: finish bounded private writer/pooling and refusal proof, then integrate
the strict transport and conditional C5 public path against the separate S2
shared-engine contract. Stride10/3/1 remains NOT_RUN pending parity/profile.

L1 production comparison (exact first-parent versus staged source):
Production LOC:135696 ->135991 (delta +295). Reference65417 ->65417 (+0);
Core70279 ->70574 (+295). tools/production_loc.py SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`,
Git archive snapshots with identical scope/exclusions; shipped SQL included,
inline legacy tests excluded. Experimental strict adapter1593 nonblank/comment
implementation lines are separately recorded and excluded from the product
headline while this unshipped research runtime remains under benchmark/.
This is added implementation, not relocation or legacy retirement.
Final L1 C2 all-target locked Clippy and Core format PASS; catalog frozen
check10/10PASS. Exact indexed adapter library is compiled separately from
ongoing uncommitted writer/native integration. Raw TSV empty final fields and
test stdout terminal blank lines remain authenticated; local evidence attributes
exclude only these byte-preserved data conventions from whitespace diagnostics.


## L2 — strict writer, native catalog and owned C5 composition, 2026-10-02

Status: PARTIAL implemented component checkpoint; source/check/LOC facts below
refer to the frozen owned source and exact staged/committed checkpoint. L1 remains its
original source and evidence. No historical receipt is promoted to the current
schema2/wireV2 profile.

The strict writer retains one C2 encoder/decoder, one activated admitted-FULL
index, unchanged FULL/PREFIX/pooling/depth/work policy, Native framing, and
bounded private canonical/group/tail windows. Raw pending reads and exact reuse
do not place an unsealed record. Additional logical uses share the eventual
physical record, with distinct reference-use facts. A new immutable snapshot
order follows each private tail append; referenced payloads require a known
provider ACK before SQL metadata staging. Typed provider failures distinguish
pre-send refusal from lost ACK without probe/retry/delete. Acknowledged payload
objects remain retained if subsequent SQL staging refuses.

Actual C1 callers now bind regular-file and metadata use separately. Inode-leaf
canonical bytes retain their exact identities while the caller supplies their
encoded inode-value roots as reference facts. An equal root used as both regular
content and metadata emits both uses; one descriptor or cache hit cannot stand
in for either required placement. Generic bare-C2 fixtures retain their original
scope and do not qualify the ordinary filesystem caller by inference.

Owned capture records bind Workspace incarnation, project/Branch, generation,
scope, C1 profile and base root. The indexed pending constraint survives Unknown
custody. Actual C5 Authority has no provider or codec capability: it checks the
capture/reservation/ready root, stages through C5, obtains storage publication
ACK, then calls conditional C5 publication. Known stage and storage facts remain
available after a failed publication; anonymous saves are refused.

P6SP1V2 native transport caps each request/reply at16384B and each transfer chunk
at8192B. Replies carry an engine epoch. An unresolved mutation or changed epoch
quarantines the session without replay. Native catalog operations support owned
admission, scoped lookups, domain-specific complete bodies, references, groups,
candidates and readiness; host C5 owns publication. Registration is paged by
row count, exact wire bytes, canonical work and reference count.

Source review found two avoidable SQL costs: transfer progress updated beside a
large metadata BLOB, and latest-per-slot candidate selection repeated for every
page. The correction separates small transfer progress and builds one bounded
scoped SQL scratch selection for a hydration walk. Count diagnostics and source
bounds are recorded at their actual identities; no timing or physical memory
PASS follows from those changes. A pinned current schema fingerprint prevents
same-version incompatible metadata admission.

SQL COMMIT now has one attempt and no implicit rollback after an unresolved
outcome; the engine is quarantined. Covered registration/owned-admission errors
explicitly abort once and export definite refusal only after known ACK. Other
uncertain paths remain conservative. The genuine COMMIT-error OS diagnostic below qualifies one exercised failure;
failed-abort physical qualification is NOT_RUN. Complete physical attribution
remains INCOMPLETE.

Public SDK Exec -> real FUSE -> shared engine -> strict C1/C2 -> C5 remains
NOT_RUN. Re-reading #296 on2026-10-02 found no published S2 checkpoint; its body
still records uncommitted/unverified local S2 work. The actual published parent
creates per-Workspace metadata databases and retains engine ownership across
Commit construction. SP1 does not import foreign dirty S2 source or label this
component Engine fixture as the fourth public witness. Dependent routing stays
off. Narrow stride10/3/1 remains NOT_RUN until functional/public qualification
and prospectively registered provider/resource accounting.


L2 component composition and causal failures:

- c001 setup FAIL: a200000B fixture write exceeded the existing128KiB callback
  admission. The fixture writes the same bytes in admitted chunks; no bound or
  workload is reduced.
- c002 FAIL: bare-C2 preload discarded the sealed direct-reference facts.
  Immutable-use refusal was correct. A separate explicit empty-namespace import
  reads the independent producer sidecars and inode-value roots, and registers
  in producer ordinal order after complete body/group admission. Product source
  and all oracle bytes remain unchanged.
- c003 FAIL at its declared complete-stream root: actual edited chunked root
  `62f38d975e15e1a3f398bc36af9f8994f19f67632dc329cb996fd3c62f0c3a14`
  differs from the complete-stream oracle root
  `ccead465af722b1f8f26e4fa8dc4db5ac586a0bd51c3f589320d246664b19506`.
  A read-only diagnostic reconstructed all200000 bytes with mismatch_count0 and
  SHA256`e720f87dd8b98b622909be70045cb8d4a1f47898df97bd3e438345f9ae22e318`.
  A separately linked unchanged-parent apply_edits diagnostic produced the exact
  same105B canonical object and edited root. This is an operation-oracle mismatch,
  not a storage fallback or an excuse to change C1. The original stream oracle
  and c003 FAIL remain immutable.
- New edit-oracle-v1 is independently sealed from archived unchanged parent
  before c004 use. Its three operations are initial stream construction,
  overwrite64..96 plus append7a, then boundary truncate plus duplicate file.
  Its manifest is SHA256
  `243ddb6944825f3018a1d8997823097ed46a4e05f1aca2049ec4b22643fdfdd0`.
  Original360 product inputs match the archived parent;73 retained files pin
  complete canonical emissions, reference sidecars, roots and full-byte parity.
- c004 PASS at the edit-operation profile: real authenticated native catalog,
  actual MinIO payloads and C5 conditional commits reconstruct all three edited
  states and retain all three roots after both catalogs reopen. Canonical roots:
  `f7bb395fe7359ddf87221a3928ef861d6ba0bc9e564727f15c79f1da38343461`,
  `b634e9244a0bd690df9157bfd7bd4561b7a089cff936648cae6aa3982b41119c`,
  `e9d4d759c1d4d1b80b4ab4de4878d8cd26af19bd4d57cd09a475cc95b3607c8b`.
  All old logical raw vectors still match, all inode-value reference uses and21
  inode edges are checked, and SQL has no payload shadows. This explicit Engine
  fixture has no SDK Exec, real FUSE, S2, physical resource or speed qualification.

The OS COMMIT-error witness ran once from its source-matched binary: child-only
RLIMIT_FSIZE at the existing file length, unchanged MEMORY/OFF and2MiB pager,
1MiB metadata allocation. It returned actual`SQL COMMIT: disk I/O error`, then
normal reads/writes refused engine access. Prior state and every failure artifact
are retained in [strict-sql-commit-fault-r001](evidence/strict-sql-commit-fault-r001/manifest.json).
A separate read-only observation found the previously acknowledged body and
publication1. It supplies no recovery/adoption or crash-durability claim.

Owning verification so far: Core locked all-target test759PASS/10ignored,
all-target Clippy-Dwarnings, fmt, boundary359 and guard self-tests9 PASS.
Adapter all-target r001 failed to compile a test-only unsigned SQL decode;
r002 then passed15 catalog,9 publication, pending reuse and three SQL-window
checks, but failed a newly authored noncanonical180-entry C1 leaf fixture.
The repaired fixture keeps183 children, increases reference edges to1024 in
8 valid128-entry leaves, and its exact failed-test covering command PASS checks
3 requests of13261/13261/8855B. Remaining writer routine target has5 deliberately
ignored real-provider cases. All-target Clippy initially found a range-iteration
lint in the new integration fixture; its covering all-target command and fmt
PASS after the fixture-only correction. Original failures are retained; passing
unchanged targets are not repeated to select a more convenient observation.


Final L2 frozen-profile matrix:14/14 once (reader3, writer5, refusal6),
[receipt](evidence/strict-current-provider-r005/manifest.json) SHA256
`7d990d5cc16009e36f8a1b0d04661b85a41093b82b54dc4dd9adea08474a5e6a`.
Its427-file runtime inventory is SHA256
`340748251ac70a32e6f47a3295d569c821347f96f6ada0ac9478aaefa63a592d`,
checked unchanged during all proofs. New c004
[receipt](evidence/strict-capture-c004/manifest.json) SHA256
`87bd758fcff5ec0870ee42a41ad0f5de3dca991e0d394aa43930bb64a9734b1f`
qualifies the edit-operation component only. Prior reader/writer/refusal receipts
retain their original source, profile, failures and status; this new matrix does
not promote them.

Targeted final Clippy/compilation for the changed integration/helper and provider
test targets PASS. The new independent edit acquisition's targeted current Core
compilation/Clippy/fmt PASS; earlier owning Core checks cover unchanged product
source. Original archived acquisition Clippy and execution PASS independently.
There is no aggregate preflight/CI and no numeric speed or resource admission.

[Cleanup](evidence/strict-provider-cleanup-r001/manifest.json) PASS: exact owned
MinIO container and bind mount verified, container stopped/removed. Owned bind
data, immutable binaries, databases and all failures remain retained. No foreign
worktree, target, run, provider or source was changed. Credentials remain outside
tracked evidence.

Final dependency inspection2026-10-02 found #296 OPEN with no comments/published
S2 completion checkpoint. Public enablement and fourth SDK/FUSE witness remain
NOT_RUN; the component fixture does not complete that gate. Next: integrate a
published S2 captured scoped view and authenticated Workspace dispatch, then run
the fourth public witness at matched identities. Admit/attribute simultaneous
owners before physical qualification, and prospectively register changed-storage
stride10/3/1 accounting before collection. These gates, failed-abort physical
proof, C1 draft accumulation, non-pausing/concurrency, import/GC/cutover and cloud
remain open. No merge, issue closure or release admission is claimed.


L2 exact first-parent/staged-source comparison:
Production LOC:135991 ->135991 (delta +0). Reference65417 ->65417 (+0);
Core70574 ->70574 (+0). Same tools/production_loc.py SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`
on Git archive snapshots of the exact first parent and staged tree; shipped SQL
included and inline legacy tests excluded. The strict unshipped research runtime
is separately counted in the identical strict_* Rust/SQL scope:1593 ->5955
(delta +4362). Modified existing runtime support is disclosed separately at the
same before/after scope:540 ->652 (delta +112), including construction, MinIO,
its new typed failure module and library declarations. This expands disclosure
of existing support rather than changing product classification. Added runtime
is not a relocation, legacy retirement or algorithmic simplification.23 strict
Rust/SQL/failure files obey the999-line ceiling (maximum908); library67 <=200.

Counter reproduction: Git archive each identity for crates/, core/crates/,
tools/production_loc.py and core/benchmark/phase6-live/src/, then use scan() for
product totals and counted_lines() for the declared experimental/support lists.
The exact staged/committed tree and source-subtree confirmation are included in
the published issue checkpoint. Test, evidence, documentation and target output
never contribute to the production headline.
