# Directory entries and operation records

> **Status:** Implemented terminology and schema16 with scoped host/Linux
> proofs after `a40acd673`; no table-layout or performance claim.
> Owner instruction dispatched from side chat `01a1152b-e93c-72b2-bac1-4d5b210d2830`.

The owner selected `dentry` → `directory_entry` and operation-owned `scratch` →
`operation_record`, including consistent tables/files/types/fields/methods and
related indexed/owned variants. This authorizes naming only. Existing physical
tables remain shared by Workspaces and partitioned by namespace; no per-Workspace
physical table redesign or implicit migration is introduced.

## Deepest-file plan and semantic scope

[Receipt01](checks/pre-s8-terminology-20261007/01-file-plan.json) enumerates the
actual affected active product/test/example files before editing.

- Overlay SQL: rename dentry and its indexes/triggers/prepared identifiers to
  directory_entry; scratch/owned_scratch/indexed_scratch become operation_record,
  owned_operation_record/indexed_operation_record. Keep columns, keys, predicates,
  payload/namespace ownership, bounded windows and cleanup algorithms unchanged.
- Overlay source: rename record/directory types, method/variable identifiers,
  diagnostics statement families and maintenance kinds; move affected contract,
  lifetime, maintenance and diagnostic module filenames. Related generic Indexed
  types become explicitly IndexedOperationRecord{Scope,Key,Change,Apply}; the
  expected-value type and byte accounting function name the same record concept.
- Workspace ports and construction: OverlayOperationRecords and
  OperationRecord{Reply,Apply,Copies,Refusal,InputRefusal}; rename the port module
  and calls while preserving original completion/input custody and copied bytes.
- Daemon jobs/service: IndexedOperationRecord{Job,Reply}, OperationRecord service
  class and renamed port module; same lane order, credits and routing. Existing
  tests/examples compile under the new names; E04 is not rerun.
- Content: only the backed file editor's stored-reference error denotes an
  operation record. Its generic heap sorting/decoding scratch byte budgets are
  unrelated and retain their terms. Native Bridge crypto/frame scratch buffers
  and host/benchmark temporary scratch directories also retain their terms.
- SQL schema identity advances15→16. Overlay always creates a fresh owned file
  using create_new; no reopen/migration/fallback is added. Update readback and
  tests and prove the actual table/index/trigger vocabulary in a fresh database.
- Update current architecture/API descriptions and #303 target contracts with
  the new concept names and owning source paths. Dated receipts, source-pinned
  historical reports and root/excluded reference implementations keep original
  vocabulary. Add a current terminology record to explain that distinction.
- Extend boundary coverage for the selected active concepts; preserve generic
  temporary-buffer terminology. No production behavior switches or new deps.

Verification: build all affected package targets first, then scoped engine,
indexed-record/cursor, ownership/cleanup, daemon root qualification and Workspace
construction proofs, host before pinned Linux. Each test invocation is bounded
by100s and serialized; no E04, Durable, benchmark or timing campaign. Final
Clippy/fmt/boundary/self-tests and exact staged LOC go in one naming commit.

Related binding inventory in receipt02 also covers NameChange/NameWindow,
source_names/name_window, SourceNames/Response::Names, changed-binding vectors,
and name_rows/dirty_names counters. These become directory-entry terms because
they carry bindings, including removals. Bare component names (PathName, `name`
columns, name-only test listing helpers) retain their meaning. The old E04
receipt parser in benchmark evidence_jobs.py keeps its frozen source/schema
vocabulary for historical receipts; it is not a new product invocation surface.

## Completed scope and evidence

The source now uses DirectoryEntry, directory_entry SQL names, and the
OperationRecord/Owned/Indexed forms throughout the affected Overlay, Workspace
and Daemon files/APIs/counters. DirectoryEntryWindow remains distinct from a
plain DirectoryEntries row vector. Generic names/PathName, kernel terminology,
heap/crypto buffers and filesystem temporary directories retain their own terms.
The [current terminology guide](../../architecture/64-overlay-terminology.md)
describes these distinctions. Thirteen Rust source/test/example paths moved;
the49 architecture document moves to the current record title with a historical
link stub. Original E04 receipts/parser vocabulary remains frozen; it was not run.

Receipt12 proves both shipped Overlay SQL files equal the parent byte-for-byte
after only explicit table/column/index/trigger names and schema15→16 substitutions.
Namespace keys, predicates, types, limits, triggers' arithmetic and table layout
are unchanged. Fresh creation still uses create_new and refuses existing paths.
There is no migration/reopen/retry and no per-Workspace physical table redesign.

Receipts04/05 retain build failures: a prefix replacement accidentally touched
Namespace, two response shapes initially collided, and one external fixture
still used the old qualified method. Source inspection corrected the exact
symbols and preserved the two response shapes before successful all-target
host build06. No failed functional body occurred. Receipts17/18 both returned
success but overlapped because the second command launched before the first
exec session had been confirmed complete. They are explicitly **INELIGIBLE**
for the required serial invocation discipline; receipt19 retains timing/order
facts. Isolation was corrected, then20/21 ran each affected functional binary
once with confirmed completion before the next command. No speed claim or
best-of selection uses either run.

Receipt24 pins82 product/test/build inputs and their byte-identical refresh.
Linux hash guard25 detected stale views of seven test files before Cargo started.
Read-only diagnostic26 then observed all82 exact hashes; successful build27
checked equality again. These are retained as stale-mount evidence, not Rust
source failures or test hangs. All twelve Linux proof binaries28–39 execute
serially on that final source. Each body group passes and finishes below3s,
within the independent proof budget; all carry100s external wall stops.

The proof index44 retains every binary/receipt hash and lists68 eligible bodies
per platform: Overlay engine12, compound3, indexed records6, indexed cursor2,
source ownership4; Daemon owner9, indexed records2, root qualification1,
construction cursor3, edit backing4; Workspace real-daemon namespace8 and
captured file edits14. The whole-root qualifier still accounts2022 inodes,
2040 bindings,2080 record reads and82 batches=2162 owner jobs with the same
bounded copy/reply observations. Indexed plans correlate with executed point,
window and cleanup counts; no tree or payload copy was introduced by naming.

Host/Linux builds use locked Cargo1.85.1, repository ARM64 inputs,
`LAYERFS_CONSTRUCTION_WORKERS=1`, and the pinned Linux image
`sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
Overlay stays MEMORY/OFF/EXCLUSIVE on local disk. Linux databases stay under the
container filesystem, not `/work`. Cache state is not a cold performance claim.
No new Durable operation, E04 execution, benchmark or native >4GiB-file case ran.
The existing logical-hole constructor case is not that waived native-file proof.
All functional runs finish with their existing ownership/cleanup checks; no new
terminal unknown arose. Earlier failed fixtures and all old raw receipts remain.

Final checks40–43 pass: scoped all-target Clippy with warnings denied, formatting,
751-file boundary scan and44 guard self-tests. Guard coverage rejects retired
SQL/API concepts while accepting unrelated temporary buffers. Current renamed
link targets exist; source/docs whitespace is clean. This names existing behavior;
F8's remaining Save/history/install composition and later pre-S8 gates remain.

Production LOC:170969 ->171109 (delta+140). Core105552 ->105692;
active62387 ->62527; reference65417, excluded predecessors36325 and excluded
integration6840 unchanged. Receipt45 compares exact parent/staged product trees
with the pinned counter. Growth is declaration/call formatting from the longer
names; this is a naming refactor, not an algorithm or host-transport retirement.
