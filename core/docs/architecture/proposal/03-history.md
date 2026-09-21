# Pair 2 — commit and history implementation design

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
>
> **Investigation, 2026-09-21.** Recommended design, not implemented or qualified.
> Issue: [#180](https://github.com/Ephemeral-AI-Lab/layerfs/issues/180).

Use the reference's LayerStack, Branch, Commit and Layer semantics, with a
self-contained stage and a separate history catalog. The service composes
content construction, physical save and history transitions through public
contracts. History never opens C2's SQLite connection or interprets its packs.

This is a **semantic port with a new integration boundary**, not a source/SQL
port. Implementation remains third, after [pair 3's service/transport and pair
1's Workspace](README.md#implementation-order-pair-3-then-pair-1-then-pair-2).
The older [operational sketch](02-init-commit-and-concurrency.md) is background;
the corrections and proposed semantics here replace its conflicting assumptions.
Names below describe proposed operations, not existing Rust APIs.

## 1. Inspection basis and findings

Inspected HEAD: `10b9d4a6cf9d88267d508cb010cc82950e080d77`, plus the already
modified C1 filesystem update and service/daemon proposals. Three subagents
independently traced reference history, C1/C2 and service/runtime ownership.
Existing working-tree changes were preserved. This is source inspection, not
runtime or performance evidence. Relevant dirty-file SHA-256 identities:

| File | SHA-256 |
| --- | --- |
| `core/crates/layerfs-content/src/filesystem/update.rs` | `8d4762d1750284536f03a066931ac01fe1213aaee2ff273a7ffaaf2cbe98f8a7` |
| `service-daemon-transport/01-architecture-and-portability.md` | `22deb3ab9fa626719ce0cb89727dd51d587d16cbcd4826b328c7f23c01212832` |
| `service-daemon-transport/03-operations-and-transport.md` | `c1cce81e7aeeab54bf0d21c483894ccaa45188bf7e46e4d0bc656593574d6269` |
| `service-daemon-transport/07-public-operations.md` | `5a066b96d1ebda5615a2d8fdfb8ec2f9ed1968a7c62aa2e7537a5b441a620134` |

Service paths above are relative to this proposal directory. These identify
inspected inputs; they do not seal or qualify the changing tree.

| Finding read from source | Consequence |
| --- | --- |
| C2 schema 6 has **five** tables and rejects additional tables. See [SQL](../../../crates/layerfs-storage/sql/schema.sql) and [validator](../../../crates/layerfs-storage/src/sqlite/schema.rs), especially `REQUIRED_TABLES` and its unexpected-table query. | The issue's four-table count is stale. Adding history tables to that file makes current `Store::open` fail. |
| [Workspace publication](../../../../crates/layerfs-layerstack-store/src/workspace.rs) stages a root, retains admission, then inserts a Commit, CASes **Branch** head/base and deletes the stage atomically. | Logical Commit is distinct from object save and Layer publication. |
| [LayerStack publication](../../../../crates/layerfs-layerstack-store/src/layerstack.rs) subsequently inserts a Layer using the Commit root and CASes **LayerStack** head. | Branches have separate branch heads but compete for a shared stack head. |
| [Reference stages](../../../../crates/layerfs-layerstack-store/sql/schema/v10.sql) store only workspace, branch and root; expected head/base/root stay in the caller. | Add frozen context and an exact stage token, independent of live Workspace memory. |
| [C2 `SaveOutcome`](../../../crates/layerfs-storage/src/cas/store.rs) carries no root or Store identity. | The service must bind the exact constructed root to its successful save. |
| [Save lifecycle](../../../crates/layerfs-storage/src/cas/lifecycle.rs) still uses save ownership and prefix publication; [insertion](../../../crates/layerfs-storage/src/sqlite/write.rs) is ordinary batched INSERT. | Proposal/issue status does not establish concurrent-save safety. |
| [Reference allocation](../../../../crates/layerfs-layerstack-store/src/schema.rs) temporarily selects DELETE journal and synchronous FULL. | This conflicts with [core's no-added-durability policy](../../../AGENTS.md). |
| [Current pair 3](service-daemon-transport/01-architecture-and-portability.md) keeps C1/C2 in the service. | Workspace submits stable logical input, not remote calls for internal objects. |

Stage 7 is **closed**, per its [acceptance comment](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172#issuecomment-5745898896)
and [evidence](../../../../docs/roadmap/0.1/0.1.7/evidence/stage-7-substitution-20260920T000000Z/README.md).
The unchanged consumer proved fresh-data substitution; cross-version schema 4/6
Store reads were explicitly refused. This is not migration or history acceptance.

### Later inspection and landing: the two-writer implementation

**Landing confirmed, 2026-09-21:** the other integration agent subsequently
pushed [the reconciled multi-writer implementation](https://github.com/Ephemeral-AI-Lab/layerfs/commit/eb319aaa9ef196358035e6af86066e51b2bdf826)
directly to main. Remote main `644081e05896c565398108d664e01f2dddd0ce98` contains it.
The committed source confirms schema 7, two service operations and two private
saves per Store. This supersedes the pending/merge status in the historical
inspection below; pair 2 now targets that landed public contract. The dirty-file
hashes below remain identities of the pre-landing inspection, not the reconciled
commit or a newly run qualification.

Follow-up inspection on 2026-09-21 found newer source in the
`codex/pair3-foundation` worktree at HEAD
`fc647e91aaf82cef9432f4c02e90aa246461b5cd` **plus uncommitted changes**.
GitHub main was independently checked at `7b8a7d9d30cabf208229380020cd5442519574a5`,
the [merged pair-3 foundation](https://github.com/Ephemeral-AI-Lab/layerfs/pull/200).
That merged checkpoint still uses schema 6 and single-operation service admission.
The newer multi-writer implementation is not part of that merge. The source
findings above describe the older inspection basis, not the newer implementation.

The pending C2 implements **two private saves per Store**, each with a save ID,
private packs/candidate state and owner-scoped cleanup. Short database transactions
serialize while input/construction/encoding may overlap. Publication uses a save
row and publication sequence, so B can finish before A without exposing A's data.
Its **schema 7 has six tables**, adding `saves`; `objects` is keyed by
`(object_id, save_id)`. Up to two exact-byte-validated physical locators may exist
for the same canonical ObjectId. Foreign private data cannot satisfy logical,
delta or pooled-value dependencies. Service admission is two operations with no
waiting queue; each operation retains one construction producer.

Inspected file hashes within that worktree:

| Path | SHA-256 |
| --- | --- |
| `core/crates/layerfs-storage/sql/schema.sql` | `d4766986930a0fe6aec8552d5c46118d7f7206fc2c567af1c5ebee8e0be7637d` |
| `core/crates/layerfs-storage/src/sqlite/ownership.rs` | `7749a759e84be0fdde6429b23318d8b8f32ec7c286a83719eb55f30e03404ad8` |
| `core/crates/layerfs-storage/src/cas/lifecycle.rs` | `127fa4dbc3a02b2b4aa688bacdf32820b57fb82655ff539742daae17c957ad3f` |
| `core/crates/layerfs-service/src/owner.rs` | `9411fa2b9bc89bf2d1886bc101b3315673a89dd376829ff97bd966b0c332c868` |

Existing receipts under that worktree's
`core/docs/architecture/proposal/service-daemon-transport/implementation/evidence/`
were inspected, not rerun: `optimization-storage-final-tests-20260920` and
`qualified-v9-faults-writers-20260920` record functional passes, including reverse
completion and abort isolation. They do not establish history integration,
throughput, full resource-envelope acceptance or arbitrary cross-process writers.
Native arbitration currently uses Unix file identity and an in-process mutex;
external SQLite contention has one attempt with zero busy timeout.

Pair 2 should consume the chosen qualified C2 checkpoint through its public save
contract. Its separate seven-table catalog does not change between these C2
schemas. C2's `saves` is physical ownership, not `workspace_stages` or Commits.
Both disjoint and overlapping edits may be stored; same-head history publication
still permits only one state-changing winner. C5 reports `HeadMoved` without
inferring overlap or automatically combining roots. Explicit rebase remains
Workspace work; overlap detection/resolution remains outside this proposal.

## 2. Component boundary

```text
FUSE / CLI / another caller
            |
      Workspace (when an overlay is needed)
            |
     logical request via direct call or bridge
            |
     service: authorize, admit, validate, compose
          /                 |                    \
   C1 content          C2 storage             C5 history
   build/read trees    save/read objects      catalog transitions
          \_________________/                    |
          local public contracts          history catalog adapter
```

C1/C2 gain no history dependencies. C5 has no dependency on `layerfs-storage`,
Workspace, daemon, service, bridge, FUSE or an executor. It may reuse C1's supported
scalar types (`ObjectId`, `FilesystemRootId`, `InodeScope`) without invoking tree
algorithms. Keep one C1 package identity throughout the dependency graph.

Start with one `layerfs-history` crate. Its small public `HistoryCatalog` contract
exposes typed atomic mutations and bounded reads. An optional native SQLite module
implements it with the existing dependency version. Service composition selects
the catalog alongside C1/C2. No wrapper/factory per operation, generic SQL,
arbitrary transaction callback, key/value escape hatch, pack ID or C2 watermark.

This one provider boundary represents real metadata I/O and the requested
environment independence. Validation, identity derivation and records remain
ordinary functions. Native calls may be synchronous; request/result values and
atomicity do not depend on synchronous versus async delivery. A managed provider
adapts delivery and proves the same semantics; a Rust trait alone is not that proof.

| Responsibility | Owner |
| --- | --- |
| Canonical file/tree algorithms, profiles and validation | C1 |
| Object publication, physical schema, packs and representations | C2 |
| Commit ancestry, branches, layers, stages and inode reservations | C5 |
| Authorization, Store/catalog binding, save-to-history composition, admission, result delivery | Service / pair 3 |
| Mutable edits, generations, handles, explicit rebase input, local discard | Workspace / pair 1 |
| Startup, endpoints, native paths and deployment configuration | Daemon/service adapters |

"Workspace-agnostic service" means no live overlay, open handles or mutable
Workspace replica. C5 may store an opaque Workspace key. Connection close,
unmount and process Drop do not implicitly delete a stage.

## 3. History model to retain

Follow [reference records](../../../../crates/layerfs-layerstack-store/src/records.rs),
[identity derivation](../../../../crates/layerfs-layerstack-store/src/ids.rs),
[branches](../../../../crates/layerfs-layerstack-store/src/branch.rs)
and [schema](../../../../crates/layerfs-layerstack-store/sql/schema/v10.sql).

| Entity | Proposed meaning and guarantees |
| --- | --- |
| `LayerStack` | Named publication timeline; one genesis, one mutable head, at most one accepted successor per Layer. Name uniqueness belongs to its catalog authority. |
| `Branch` | Named pointer within one stack, with `base_layer_id` and optional `head_commit_id`. Effective root = head Commit root, otherwise base Layer root. Return these as one coherent snapshot. |
| `Commit` | Immutable `(root, parent_commit, base_layer)`. Preserve reference `H(domain, root, optional parent, base)` encoding when compatibility is intended. Parent means recorded ancestry, not a diff or proof of reconciliation. |
| `Layer` | Immutable publication of a complete root, derived from `(stack, parent_layer, root)`, with immutable source Branch/Commit provenance alongside it. Reading the filesystem does not replay Layer deltas. |
| `Stage` | Authoritative saved candidate plus its frozen Commit preconditions. It is neither a Commit nor a Layer. |

A branch has a single-parent lineage; forks share Commits and ancestors, so the
union can fork. A stack's accepted Layer chain is linear. Commit IDs contain no
Branch ID. Layer IDs omit source provenance: duplicate IDs must match all
immutable fields, or fail, rather than overwrite attribution.

Fork from a Layer creates a Branch with that base and no Commit. Fork from a
Commit validates membership in authorized source history, then shares the selected
Commit and its ancestry: new Branch base = selected Commit base, new Branch head
= selected Commit ID, even when the source Branch's current base differs.
No filesystem objects are copied. Ancestry checks can
walk history: only insertion is constant size. Use explicit page/work bounds,
not an unconditional O(1) claim or the reference's implicit million-row ceiling.

Stack/Branch IDs and Workspace incarnations use explicit portable issuance.
The application supplies checked IDs/seed material; C5 enforces authority-local
uniqueness. Do not port `/dev/urandom` with time/PID fallback. Missing required
entropy/capability fails explicitly.

## 4. A stage contains its publication preconditions

Keep one stage per Workspace incarnation, containing:

```text
catalog/Store binding, workspace key, unique stage token
branch ID, stack ID
expected branch head (including None), expected branch base, expected branch root
construction base root, intended Commit base Layer
saved candidate filesystem root, profile, inode scope
input generation (opaque echo for Workspace reconciliation)
```

Ordinary commits use the branch snapshot as construction base and preserve its
base Layer. Explicit rebase may use a different construction base/intended Commit
base, but the service validates that operation and its same-stack/scope context.
Replacing an expected token alone does not rebase an old tree.

Create only when the Workspace stage is absent. No in-place replacement initially:
explicitly discard the old token, then stage the new candidate. Allocate a
monotonic catalog-scoped stage token in the insertion transaction; preserve its
counter in catalog metadata after deletion. Refuse exhaustion or silent reset.
Token continuity also depends on the catalog incarnation/recovery contract.

Commit/discard name the exact token. A delayed request for A cannot consume B,
even if roots match. Generation is echoed to Workspace, not used as CAS token,
authorization or idempotency key. Multiple Workspaces may stage for one Branch;
only one state-changing Commit wins against the same head/base. Multiple no-change
stages can each return `UpToDate` without moving that head. Losing stages remain.
Do not add head rewind/reset without a revision-aware, versioned ABA policy.

## 5. Operations and atomicity

Every mutation makes one attempt. Its read/check/write sequence is atomic within
the history provider, with no successful partial transition.

| Operation | Required behavior |
| --- | --- |
| `InitializeHistory` | Given a validated saved root and reserved scope, atomically insert genesis Layer and LayerStack. No automatic Branch, Commit or Stage. Scanning/construction stays outside C5. |
| `ForkBranch` | Validate source stack/Layer or authorized Commit ancestry; insert Branch sharing history/root without content construction. |
| `ReadBranch` / `ReadStage` / `ReadCommit` / `ListHistory` | Coherent typed records or explicit absence/failure. Pages have count/byte/work limits and continuation anchored to a fixed head. |
| `StageRoot` | Validate binding, scope, captured context structurally and stage absence; allocate token and insert frozen context. A moved current branch does not prevent retaining the saved candidate: `CommitStaged` detects that race. Acknowledge only after metadata COMMIT. |
| `CommitStaged` | Validate exact stage and expected branch head/base/root; insert/reuse immutable Commit; CAS branch head/base; delete exact stage, in one transaction. On mismatch return `HeadMoved`/`StageChanged`, preserving the stage. |
| `PublishLayer` | Validate target and exact branch head/Commit; check existing exact-source publication first. Otherwise require Commit base = Branch base = expected current stack head, then insert/reuse Layer with Commit root and CAS stack head atomically. Branch head/base stay unchanged. |
| `DiscardStage` | Delete only the matching token; absence/mismatch is explicit. Never delete whichever stage occupies the Workspace key. |
| `ReserveInodes` | Atomically reserve a checked scope-wide range before exposure, subject to §8's continuity contract. |

For Commit, unchanged root **and** base yields `UpToDate`, consuming the exact
stage after the same checks. A new base may require a Commit even if root bytes
match. Publish may return `UpToDate` for the exact previously published source,
or `NoChanges` when root matches the current base. Validate scope/target first,
then check exact-source `UpToDate` before stale-head refusal: successful
publication left its source base unchanged. For a source not yet published, check
base/head equality before `NoChanges`.
These results do not authorize replay after an unknown acknowledgement.

The [branch CAS](../../../../crates/layerfs-layerstack-store/sql/workspace/advance_branch.sql)
compares head and base; the [stack CAS](../../../../crates/layerfs-layerstack-store/sql/layerstack/advance_head.sql)
is separate. Publication does not advance the Branch base. A later publication
needs explicit runtime rebase/updated Commit semantics or a new Branch from the
published Layer. C5 never invents that content transformation.

Publication has bounded indexed history operations and zero filesystem-object
traversal/construction/copying: constant work in filesystem size, not a latency or
database complexity measurement. Branches still share backend writer capacity;
Branches in the same stack also share its head CAS.

## 6. Save composition, trust and persistence

The service creates trusted internal state equivalent to:

```text
SavedFilesystem { logical Store binding, root, profile, inode scope }
```

This is proposed integration state, not a C2 receipt or cryptographic capability.
It requires the exact validated C1 filesystem operation **and** successful C2
`SaveOperation::finish`. Initial [service operations](service-daemon-transport/07-public-operations.md)
can return saved roots without creating history.

The first history route composes validated filesystem save and immediate
`StageRoot` in one service invocation. Trusted root facts flow directly between
those steps; no process-local root registry is needed. They remain separate
persistence boundaries, so failed stage insertion may orphan completed content.
Later root-only requests use already-established catalog provenance or a
separately specified validated-ingestion capability. Arbitrary previously saved
roots are not accepted until that capability exists; a connection-local receipt
map is not restart-independent authority.

Caller-supplied `acknowledged=true`, raw `ObjectId`,
`Store::contains`, or generic raw-object save success is insufficient.
An authenticated read plus `FilesystemRoot::decode` validates root bytes,
profile and scope, not dependency closure or complete namespace topology.
C2 checks producer-declared references, while
[`FinalizedObject::new`](../../../crates/layerfs-content/src/object/output.rs)
starts with none. Keep topology validation in C1/service ingestion, not a hidden
full-tree walk for history mutations.

Service authorizes principal → Store/catalog → stack/branch/stage. C5 enforces
structural/catalog membership without importing transport authentication types.
Roots and IDs confer no authority.

| Phase | Successful acknowledgement | Failure/persistence meaning |
| --- | --- | --- |
| ① Prepare | None; stable inputs/provisional construction | Runtime state, not history. Already exposed inode reservations remain consumed. |
| ② Physical write | C2 may commit bounded private chunks | No saved-root success yet; C2 owns definite-failure abort. A complete save is not necessarily one transaction. |
| ③ Save publication | Exact root's C2 save completed | Published objects, no implied Stage/Commit/Layer. |
| ④ Stage / logical Commit | Stage insertion is one acknowledgement; Commit + branch CAS + stage deletion is another | Stage is authoritative metadata, not an advisory cache. Failed CAS retains it. |
| ⑤ Layer publication | Layer + stack CAS committed atomically | Stale head returns `HeadMoved`; Branch Commit remains. |

No phase promises crash/power-loss durability under MEMORY journal, synchronous
OFF and no-sync/no-WAL. Ordinary successful close/reopen can recover catalog
rows; it does not recreate Workspace overlays or prove allocation continuity
after arbitrary restart. Validate relevant roots through the bound provider
before resumed use. Missing content is failure, not an empty filesystem.

Save success followed by catalog failure leaves published content unreferenced.
Stage discard keeps content and recorded history. No pin/unpin or GC API is
invented. Definite failure before publication may clean private state via C2's
existing abort contract; C5 never deletes packs.

There is no transaction spanning C2 and history, nor distributed commit protocol.
Machine failure can leave the two persisted states inconsistent. Refuse broken
references rather than promise atomic recovery. Future GC would require an
explicit root-retention/handoff contract across this boundary.

Unknown outcomes remain unknown failures. Lost terminal result delivery and
cancellation during COMMIT do not prove rollback. Optional telemetry failure
leaves a known committed result unchanged. Do not resend, delete, rewind
or refresh/reprepare on a guess. Explicit reads inspect current records; existence
alone is not an operation-outcome ledger. No background recovery/polling service
or durable idempotency ledger is proposed.

## 7. Catalog placement and schema

**Separate catalog, initially one SQLite database per logical Store/authority
binding.** C5 owns its schema; the selected C2 schema stays unchanged by history
(five-table schema 6 in the original foundation; six-table schema 7 after the
multi-writer landing described in §1). An authority
may contain multiple projects/stacks. Isolation follows pair 3; copies must not
become independent allocation authorities for the same scope.

Proposed history **schema v1**, own application ID, seven tables:

| Table | Semantic fields / constraints |
| --- | --- |
| `history_meta` | Singleton catalog identity/incarnation, Store/authority binding, history identity-format version, monotonic next-stage token. |
| `layer_stacks` | ID, authority-local unique name, scope/profile association, head Layer. |
| `layers` | ID, stack, parent, root, optional source Branch/Commit; one genesis and accepted child per parent. |
| `branches` | ID, stack, stack-local unique name, base Layer, optional head Commit. |
| `commits` | ID, root, optional parent Commit, base Layer; immutable and shared by forks. |
| `workspace_stages` | Unique Workspace incarnation/token plus binding and frozen context from §4. |
| `scope_allocator` | Unique scope, checked high-water/exhaustion state, continuity authority association. |

Foreign keys, uniqueness and conditional updates enforce relationships. Same-stack
ancestry/base checks and duplicate-ID field equality remain explicit transactional
validation where constraints cannot express them. No foreign key enters C2.

Create writes v1. Open validates application/schema/identity-format versions,
tables/columns/constraints, authority binding and numeric ranges before mutation.
Unknown/older schemas or malformed records fail; do not create missing tables,
reset counters, guess migration or reinterpret profiles. Each later added table
declares a new C5 schema. Sharing C2's file instead requires a separate reviewed
C2 schema/validator change and is not this recommendation.

Native adapter: MEMORY journal, synchronous OFF, no WAL, zero busy wait and one
transaction attempt. Reuse published dependencies; no patches/fallbacks. Paths
belong to service configuration. C2 has no public persistent Store UUID today:
preserving the logical binding across relocation/import is a service requirement,
not an existing C2 capability. Root/profile checks alone cannot prove an operator
selected the intended Store.

## 8. Allocation and discard

C1's [inode contract](../../../crates/layerfs-content/src/filesystem/identity.rs)
accepts `1..=i64::MAX` but allocates nothing. Current-base checks do not cover all
Branches, discarded stages or unused exposed reservations.

C5 owns one allocation authority per inode scope. Every Workspace, Branch and
zero-copy fork sharing it uses that authority. Reserve an explicit bounded count
with checked arithmetic; commit before exposure; never return exposed numbers on
failure/discard. Do not copy the reference's fixed `2^32` range into the API.
Exhaustion fails explicitly.

Namespace initialization reserves new scope/range before construction, saves the
root, then creates genesis history. Failure may consume a range and orphan saved
objects. Init retains its allowed multi-worker path and existing cold target;
commit/capture/snapshot stay single-producer. Allocation adds no construction lane.

**Open implementation gate: local MEMORY/OFF allocation cannot establish
crash-safe nonreuse.** A stored high-water row may roll back after a crash.
Scanning retained roots misses exposed-but-unused serials; an in-file clean-exit
flag has the same durability limitation.

Initial support is allocation under one continuing authority, no recycling on
failure/discard, and tested ordinary reopen of intact catalog state. For uncertain
restart, backup restoration or independent catalog copy, refuse new allocation
and writable resumption unless an explicit authority establishes continuity.
Specify that proof mechanism before claiming resumable new-inode writes.
Read-only use can continue if catalog/content validation succeeds.

External allocation authority is a future capability, not permission for added
sync/WAL. New-scope migration must be explicit: C1 [requires base scope equality](../../../crates/layerfs-content/src/filesystem/validate.rs),
so rollover is not a transparent zero-copy repair. The first existing-inode
service route does not depend on stronger allocation recovery.

Discard removes only the exact stage reference. Workspace clears its own mutable
state separately after a known result. Neither deletes Branches, Commits, Layers,
stacks, shared content or reservations. Without GC, saved unreferenced bytes
accumulate; bounded live memory does not bound retained disk usage.

## 9. Environment independence and compatibility

| Replacement | May change | Must remain stable |
| --- | --- | --- |
| Compatible C1 revision | Content algorithms/dependency selection | Canonical profile/IDs, public behavior, errors, bounds; no C5 changes. |
| Compatible C2 revision | Storage algorithms/dependency selection | Save/publication and declared persisted-format compatibility; no history SQL/logic changes. |
| C5 provider | Native/managed metadata adapter and wiring | Atomic transitions, ID encoding, absence/failure distinctions, page bounds, allocation continuity. |
| Daemon/transport | Projection, framing, endpoints, scheduling | Authorized semantic requests and terminal results; same history meaning. |
| Host/platform | Paths, endpoint setup, entropy, scratch capability | IDs, canonical names and scoped serials independent of host paths/PIDs/clocks. |

Paths, descriptors, SQLite handles, Tokio tasks, FUSE inode numbers, HTTP messages,
process lifetimes and ambient environment variables never enter C5 semantics.
Names follow explicit canonical UTF-8/byte rules, not host case folding/locale.
Limits are explicit; absent required capabilities fail. A managed backend must
provide required atomic transactions/CAS and bounds or refuse support. This does
not claim native rusqlite or every C1 scratch adapter already runs in WASM.

Wire version, C5 schema/identity format, C1 canonical profile and C2 physical
schema are distinct. Dependency selection/rebuild does not imply a stable Rust
ABI or transparent format migration. Async delivery may need adapter changes
without changing history semantics.

Keep batching and bounded input/output. `HeadMoved` detects stale publication,
not overlapping edits. Return it once. Explicit rebase belongs to Workspace and
can silently resolve overlap until [#164](https://github.com/Ephemeral-AI-Lab/layerfs/issues/164).
No automatic queue/backoff/rebase, diff or conflict resolution enters C5.

Optional existing telemetry scopes measure finite history operations; service
owns reports and process/resource collection. Do not carry thread-bound scopes
across executor hops or reinterpret a committed result when reporting fails.

## 10. Implementation and proof

1. **Freeze the small contract:** Store/catalog binding, root provenance,
   stage/branch/stack results, v1 schema and supported allocation continuity with
   pairs 1/3. Sequential composition needs no C2 SQL extension.
2. **Implement real C5 behavior:** one crate with IDs/records, native catalog,
   transitions, bounded reads and supported reservation behavior. Suggested files:
   `identity.rs`, `records.rs`, `catalog.rs`, `staging.rs`, `commit.rs`,
   `layerstack.rs`, `allocation.rs` and focused SQLite/schema files. Add files only
   with real code. Entry files are delegation-only, ≤200 physical lines; other
   product files ≤999; tests outside `src/`.
3. **Compose through service:** reuse filesystem/save bodies, then C5. Extend the
   existing bridge after semantics freeze. Workspace installs only acknowledged
   input generations, preserving later edits. Keep Q=0 and the selected C2
   admission/ownership contract; the landed schema-7 profile supports two saves,
   with one construction producer per operation.
4. **Prove lifecycle/replacement:** external public consumers, direct service and
   real daemon/service route. Reuse identity-matched multiwriter substrate evidence
   where applicable, then prove history integration separately; issue status alone
   is insufficient.

Required behavioral checks, all **NOT_RUN for this investigation**:

| Check | Acceptance |
| --- | --- |
| Init → fork → save → stage → Commit → Layer | Correct IDs/parents and root reuse; separate branch/stack transitions. |
| Same-branch stages; same-stack branches | One expected-head winner; loser retained; Busy distinct from HeadMoved. |
| Delayed stage/discard; no-change paths | Stale requests never consume new stages; §5 outcomes respected. |
| Failure at every phase boundary | No knowingly failed root published into history; orphans allowed; unknown explicit. |
| Disconnect/cancellation around COMMIT | No replay/guessed rollback; server state and caller knowledge distinguished. |
| Reopen, wrong binding/schema/profile, missing root | Intact state reopens; incompatible state refuses; no crash-survival claim. |
| New invocation and connection after staging | Create stage, drop its connection/handler, then inspect and commit/discard using only bound catalog, Store and explicit IDs. No session-local preconditions; continuity restrictions still apply. |
| Fork-shared scope, failure/discard, exhaustion | Disjoint ranges and no reuse; unproven continuity refuses writes. |
| Authorization and hostile root assertions | Cross-authority access refused; raw existence is not trusted provenance. |
| Unchanged consumer: C1-only, C2-only, paired revisions | No integration logic edits; canonical and persisted compatibility separate. |
| History without native I/O; Linux/macOS integration | Semantic code builds without FUSE/executor/native SQLite; native routes share contract. Managed runtime remains separate proof. |
| Bounded history/content errors/telemetry | No whole-history/tree accumulator, retry/input shrinking or per-object report growth. |

Implementation uses explicit core-manifest locked tests/examples, fmt and
warning-denying Clippy, plus boundary guard and self-tests. This investigation
changes documentation only: no product, benchmark, integrated qualification or
release acceptance is claimed. Later measurements obey existing cold-cache,
identity, lock and wall-budget rules; no worker/timeout/cache increase is proposed.

Remaining design gates: service Store/provenance binding, allocation continuity
for writable recovery, and conformance before supporting another catalog provider.
GC, cross-store atomic crash recovery, multiwriter C2, conflict resolution and
automatic retries are outside this implementation proposal.
