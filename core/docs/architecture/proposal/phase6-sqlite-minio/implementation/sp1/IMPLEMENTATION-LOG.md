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
