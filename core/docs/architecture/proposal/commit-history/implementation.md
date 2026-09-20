# Pair 2 implementation specification — history through the service

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Parent design: [#180](https://github.com/Ephemeral-AI-Lab/layerfs/issues/180).
Implementation tracking: the implementation sub-issue of #180 links this spec
and its [pre-publication audit](review-20260921.md).
This document specifies work to implement; it does not claim implemented history,
runtime qualification, release admission or measured performance.

## 1. Decision and inspected baseline

Implement C5 history independently through the existing pair-3 service, then let
pair 1 consume its public operations. No Workspace or FUSE implementation is a
prerequisite. This is the current implementation recommendation, replacing the
earlier proposal to finish pair 1 before beginning history; pair numbers label
responsibilities rather than implementation order.

Source reviewed: `a02168adbb1b02571941654919cefca12dbc1f42`, containing the landed
[multi-writer implementation](https://github.com/Ephemeral-AI-Lab/layerfs/commit/eb319aaa9ef196358035e6af86066e51b2bdf826).
Reference history was compared with tag `v0.1.6`, commit
`44cf748486863ab7c21ca47e731bd88e2b9a7b4a`; its inspected history source is unchanged
at this baseline. Historical receipts are not promoted to C5 evidence.

The baseline supplies C1 public content/filesystem APIs, C2 schema 7, two private
saves per Store and two admitted service operations in total, including reads.
Q=0 and one construction producer per ordinary operation remain; namespace init
retains its existing exception. C2 has six application
tables: `store_policy`, `saves`, `object_packs`, `objects`,
`metadata_value_groups`, `content_signatures`. No C2 schema change is required here.

Source anchors, relative to this specification:

- [C2 save completion](../../../../crates/layerfs-storage/src/cas/store.rs),
  [ownership/publication](../../../../crates/layerfs-storage/src/sqlite/ownership.rs),
  [schema](../../../../crates/layerfs-storage/sql/schema.sql).
- [Service ownership](../../../../crates/layerfs-service/src/owner.rs),
  [write composition](../../../../crates/layerfs-service/src/operation/write.rs),
  [filesystem boundary](../../../../crates/layerfs-service/src/operation/filesystem.rs).
- [Bridge request contract](../../../../crates/layerfs-bridge/src/contract/request.rs).
- [Reference identities](../../../../../crates/layerfs-layerstack-store/src/ids.rs),
  [schema](../../../../../crates/layerfs-layerstack-store/sql/schema/v10.sql),
  [Commit](../../../../../crates/layerfs-layerstack-store/src/workspace.rs),
  [Layer publication](../../../../../crates/layerfs-layerstack-store/src/layerstack.rs).

## 2. Component boundaries and environment independence

```text
Non-FUSE test client now              Workspace / FUSE later (#179)
              |                                  |
              +---------------+------------------+
                              |
                 Existing bridge logical contract
                              |
                 Service: authorize/admit/compose
                       /                 \
                      v                   v
                C1 + C2 pipeline       C5 HistoryCatalog
                validated roots       metadata transitions
                      |                   |
                      v                   v
                C2 schema 7           C5 catalog schema 1
                six tables            seven tables
```

Add one `layerfs-history` crate. Its portable contract uses typed IDs, bounded
records, explicit inputs and typed outcomes; a native SQLite module implements
the atomic metadata boundary. SQLite is optional at the crate boundary. Use the
existing published dependency versions and ordinary functions internally.

C5 may reuse supported C1 identity/profile scalar types and existing BLAKE3, but
has no dependency on C2, service, bridge, daemon, FUSE or an executor. Service
converts between wire DTOs and history types. Bridge does not depend on the C5
SQLite implementation. No generic SQL callback, plugin registry or algorithm
factory is introduced. Keep one C1 package identity through dependency selection.

C1 retains canonical identity, CDC, tree construction and validation. C2 retains
deduplication, physical delta selection, packs, publication and cleanup. C5 must
not copy those algorithms or interpret physical `save_id`, locator or pack fields.
The service owns authenticated principal, logical Store/catalog mapping, admission
and composition. Workspace later owns live bytes, edit intent, handles and local
generations. A Workspace ID in a stage is an opaque producer incarnation.

Native paths, file descriptors, sockets, clocks, PIDs and environment variables
stay in adapters. Semantic APIs do not require a permanently open connection or
live Workspace. Current C2 native arbitration is Unix-specific; Linux/macOS
composition is the initial supported profile. A no-native-SQLite build proves
contract separation, not working Windows/WASM/cloud persistence. A future provider
must prove atomic transitions, bounds and its continuity capability explicitly.

## 3. History entities and relationships

| Entity | Meaning and invariant |
| --- | --- |
| LayerStack | Named linear Layer publication timeline with one genesis and one conditional head. |
| Branch | Belongs to one stack; stores a base Layer and optional head Commit. Effective root is head Commit root, otherwise base Layer root. |
| Commit | Immutable `(root, parent Commit, base Layer)`. Parent means recorded ancestry, not a diff or successful conflict resolution. |
| Layer | Immutable full-root publication in a stack, with parent Layer and source Branch/Commit provenance. |
| Workspace stage | Saved candidate plus immutable expected publication context. Neither a Commit nor a Layer, and unrelated to a C2 private save slot. |

Preserve reference typed-ID encodings and domain-separated Commit/Layer derivation
where the semantic inputs agree. Stack/Branch IDs and Workspace incarnations are
explicit checked inputs from the application authority; no `/dev/urandom` with
time/PID fallback in C5. Exact duplicate derived IDs must match every immutable
field, including provenance omitted from the hash, or fail with integrity error.
Catalog IDs/Store bindings are distinct from those reference IDs. Exact binary
encodings and tag fixtures are frozen in M0 before implementation.

```text
LayerStack S                         Branch A
+--------------------+              +----------------------+
| head_layer = L0    |              | stack = S            |
+---------+----------+              | base_layer = L0 -----+--+
          |                         | head_commit = K2     |  |
          v                         +----------+-----------+  |
    L0: root R0 <---------------------------------------------+
    parent=None                                |
                                               v
                           K2: root R2, base L0
                                  |
                                  | parent
                                  v
                           K1: root R1, base L0
                                  |
                                  v
                                 None

Workspace stage W:
  token=17, branch=A, expected=(K2,L0,R2), candidate=R3
```

```text
commit_staged(17)                     add_layer(A,K3,expected=L0)
       |                                           |
       v                                           v
Insert K3(root=R3,parent=K2,base=L0)   Insert L1(root=R3,parent=L0,source=A/K3)
CAS A.head K2 -> K3                   CAS S.head L0 -> L1
Delete exact stage 17                A.head and A.base unchanged

K3.root == L1.root == R3; no filesystem content is copied or re-encoded.
```

Fork from a Layer sets new Branch base to that Layer and head to None. Fork from
an authorized ancestor Commit sets base to the **selected Commit's base**, head
to that Commit, and shares all selected ancestry. The union of Branch histories
can fork; each Commit has one parent. History ancestry checks may walk rows;
they are not claimed O(1). Layer publication performs bounded metadata operations
with zero content traversal, not a measured latency guarantee.

```text
                    K1
                     ^
                     | parent
                    K2
                   /  \
            parent/    \parent
                 /      \
               KA3      KB3
                ^        ^
                |        |
            Branch A   Branch B
```

## 4. Supported operations

These are semantic operation names, not existing methods or shell commands.
Queries and history-only mutations never acquire a C2 save slot.

| Operation | Input and successful result | Conditions / effects |
| --- | --- | --- |
| `init_layerstack` | Checked stack ID/name plus Empty or bounded logical namespace manifest -> stack/genesis IDs and root descriptor | Service allocates a fresh scope, builds/saves through C1/C2, then atomically creates genesis Layer and stack. No automatic Branch, stage or Commit. |
| `fork` | Source Layer, or selected Commit with source Branch/anchored ancestry, new Branch ID/name -> coherent Branch snapshot | Validate authorized membership and same stack; share content and history. |
| `read_branch` | Branch ID -> stack/base/head/root/profile/scope/root-serial descriptor | Coherent metadata snapshot, then service validates relevant root descriptor through C1 outside catalog lock. |
| `stage_changes` | Workspace incarnation, Branch snapshot, frozen bounded prepared changes and generation -> exact stage token and candidate descriptor | Build from explicit base, finish C2, then insert authoritative stage. No arbitrary caller assertion of saved-root validity. |
| `commit_staged` | Workspace incarnation + exact stage token -> Committed or UpToDate | Validate expected head/base/root; insert Commit + conditional Branch advance + stage removal in one catalog transaction. |
| `commit` | Same input as stage_changes -> Committed/UpToDate or typed result retaining a stage | Convenience composition of stage_changes and commit_staged, sharing bodies and one service admission. Not one cross-database transaction. |
| `add_layer` | Branch, exact expected head Commit and expected stack head -> Added/UpToDate/NoChanges | Validate source; insert Layer with Commit root + conditional stack advance atomically. Branch unchanged. |
| `discard_stage` | Workspace incarnation + exact token -> Removed or Absent/StageChanged | Remove only the matching stage. No content/history deletion or serial recycling. |
| `reserve_inodes` | Scope + checked bounded count -> reserved half-open range | Continuing-authority capability required; consume reservation before exposure. Does not itself enable remote create/mkdir. |
| `get_*`, `list_*`, `commit_history`, `layer_history` | Explicit identities and bounded page request -> records/page | Include stack/Branch/Commit/Layer/stage inspection; no unbounded materialization or false absence on work-limit exhaustion. |

### Initial namespace construction and ingestion scope

The landed service has no init/import route. Its `prepare_store` example is a
fixture and cannot satisfy this operation. Add a production bootstrap handler
using public `build_filesystem`/timed equivalent, portable attribute construction,
typed file inspection and symlink construction where applicable.

The initial manifest is pathless and bounded by the existing 32 KiB metadata
envelope and at most 128 entries including root. It names parent/entry indexes,
canonical component names, kinds, portable mode/mtime and already published file
roots; symlink targets are bounded values. Service assigns all inode serials from
one fresh scope reservation. No raw caller inode serial or foreign scope import.
Empty is the one-directory case. Large/host-tree import and arbitrary existing
filesystem-root registration are unsupported in this issue; reject them before
construction rather than introduce host scans or a second import algorithm.

Prerequisite file roots are saved through existing ConstructFile/EditFile or
validated trusted content ingestion. Build attributes/symlinks, finish their save,
then build/save the filesystem and create history. Preserve ordinary ownership,
bounded batches and each completion; no combined unpublished reader/sink is
assumed. Earlier successful saves may become unreferenced on later failure.
Initialization retains its namespace-init worker exception; this spec neither
adds workers to ordinary operations nor introduces a performance claim.

`stage_changes` initially supports the landed existing-inode prepared update
surface, with existing changed-name/inode limits (128 of each). New-inode updates,
arbitrary metadata/xattr changes and new symlink creation for a live Workspace
need later explicit service extensions under pair 1; they are not silently
enabled by reserve_inodes. This issue must not claim complete writable FUSE.

Source-backed validation gap: the current filesystem handler authenticates supplied
content/metadata bytes but does not fully validate their semantic roles. History
ingestion must validate proposed replacements with bounded public C1 file/profile,
portable-attribute and symlink validators, or require unchanged trusted base
metadata for unsupported variants. A stored wrong-role root must be rejected.
This is boundary validation, not a full namespace traversal per history Commit.
Direct C5 root registration is trusted-library composition only; do not expose it
as a wire endpoint accepting `root_exists=true` as proof.

### Exact transition and outcome rules

1. Capture/check expected Branch context before construction without retaining a
   catalog lock. A stale request discovered before work may return HeadMoved.
   In this initial profile, construction base must equal the captured Branch's
   effective root and intended Commit base must equal its captured base Layer.
   The service rejects mismatches before C1 work; trusted direct catalog
   composition enforces the same structural equalities. Arbitrary same-scope
   roots or changed expected tokens are not accepted as a rebase operation.
   If the Branch moves during a successful save, stage the structurally valid
   candidate with its original context anyway; subsequent commit returns HeadMoved.
2. Stage insertion requires no existing stage for that Workspace incarnation.
   Allocate token and insert in the same transaction. Replacement requires
   explicit exact discard and a new stage; no overwrite or automatic replay.
3. Commit validates exact stage and expected Branch head/base/root atomically.
   On mismatch, keep the stage. Same candidate root **and intended base** yields
   UpToDate, consuming only the exact stage. Otherwise insert/verify immutable
   Commit, CAS Branch head/base, delete stage and COMMIT. Multiple no-change stages
   can succeed; only one state-changing Commit wins against a fixed head/base.
4. Add-layer order: authorize and validate selected source; exact already-published
   source -> UpToDate; otherwise mismatched Commit base/Branch base/stack head ->
   HeadMoved; otherwise equal Commit/base root -> NoChanges; otherwise insert Layer
   and CAS stack head. Check prior publication before treating its old base as stale.
5. Branch base does not move merely because its Commit became a Layer. Subsequent
   publication needs a new Branch from that Layer or explicitly defined future
   Workspace rebase. Changing an expected token alone never rebases content.

Failures preserve InvalidInput, NotFound, Unsupported, Busy/OwnershipUnavailable,
Capacity, Integrity, HeadMoved, StageChanged, ContinuityUnavailable and unknown
persistence/delivery outcome. Busy is not a semantic conflict. Do not parse error
strings to reconstruct lost error classes. Logical non-success replies carry
typed expected/actual context and retained stage when known. Missing response
does not prove rollback; optional telemetry failure does not change a known result.

## 5. Catalog schema 1

One separate catalog per configured logical Store/authority binding, supporting
multiple stacks. Native configuration supplies a stable binding key and catalog
identity; request Store IDs are routing aliases, not persisted file identities.
C2 has no public persistent Store UUID: correct physical Store selection remains
an explicit operator/service binding, checked against the catalog's configured key.
Root/profile checks alone do not authenticate an operator-selected database file.

| Table | Required columns (semantic names) | Keys and invariants |
| --- | --- | --- |
| `history_meta` | singleton ID, catalog ID/incarnation, stable Store/authority binding, identity-format version, next_stage_token | Singleton; positive checked token counter; no silent reset on reopen/copy. |
| `layer_stacks` | stack ID, name, scope ID, profile ID, head_layer_id | PK stack; unique authority-local name; head belongs to stack; scope/profile fixed. |
| `layers` | layer ID, stack ID, parent_layer_id?, root_id, source_branch_id?, source_commit_id? | Immutable; unique (stack,layer), genesis per stack, child per (stack,parent), source per (Branch,Commit). |
| `branches` | Branch ID, stack ID, name, base_layer_id, head_commit_id? | PK Branch; unique (stack,name) and (stack,Branch); base/head belong to stack; head Commit base equals Branch base. |
| `commits` | Commit ID, root_id, parent_commit_id?, base_layer_id | Immutable; base and parent ancestry in same stack; parent may have a different base after explicit rebase. |
| `workspace_stages` | Workspace incarnation, token, Branch/stack IDs, expected head?, expected base/root, construction base root, intended Commit base, candidate root, profile/scope, input generation | Unique Workspace incarnation and catalog token; structurally checked frozen context; no FK to a runtime workspaces table. |
| `scope_allocator` | scope ID, checked high-water/exhaustion representation, authority association | Unique scope; all shared-scope Branches/forks use it; may exist before/without a stack after failed initialization. |

Use STRICT tables, checked integer conversions and the existing supported SQLite
capabilities. Reference names are 1–63 ASCII bytes, lowercase letters/digits with
`.`, `_`, `-`, alphanumeric ends. Preserve typed ID tag/length checks and reference
Commit/Layer derivation, with frozen fixtures. A genesis Layer has parent and both
source fields absent; other Layers have all three. Source Branch and parent Layer
belong to its stack. Published root equals source Commit root; publication-time
head equality is not a forever constraint after that Branch later advances.

Express relational constraints as foreign keys/unique constraints where possible;
check remaining cross-record invariants inside the transaction. Use deferred
constraints for stack/genesis mutual references. No FK crosses into C2. Two C2
locator rows for one ObjectId do not create two logical roots or history records.

Create writes its own application ID and user_version=1. M0 freezes the unused
application ID, exact column/constraint text and versioned identity format.
Open validates them plus binding/ranges; incompatible or incomplete catalogs fail.
No automatic migration, table repair, old-schema promotion or C2 modification.
Future table changes require an explicit C5 schema version and open behavior.

## 6. Multi-writer, visibility and persistence

```text
Request A                          Request B
capture Branch snapshot            capture Branch snapshot
       |                                  |
C1 -> C2 private save A             C1 -> C2 private save B
       |                                  |
       |                           finish B; publish B's objects
       |                                  |
       |                           short C5 stage/Commit transaction
       |
finish A; publish A's objects
       |
short C5 stage/Commit transaction

No C5 transaction/lock spans either upload, construction or C2 finish.
```

Native C5 has one short transaction at a time per configured catalog authority.
Use immediate bounded admission (`try_lock`/Busy) and one SQLite transaction
attempt, MEMORY/OFF, zero busy timeout, no WAL/sync. Callers can overlap C2 work
while metadata transactions serialize or refuse Busy. Different Branches/stacks
avoid shared logical heads, not SQLite contention. Do not hold C2 arbitration
while entering C5; no lock is nested across the database boundary.

| Concurrent changes | Expected history result |
| --- | --- |
| Same Branch, disjoint files | One state-changing winner; loser HeadMoved with stage retained. No implicit root combination. |
| Same Branch, overlapping changes | Same stale-head result; C5 does not detect/resolve overlap. |
| Different Branches, same stack | Both Branch Commits may succeed; stack publication competes for the shared head. |
| Different stacks | Independent logical transitions subject to bounded admission/metadata locking. |

Ordinary C1 reads use operation-owned StoreProvider sessions; do not share a
non-Sync provider across requests. C2 save IDs, private dependencies, duplicate
locator selection and cleanup remain C2-internal. Its save.finish acknowledgement
is associated by service with the exact constructed root. It does not create a
Commit or certificate. Metadata-only operations never start/abort C2 saves.

The first four persistence boundaries are C2 finish, stage insert, Commit/Branch
transition and Layer/stack transition. Each is separately acknowledged. C2 failure
prevents staging; success then C5 failure can orphan content. Discard removes a
stage only; no GC, content deletion, branch deletion or serial refund. Unknown
outcomes cause no replay, guessed discard or rollback. A machine failure can
leave the two databases inconsistent; resumed reads validate relevant roots and
fail explicitly if content is missing. No crash-atomic cross-store claim.

## 7. Allocation continuity and reopen support

Serials are in `1..=i64::MAX`; reserve a checked half-open range before exposure.
Use checked high-water arithmetic including the terminal endpoint representation.
No fixed `2^32` reservation. All zero-copy forks sharing a scope use one authority.
An allocation is consumed after successful reservation despite later failure,
stage discard or unused numbers. Scanning roots cannot recover exposed unused IDs.

Freeze the first native support envelope instead of an unspecified recovery gate:
fresh catalog creation **inside the owning service process** establishes one
continuing writable service authority; a separate provisioning process followed
by service open does not transfer that authority. Successive requests/connections
share the live authority. In particular,
read-only open/reopen is supported.
Arbitrary process restart, restored/copy catalog or authority loss does **not**
automatically reacquire write authority. Reject all catalog mutation and new
allocation on such an open with ContinuityUnavailable in the initial provider.
No `assume_clean` flag, in-file shutdown bit or root scan establishes continuity.
Sharing/cloning the still-live authority handle is not a new independent writer.

This permits tests that stage on one connection and commit on another while the
same service authority continues. It deliberately does not promise writable
service restart. A trustworthy external continuity handoff is a separate future
provider capability. Neither added fsync/WAL nor silent fresh-scope rollover is an
allowed workaround. Re-scoping inherited roots changes inode identities and is
not zero-copy recovery. Keep this limitation in public errors and the issue.

## 8. Service/bridge contract and later pair 1

Reuse the existing authenticated handler, admission, deadlines, bounded delivery
and telemetry. Composite commit acquires one service slot and invokes shared
production bodies directly; never recursively call Service::handle. A client
supplies stable complete input; no automatic mutation replay/queue/rebase.

The current service Grant.operations is u8 and authorization shifts by opcode.
Use **two grouped opcodes**, 6=HistoryQuery and 7=HistoryCommand, with closed
typed suboperation enums, preserving legacy 1–5 permission bits. Existing grants
such as mask 31 grant no history permissions. A HistoryQuery grant permits only
queries; HistoryCommand permits the declared mutating history operations within
that configured Store/authority. Initial authorization is Store-wide, not a
per-Branch ACL claim. Validate catalog/stack/Branch/stage membership on every
suboperation, and allow no arbitrary SQL or caller-asserted principal.

Use operation profile 2 for history requests and replies; retain profile 1 for
legacy operations. Keep current HELLO/framing separate from the operation profile.
M0 freezes payload/result tags and checked permission mapping before codecs are
written. Updated peers reject unknown profile/suboperation combinations before
mutation. Older peers must refuse history requests; no auto-downgrade or replay.

Replace `Operation::mutation() == opcode >= 3` with exhaustive semantic matching:
HistoryQuery is read-only, HistoryCommand mutates; distinguish content mutations
from metadata-only mutations in service dispatch. Update codecs, validation,
failure decoding and native-client request/result matching together. Preserve
known rejection versus unknown mutation outcome. No second daemon parser: reuse
its generic framed relay and add an external driver for the new operations.

History metadata fits the existing 32 KiB request envelope; default/list maximum
is 128 records and 16 KiB encoded result per page. A lineage-membership query has
an explicit 4,096-row work ceiling, independent of C1 traversal budgets; exhaustion
is Capacity/Unproven, never NotFound. Freeze continuation encoding and validation
in M0: it binds catalog, query/Branch, immutable anchor and next record. Invalid
incarnation/anchor/context or tampered cursors fail; advancing the live Branch
does not invalidate an otherwise valid cursor anchored to its old immutable head.
There is no process-local cursor registry. Totals, tags and
arithmetic are validated before allocation. Do not enlarge service W=2, frame,
replay, worker, timeout or cache limits to pass an acceptance case.

Later Workspace uses read_branch -> local edits/reservations -> stage_changes or
commit -> add_layer. It distinguishes a saved working root from committed Branch
state, retains bounded input/edit intent until outcomes are known, and never
clears a newer generation on an older reply. New-inode/metadata service extensions
and explicit rebase remain later pair-1 work; diff/conflict resolution is #164.
Connection close/unmount does not implicitly discard a stage.

## 9. Files, folders and production LOC plan

Numbers are nonblank/non-comment first-party production Rust + runtime SQL
planning allocations, not measured LOC or caps. Tests/docs/manifests are excluded.

```text
core/crates/
  layerfs-history/                         NEW
    Cargo.toml
    src/
      lib.rs                               20
      identity.rs                         220
      records.rs                          220
      catalog.rs                          200
      error.rs                             90
      sqlite/
        mod.rs                             15
        open.rs                           220
        rows.rs                           200
        branch.rs                         220
        staging.rs                        180
        commit.rs                         220
        layerstack.rs                     230
        allocation.rs                     130
        query.rs                          180
    sql/schema-v1.sql                     160
    tests/                                excluded from production LOC
      lifecycle.rs
      conditional_updates.rs
      allocation.rs
      reopen.rs
      history_pages.rs
  layerfs-service/                        EXTEND
    src/operation/history.rs              350
    src/operation/history_bootstrap.rs    400
    src/owner.rs, operation/{dispatch,failure}.rs,
        native/{config,startup}.rs        150 combined change budget
    tests/history.rs
  layerfs-bridge/                         EXTEND
    src/contract/history.rs               250
    src/contract/{request,outcome}.rs     120 combined change budget
    src/adapters/native/protocol/
        {metadata,response,state}.rs     230 combined change budget
    src/adapters/native/client.rs         50
    tests/history_protocol.rs
  layerfs-daemon/tests/history_route.py    external driver, not product LOC
```

| Scope | Central allocation | Expected range |
| --- | ---: | ---: |
| New history crate including SQL | 2,505 | 2,000–3,100 |
| Service extensions including real bootstrap/ingestion validation | 900 | 650–1,100 |
| Bridge contract/codecs/classification | 650 | 500–850 |
| Combined production change | 4,055 | Planning envelope **3,500–5,000** |
| External tests/helpers/drivers | Excluded | 1,800–3,000 |

This revises the earlier rough 3,000–4,500 estimate to include bootstrap,
role-validation and protocol/permission work found in audit. It excludes general
host import, full writable Workspace, managed provider, GC and recovery service.
The per-component ranges are approximate and not independent hard budgets.

Use existing dependencies, no vendor/fork/registry edits, locked builds. No new
files without real code. lib.rs/mod.rs are declaration/delegation-only and at most
200 physical lines; other production files at most 999. Tests stay outside src/.
If a codec/handler approaches the ceiling, split by responsibility rather than
compressing code. Every commit reports exact first-parent/staged/committed LOC
with separate reference/core totals under the repository's counter policy.

## 10. What is retained and simplified versus v0.1.6

Keep the LayerStack/Branch/Commit/Layer semantics, identity derivations,
single-parent history, zero-copy forks, two conditional head publications and
scope-wide no-recycling policy. Root-only Layer publication already exists in
the reference; do not claim it as a new optimization.

Separate C5 from the reference Store's packs, content admission, construction and
native runtime assumptions. Service composes public operations. Frozen stages
remove a live-Workspace dependency, with more explicit metadata. Separate schemas
trade SQL FKs across content/history for explicit service validation and separate
failure boundaries. This is responsibility simplification, not proof of a smaller
equivalent whole product. The six C2 plus seven C5 tables exceed the reference's
nine; table-count reduction is not the objective. Reference allocator FULL sync
is not carried over; the narrower continuity support in §7 is explicit.

## 11. Milestones and acceptance

| Milestone | Deliverable / exit condition |
| --- | --- |
| M0 contract freeze | Pin source/lock/toolchain; freeze ID fixtures, catalog application ID/DDL, grouped operation tags/profile/permissions, page/cursor format, bootstrap manifest and limits, writable-authority lifecycle. No implicit defaults remain at trust boundaries. |
| M1 catalog | Implement real crate/schema/open validation, read-only reopen, IDs, Branches, genesis, allocation and bounded queries; external tests on public APIs. |
| M2 transitions | Stage/exact discard, Commit/Branch CAS, add-layer/stack CAS; no-change/outcome precedence, ancestry and constraints verified. |
| M3 service + bridge | Production Empty/manifest init, stage/commit composition, explicit metadata-only dispatch, role validation, authorization, codecs, failure/result matching; direct test client works without Workspace. |
| M4 multi-writer / deployment | Same handlers through native transport and actual Linux Docker daemon -> service; tests below, finite resources and optional telemetry through real bodies. |
| M5 review/handoff | Independent audit, exact validation and compatibility results, LOC accounting, source-backed architecture docs and pair-1 contract handoff; no #180/#192/#193/#179 closure by implication. |

Required public behavior checks (all NOT_RUN for this specification):

| ID | Acceptance |
| --- | --- |
| H01 | Empty and bounded manifest init through production service; invalid/dangling/wrong-role input refused; no fixture-only implementation. |
| H02 | Layer fork and historical-Commit fork share roots/ancestry; selected Commit base used; over-budget ancestry is not absence. |
| H03 | Save -> stage -> Commit -> add-layer -> readback; immutable IDs, parent chain, exact root reuse and separate heads. |
| H04 | Actual service overlap preserves publication and cleanup isolation; third operation/save admission remains bounded. A reverse-completion claim requires observed overlap of real C2 save lifetimes, then B completion before A and survival of B's content/stage after A failure. See proof rule below. |
| H05 | Same Branch stale loser retains stage for disjoint and overlapping edits; no implicit merge/retry. Multiple no-change stages can return UpToDate. |
| H06 | Different Branches commit independently; same-stack publication has one expected-head winner; independent-stack work does not hold A's upload behind a catalog lock. |
| H07 | Delayed commit/discard token cannot consume a replacement stage; exact already-published source checked before stale-base refusal. |
| H08 | Failure/connection loss at every save/history boundary preserves known/unknown outcomes; completed roots may orphan; telemetry failure never rewrites success. When A and B produce the same ObjectId, B's publication cannot upgrade A's failed/unknown finish into acknowledged staging. |
| H09 | Stage on one connection, inspect/commit/discard on another with same live authority and no connection-local state. Read-only reopen works; writable restart without continuity fails. |
| H10 | Scope shared by forks, concurrent reservations, failure/discard, range overflow/exhaustion and catalog restoration never recycle exposed IDs within support envelope. |
| H11 | Wrong Store/catalog/scope/Branch/stage, caller-asserted authority, unknown opcode/profile, wrong result and malformed counts rejected; legacy mask31 grants no history. |
| H12 | C5 metadata-only transitions make no C2 save/object write, do not re-encode filesystem content or select physical delta bases; scope/profile mismatches are refused. |
| H13 | Bounded listing/cursors and byte limits; core/no-native backend build; Linux/macOS native route; cloud/Windows-native support remains unclaimed. |
| H14 | Unchanged external consumer across supported compatible C1-only, C2-only and combined revisions; canonical/API/schema compatibility reported separately. Reuse existing substitution harness where suitable. |

H04 must not infer save overlap from submission order, delayed END_INPUT or
sleep timing: the prepared-update handler consumes complete metadata before
begin_save. Use production observations and real bodies. If that bounded route
cannot establish the exact reverse-completion schedule, mark that proof NOT_RUN;
report the existing C2 W2 proof separately from actual service/history checks.
Do not add product fault hooks, delays or alternate algorithms solely for the
test, and do not silently count the missing schedule as qualified integration.

Before implementation handoff run the explicit core workspace checks:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace
cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --workspace --examples
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace --all-targets -- -D warnings
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

Add the explicit no-native feature check when the crate exists; preserve test
discovery. No CI/aggregate preflight is introduced. Existing C2 writer receipts
do not replace H04–H09. Any resource/performance qualification follows repository
measurement locks, cold-state rules, identity-matched reuse, complete-command
budgets and append-only receipts. Do not rerun or retune unrelated performance
work, claim a speedup, or alter #209's transaction-cadence investigation here.

Out of scope: FUSE/Workspace implementation, auto-rebase/retry, diff/conflict
resolution (#164), GC/retention policy, reset/force-push, arbitrary root/host import,
transparent schema migration, cross-store atomic crash recovery and stronger
durability. Every absent proof remains NOT_RUN or explicitly blocked, never PASS.
