# S7 and S9 remaining implementation plan

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Prepared 2026-10-06, Asia/Singapore. S7 and S9 remain incomplete; S8 is a later batch.
> Current progress is reconciled against local source, retained receipts and #307.

## 1. Assignment and authority

Complete the independent S7 engine cost/resource gate and S9 authenticated host
runtime/faithful complete-root work in the primary checkout
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, local `main`, product under `core/`.
This plan records the owner's requested next-work plan and the SQLite acquisition
correction discussed in the current chat. It is not evidence that the correction
has been implemented or that its concrete public backing contract is settled.

Read [root AGENTS](../../../../AGENTS.md), [core AGENTS](../../../AGENTS.md), both
root handbooks ([cluster one](../../../../cluster_one_handbook.md),
[CAS/CDC/delta](../../../../cas_cdc_deltaencoding_handbook.md)), the
[303 index](../303/README.md), primary operation contracts,
[engine](../303/daemon-sqlite.md), [FUSE](../303/fuse.md),
[runtime](../303/06-cluster-one-integration.md) and
[validation](../303/07-implementation-validation.md).
Apply [source organization](SOURCE-ORGANIZATION-S7-S13.md) as grouping guidance,
extending [the original organization](SOURCE-ORGANIZATION.md); it does not select
algorithms. Its dated S6-in-progress statement is superseded by completion.

This is the current next-work checklist following the consumer/backed-import
commits. Older handoff gap lists remain historical snapshots. Preserve the
[S5/S6 stopping record](HANDOFF-S7-S13.md), the
[resume note](HANDOFF-S7-S9-RESUME-20261006.md),
[speed-plan draft](S7-S9-SPEED-TEST-PLAN.md), source-organization side document and
[Workspace analysis](WORKSPACE-EFFICIENCY-ANALYSIS.md). Do not overwrite them.

S8 native FUSE/mount/control/ordinary Bash lifecycle is outside this continuation.
S10–S13 and P3/P6/P7/P13/P14 remain later Commit prerequisites. Do not implement
incremental Workspace Commit, retire the root reference or silently claim these
prerequisites resolved by initial acquisition. Local commits and #307 evidence
updates remain authorized. Pushes, releases and deployments are outside scope.

## 2. Exact progress at the planning boundary

Inspection HEAD: `b2b979f91aadc23a7858d618a438a39a4e6a3edd`.
Tree: `7e6f7a610d1db308ebe82dc277111803b30952da`. Branch: local `main`.
Before this plan, only the resume note and speed-plan draft above were untracked;
no product source was dirty. They are separate owner documents and stay untracked
unless their owner separately selects their capture. Reconcile again before work.

Tracker #307 was OPEN, updated `2026-10-06T04:44:09Z`, with S7/S8/S9 unchecked.
Its latest checkpoint receipt was [native aliases](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6009500696).
The two subsequent source checkpoints below lacked tracker receipts at inspection.

| Commit | Actual result | Core LOC | Reference LOC | Combined LOC |
| --- | --- | --- | --- | --- |
| `90d7a2c5b063a8ece5bdd9a4f9261845f9584dfe` | Finish and commit inherited native consumer ports | 91342 → 91915 (+573) | 65417 → 65417 (+0) | 156759 → 157332 (+573) |
| `97a02fffc82a8826da4e2262a005db1b5ae091aa` | New streamed, ordering-run-backed initial acquisition | 91915 → 92680 (+765) | 65417 → 65417 (+0) | 157332 → 158097 (+765) |
| `b2b979f91aadc23a7858d618a438a39a4e6a3edd` | Committed LOC/tree receipts only | 92680 → 92680 (+0) | 65417 → 65417 (+0) | 158097 → 158097 (+0) |

The respective source trees are `8832331f82394036c423673a9d7495b0d99fdb81` and
`568ec923a9196338f9c030772364b17fe96790f9`. Their
[consumer LOC receipt](checks/s9-consumer-ports/committed-loc.json) and
[acquisition LOC receipt](checks/s9-backed-acquisition/committed-loc.json) match
the actual first parents and committed trees. Use the unchanged counter, not diff
statistics. These commits introduce no reference retirement or measured speedup.

Completed earlier boundaries remain S5 `a0dc7da9b`, S6
`983c2ee6d36a4417d8fff2d14db6b141f5386c8c`, tree
`be2744223a450eaa01b9f31c4e3c850bbd141d72`, and receipt/handoff
`4ecea41983b673d62db90880b777a94565eac985`. Do not reopen their old campaigns.

### S7: implemented attribution, acceptance still open

Existing Overlay/Daemon observations count original SQL attempts/executions,
actual VM work, bound/returned bytes, triggers, payload cells/copies, queues,
retained results and maintenance. Startup work and actual native
framing/socket/crypto attempts are observed. Earlier EXPLAIN/runtime profiles,
allocation/range/freelist proofs retain their own identities and scopes.
The full daemon reservation is **268435456 bytes**, comprising 128 MiB mutation
and 128 MiB cleanup capacity. Physical reservation is not RSS or logical page count.

Neither of the two latest product commits advances the S7 acceptance campaign.
Exact page/dirty/overflow/index/journal/device-I/O evidence, whole-system phase
residency, numerical service/debt acceptance and complete cumulative cost closure
remain open. See the separate [S7 audit](S7-EXIT-AUDIT.md) and
[operation observations](../../architecture/36-operation-cost-observations.md).

### S9: primitives and streamed Init exist; assembly and qualification do not

| Area | Implemented now | Remaining exit |
| --- | --- | --- |
| Host library adapters | Initialized Store/Storage/history owners; policy, objects, lengths, serial ranges, interleaved Save lifecycle, same-Save reads, exact history receipts | Complete owning application/consumer assembly and real cross-platform topology |
| Authentication/wire | Native KK peer verification; checked headers/fragments, typed requests/replies/errors, authority before body allocation, retained partial/result credits | Supervised connection ownership and all disconnect/restart boundaries |
| Fair service | Bounded class/Workspace dispatch, same-Save ordering, demand/control reserves, retained-result ownership | Slow-peer isolation and numerical sustained progress through the assembled path |
| Consumers | `Calls`, `RemoteObjects`, `RemoteLengths`, `RemoteSerials`; independent close; terminal original call failures | Ordinary owning attachment/provider wiring; no assumed pipelining through the shared call mutex |
| History/authority | Captured binding, semantic object admission, scope/profile/root checks, saved reference closure, known conflict/discard custody | Full contextual child/topology/provenance evidence and restart fencing; P10 resolver scope explicit |
| Initial acquisition | Full membership and opaque links; native hard-link identity; streamed canonical directory/inode construction; input-sized records in file runs | SQLite backing correction, failure/accounting corrections, owning-platform huge-root/>4-GiB and whole-resource qualification |

The consumer checkpoint has 40 host and 21 Docker portable passing bodies and
seven matching pre/post binary hashes per platform. Those were predecessor
checks adopted at the source checkpoint, not new performance results. Its source
map is retained in [identity](checks/s9-consumer-ports/identity.json).
Later Project-only changes do not invalidate unchanged consumer behavior, but
the old all-source map is not the current complete repository map.

The run-backed acquisition checkpoint has 12 host passing bodies and 10 Docker
passing bodies across its selected runs. The Docker all-target command itself
**FAILED**: unmodified `init_sqlite` reached the macOS-only provider and returned
`BackendUnavailable` before Init. A later selected scaling binary passed once;
the failed command remains failed. Source hashes are retained, but no compiled
binary pre/post seals were recorded for this acquisition checkpoint. Do not
retroactively fabricate them. See [failures](checks/s9-backed-acquisition/FAILURES.md)
and [identity](checks/s9-backed-acquisition/identity.json).

No cold speed, RSS or sustained-rate result is eligible. Greater-than-4-GiB native
proof and the speed draft's proposed rows are NOT_RUN. The Claude session was
interrupted while designing supervision; no new Supervisor or attachment owner
was committed. FUSE/API-core/Sandbox remain excluded in [core Cargo](../../../Cargo.toml).

### Findings requiring correction

1. SQLite was withdrawn after a direct `rusqlite` dependency inside Project was
   rejected. That guard establishes dependency direction, not a performance case
   for a custom sorter. Restore indexed acquisition through the proper owner;
   do not restore the rejected per-Init-database prototype unchanged.
2. Scratch is created before scanning without excluding its placement from the
   source. An empty source with `scratch_parent == source` failed with ENOENT
   after scanning its own temporary files. Prevent self-acquisition before any
   input modification, including path aliases/symlinked parent placement.
3. The reused Content `FileBacking` refunds an entire failed `write_all` append
   even when bytes reached disk. A public-API probe observed 4096 physical run
   bytes, held charge 0, logical length 0, requested high-water 65536. This is an
   inherited helper defect exposed by the new path; fix surviving users too.
4. `Scratch::finish` can mask a release failure with directory removal and turns
   the release cause into a string. Preserve deciding causes and residual custody.
   Review destructor cleanup as well: a failed explicit release must not become
   an unreported cleanup replay or erase the retained owner/account.

The two public probes failed once each, under 30-second wall ceilings, after an
isolated Cargo no-run build. They are regression diagnostics, not qualification
or performance samples. Their initial manual-linking harness build failed because
it mixed compiled crate instances; no test ran then. All output, the repaired
isolated build, harness/lock identities and source hashes are retained in
[review receipts](checks/s7-s9-review-20261006/review.json).

## 3. Required SQLite backing correction

**Resolve the rejection first, before another acquisition rewrite.** The guard
should continue to reject direct Project → rusqlite/engine access while accepting
the existing Project → Storage and Persistence → Storage/SQLite composition.
Its diagnostic and an external guard regression now state that SQLite-backed
Project acquisition is allowed through that ownership boundary. Core guidance
states the same distinction. This tooling/documentation correction does not
implement the missing backing capability or make the discarded prototype valid.
An agent must not respond to this refusal by selecting a custom sorter solely to
make the guard green. A1/A2 establish the actual ordinary SQLite-backed path.

Project remains the single initial-acquisition implementation. Host-side
acquisition uses host-owned SQLite; daemon Workspace/Commit scratch remains in
the one daemon-local Overlay database. Canonical filesystem/content formats,
CAS identities, global Save and history semantics remain their current owners.
Scratch contains metadata, source-validation facts, jobs, aliases and object IDs;
it does not duplicate every file payload or replace canonical objects with rows.

The default design target is operation-namespaced acquisition state in the existing
host-owned database, initialized through explicit owner composition, with indexed
bounded jobs. No per-Init/Workspace/tool-call database or connection bootstrap.
Project must not open SQLite, receive a raw connection or issue SQL. No whole-file
construction/Save/transport wait holds the provider writer or transaction.

**A contract/compatibility design is the first implementation deliverable.** Current
Project `init` takes Storage and HistoryCatalog; neither input currently exposes
acquisition backing. Persistence currently validates the exact table set, schema
source/definition and version. Existing TEMP storage is selected as MEMORY, which
cannot be assumed to provide disk-backed huge acquisition. Simply adding tables,
turning off global sync, or hiding a TEMP-memory whole root is unacceptable.

Specify an additive, backend-neutral ownership capability along existing
Project → Storage/Content and Persistence → Storage/Content dependencies. The
expected contract home is Storage's `port/acquisition/`, with concrete SQL in
Persistence. Confirm this placement against actual owner composition before
introducing types. Keep current public entrypoints and canonical contracts;
demonstrate how existing callers obtain the capability without a new Store per
operation. A required public-contract addition is explicit, not a silent break.

The design must specify supported Store schema/open compatibility, creation and
reopen checks, read-only refusal, operation identity/epoch, ownership/fencing,
capacity/page charges, cleanup and restart disposition. No implicit format
migration or weakening of Store integrity checks. Any necessary incompatible
Store-format decision must be documented and resolved before its implementation;
the backing request alone does not authorize silent migration.

Expected relations/access paths, refined in that design:

| Logical state | Indexed access and obligation |
| --- | --- |
| Acquisition owner | Exact operation/epoch; original terminal/uncertain disposition; no reused key |
| Entries | `(operation, position)`; directory bindings by `(operation, parent, binary name)`; preserve BFS/name order and attributes |
| Directory frontier | `(operation, position)` keyset windows; no increasing OFFSET or resident full frontier |
| Native regular identities | `(operation, device, inode, position)`; first position canonical; byte-exact source stability evidence |
| Unique jobs and completed roots | Indexed original job/position; construct each identity once; failures retain original phase |
| Aliases and counts | Indexed canonical/alias position; count only bindings inside the root; serial holes stay consumed |
| Metadata, symlink and directory roots | Point/window reads by operation/position; opaque targets preserved without traversal |
| Reclamation | Operation-owned bounded deletion cursor; retained/uncertain owners prevent premature disposal |

### Proposed three-table working schema

The proposed minimum is three ordinary shared tables in the existing global
SQLite Store: `init_operation`, `init_entry` and `init_native_file`. They are
**planned tables, not current Store schema**. Their concrete columns, constraints,
indexes and supported schema/open compatibility are A1's design deliverable;
creation and provider implementation belong to A2. They are created through
explicit compatible Store initialization and retained for later operations.
Do not use per-Init databases, create/drop the schema on every Init, or put an
input-sized acquisition into the current MEMORY-backed TEMP schema.

| Proposed table | Working facts | Expected keys/indexes |
| --- | --- | --- |
| `init_operation` | Operation identity, owner epoch, source/binding facts, phase, position/serial-allocation facts, progress and capacity charges, original outcome/custody and bounded cleanup cursor | Exact operation identity; eligible phase/cleanup access that does not scan unrelated operations |
| `init_entry` | Stable scanned-entry identity, parent identity, binary name, kind, portable attributes, native path/stability evidence, opaque symlink target, finalized BFS position, native-file identity reference and constructed metadata/target/directory object IDs | Primary operation/entry identity; unique operation/parent/binary name; ordered operation/position; directory scan-state/position selection |
| `init_native_file` | One distinct native device/inode, checked source evidence, canonical entry/position, in-root alias count, construction state and canonical file object ID | Unique operation/device/inode; ready-job state/canonical-position access; completed-root access in acquisition order |

All access paths are scoped by operation identity. A stable scanned-entry key is
distinct from its finalized BFS position: a directory's native enumeration may
arrive unsorted, and its name-ordered children receive final positions through
indexed traversal. Canonical hard-link identity is selected by the first finalized
acquisition position, not whichever native directory entry happened to arrive
first. Preserve exact unsigned native identity fields through an explicit encoding;
do not silently truncate them to SQLite's signed integer range.

The directory frontier and file-job queue are indexed states in these rows, not
additional permanent queue tables or resident whole-root collections. Several
`init_entry` rows can refer to one `init_native_file`; that is the hard-link alias
relation. The in-root count excludes native aliases outside the acquired source.
Constructed roots are object IDs in the relevant rows. Actual file payloads use
canonical construction and global Save; these working tables do not duplicate
payload bytes. Exact row/window bytes, index maintenance, page allocation and
transaction costs remain part of A2/E2/E3 qualification.

### Successful Init cleanup and table lifetime

**Successful Init removes its operation's temporary working rows, not the shared
tables or indexes.** Another Init may be using those tables concurrently. The
tables remain part of the compatible Store schema and their free space can be
reused; SQL deletion does not imply that the database file shrinks.

The intended successful path is:

1. Complete acquisition and canonical-root construction while retaining the
   original Save/operation owners; consume all required working facts.
2. Fence outstanding row consumers and file jobs. Only state with no remaining
   consumer, capture or uncertain attempt is eligible for disposal.
3. Delete that operation's `init_entry` and `init_native_file` rows through
   bounded indexed jobs. Retain and advance the original cleanup cursor, include
   its work in Init's accounting, and preserve any deciding failure.
4. Complete the selected Save/history publication steps and report their exact
   original outcome. Finish the successful operation's working-row cleanup before
   reporting Init success; cleanup placement relative to the final Save/history
   steps must be explicit in A1 and preserve the current publication contract.
5. Retain only the small `init_operation` outcome/custody record when an original
   reply, fence or explicit owner still requires it. Release that record when its
   last owner permits; do not accumulate it forever or discard it on an arbitrary
   timeout. The saved objects and LayerStack/history remain in their global tables.

On refusal, conflict, disconnect or uncertainty, preserve the deciding phase,
original errors and still-owned state. A lost reply or process exit alone does
not authorize deletion of unresolved work. Report retained rows/bytes and eligible
cleanup debt; do not mark them gone or retry a failed cleanup implicitly. Global
Store durability remains its selected profile even for temporary working rows.
No `DROP TABLE`, per-operation VACUUM, provider reopen or global-object/history
deletion is part of successful Init cleanup.

Use prepared SQL, bounded row **and byte** windows and explicit output backpressure.
An index's existence does not establish a bounded query: retain exact EXPLAIN and
correlated execution profiles, including table fetches, triggers and caller loops.
Keep streamed `build_directory`/`empty_directory`/`build_table` construction and
the root-equivalence oracle. Remove run-specific sorter/codec code only after the
SQL ordinary path passes its required proof; do not retain two selectable product
algorithms or a benchmark-only route. Report source retirement accurately.

## 4. Expected files and dependency ownership

`existing` means present at this source pin; `add` means an implementation
destination, not a promised API or permission to create empty files. Keep actual
source-organization groups; reuse focused existing modules before adding another.
Every `lib.rs`/`mod.rs` stays declaration/delegation-only and ≤200 physical lines;
other new production files, including shipped SQL, stay ≤999. Tests stay outside
`src/`. No filenames selected here authorize new algorithms or weaker contracts.

### Acquisition library and provider

```text
core/crates/
├── layerfs-project/
│   ├── src/lib.rs                         existing thin public exports
│   ├── src/import/
│   │   ├── mod.rs                        existing declarations
│   │   ├── init.rs                       existing single Init orchestration
│   │   ├── scan.rs                       existing scan; use indexed windows
│   │   ├── source.rs                     existing native/link stability checks
│   │   ├── files.rs                      existing bounded producer handoff
│   │   ├── batch.rs                      existing object/completion windows
│   │   ├── namespace.rs                  existing streamed canonical builders
│   │   ├── metadata.rs                   existing portable metadata construction
│   │   ├── backing.rs                    add domain use of owning capability
│   │   ├── scratch.rs                    revise to typed operation custody only
│   │   ├── work.rs                       existing accurate observations
│   │   └── error.rs                      existing original typed failures
│   └── tests/                            acquisition, identities, failures, scale
├── layerfs-storage/
│   └── src/port/acquisition/             add if contract design confirms this seam
│       ├── mod.rs                        thin capability/type exports
│       ├── contract.rs                   bounded units and ownership contract
│       ├── rows.rs                       domain facts; no SQL or file-run codec
│       └── work.rs                       actual work/charges; no RSS assertion
└── layerfs-persistence/
    ├── src/storage/acquisition/           add implementation of the owning port
    │   ├── mod.rs                        thin declarations
    │   ├── owner.rs                      shared Session + operation/epoch custody
    │   ├── rows.rs                       typed bounded mapping and window checks
    │   └── lifecycle.rs                  explicit finish/fence/cleanup disposition
    ├── src/backend/sqlite/acquisition/    add concrete engine implementation
    │   ├── mod.rs                        thin declarations
    │   ├── statements.rs                 prepared statement identities/profile use
    │   ├── access.rs                     indexed bounded reads/writes
    │   ├── accounting.rs                 actual rows/bytes/pages/allocation/debt
    │   └── cleanup.rs                    bounded indexed deletion, exact failures
    ├── src/store/                        existing explicit composition/open checks
    ├── sql/sqlite/acquisition/            add shipped schema/queries after A1
    │   ├── schema.sql                    proposed three shared working tables
    │   └── queries/                      indexed operation-scoped units/cleanup
    └── tests/                            real Store, compatibility and plan/profile
```

`Project/import/runs.rs` and file-run-only portions of `scratch.rs` are planned
replacement scope, not a second permanent backing. Content's existing public
FileBacking remains for other legitimate callers; fix its failure accounting and
cleanup in `layerfs-content/src/filesystem/references/backing.rs` with public-API
external tests. Do not add Persistence → Project or Project → Persistence/Overlay
dependencies, and do not weaken the boundary guard to admit them.

### Host runtime, consumers and application assembly

```text
core/crates/layerfs-api/sdk/src/
├── lib.rs                                existing thin exports
├── client/
│   ├── call.rs, call_error.rs, ports.rs   existing original exchange/consumer ports
│   ├── header.rs, input.rs, reply.rs      existing typed request/reply ownership
│   ├── native.rs, records.rs, failure.rs existing transport and cause codecs
│   ├── attachment.rs                     add consumer attachment/fence ownership
│   └── project.rs                        add real application forwarding, if needed
└── runtime/
    ├── owner.rs, sessions.rs, binding.rs  existing initialized owner and authority
    ├── handlers/                         existing wire/history/result execution
    │   └── admission.rs                  add contextual validation if needed
    ├── ports/                            existing length/serial adapters
    ├── service/                          existing fair provider service
    │   ├── input.rs, output.rs            existing bounded socket workers
    │   ├── queue.rs, execution.rs         existing class/Workspace dispatch
    │   └── supervisor.rs                 add host connection/service orchestration
    └── custody/                          add only real cross-connection/restart work
        ├── mod.rs                        thin exports
        ├── attempts.rs                   original dispatch/completion knowledge
        └── fences.rs                     exact epoch/process/disconnect boundaries

core/crates/layerfs-bridge/src/
├── contract/                             existing wire identities and limits
├── codec/                                existing checked representation/credits
└── native/                               existing authenticated channel and fences

core/crates/layerfs-api/core/src/contract/ identity/project/sandbox/error contracts
core/crates/layerfs-sandbox/src/
├── owner/                                real config/routing and provider binding
├── backend/docker/                       actual owning Docker transport attachment
├── session/                              binding/attachment lifetime
└── lifecycle/                            readiness and explicit connection teardown
```

SDK semantics stay in SDK runtime/handlers; Bridge owns representation and native
delivery. SQL stays in Persistence/Overlay. API-core/Sandbox are excluded today:
activate only actual required S9 replacements, with public exports, dependencies
and tests, preserving/counting excluded predecessor source. Do not import obsolete
Bridge types, revive Server or create mount/Exec/Commit/unmount scaffolds in this
batch. S9 connection teardown does not terminate Bash or unmount a Workspace.
Daemon authenticated-consumer assembly belongs to its source-organization
`upstream/` group when real S9 wiring requires it; native service/registry/control/
execution/mount integration remains S8 work. Do not add an SDK → Daemon cycle.

### S7 product observations and external qualification

```text
core/crates/layerfs-overlay/src/
├── database/                             existing settings/allocation/accounting
├── diagnostics/                          existing actual engine observations/plans
└── maintenance/                          existing debt and bounded reclamation
core/crates/layerfs-daemon/src/
├── overlay/                              existing original owner jobs/queue/credits
└── service/                              existing startup/operation observations
core/benchmark/
└── owning-s7-s9-family/                   add registered component/runtime family
    ├── specification.md                  prospective cases, budgets and observers
    ├── runner                            thin public-API execution/supervision
    ├── observers/                        external OS/SQL/phase resource observation
    └── verifier                          independent exact-state/custody oracle
core/docs/issues/307/
├── S7-EXIT-AUDIT.md, S9-EXIT-AUDIT.md      separate dated progress and exit evidence
├── checks/<checkpoint>/                  append-only build/test/source receipts
└── <campaign-owned ledger>/              registered measurements and all outcomes
```

The harness directory name is finalized by registration; it is not an aggregate
pre-push wrapper. Reuse the existing sole `init_namespace` family route for owning
SDK Init measurements rather than add a second SDK Init runner. Product telemetry
must be ordinarily useful; no test-only hooks or private-source recompilation.

## 5. Work packages and acceptance

Work may be interleaved when independent. This is not permission to overlap builds
or measurements in the same checkout or to spawn agents without authorization.
Each package ends in an exact source/check/LOC receipt; a checkpoint is not the
batch stopping boundary while useful independent packages remain.

| ID | Owner / work | Depends on | Concrete acceptance |
| --- | --- | --- | --- |
| T0 | Reconcile HEAD, source maps, ownership, tracker and retained failures; publish missing dated checkpoint receipts with current limitations | Current pin | Separate S7/S9 status accurate; S8/later unchecked; preserved side documents/containers |
| A1 | Design acquisition capability, placement, proposed three-table schema/open compatibility, cleanup/capacity and caller wiring | T0 | Written typed ownership/access contract against real public inputs; distinguish stable entry identity from BFS position; successful working-row cleanup and retained outcome ownership explicit; no forbidden edge, per-operation DB, TEMP-memory whole-root or silent migration |
| A2 | Implement provider-owned indexed acquisition and real owner composition | A1 | Initialized database and shared working tables reused; bounded row/byte keyset jobs and operation-row cleanup; no per-Init table drop; original errors/uncertainty; all relevant queries have EXPLAIN + execution profiles |
| A3 | Port Project scan/jobs/aliases/roots to that capability; retain streamed canonical construction; remove run machinery | A2 | Root-equivalence, full membership, stable native identity, consumed serial gaps and no input-sized resident collections on the ordinary path |
| A4 | Correct source/backing overlap, partial-write charges and exact cleanup custody; fix surviving FileBacking callers | T0; overlap design informs A1–A3 | Both review regressions covered through public APIs; physical partial bytes charged; deciding cleanup causes/owners retained; no replay |
| R1 | Implement host service supervisor and consumer attachment owner | T0, existing SDK/Bridge | Real initialized owners and sockets assembled; I/O outside provider owner; original result/partial credits returned at explicit fences; no whole-Save lock |
| R2 | Complete contextual admission and faithful root binding | R1, A3 for acquired-root witnesses | Identity/role/refs recomputed; closure and contextual child/root/profile/scope/provenance evidence exact; invalid/stale/cross-authority cases refused before prohibited buffering/publication |
| R3 | Complete disconnect and process-restart custody | R1; policy scope reconciled with P10 | Original unattempted/dispatched/known/uncertain states distinguishable; stale epochs fenced; no resend/refresh/re-stage/guess; real restart limitations declared |
| R4 | Complete owning API-core/Sandbox/daemon-consumer integration needed for S9 | R1–R3 | Real members/exports and actual macOS-host/Docker-consumer path; existing Store reused; no excluded-code fallback or S8 lifecycle substitute |
| E1 | Register S7/S9 workload/cache/observer/verifier contract and build thin runner | T0; can precede A/R implementation | Every selected row has exact route, timer/ack, numerical gates, identities, cache treatment and feasible owning budget; missing rows remain NOT_RUN |
| E2 | Close whole-operation cost model and engine/native attribution | E1; affected A/R source | Actual SQL/VM/row/trigger/copy/queue/request work, failed attempts, all reservation/range/freelist/allocation dimensions; worst/amortized/cumulative derivation correlated with observations |
| E3 | Qualify physical page/I/O and phase residency | E1, E2 | Declared supported observations or contract-permitted conservative bounds; pager/journal/process/kernel/file-cache included; no logical-as-physical or lifetime-as-phase substitution |
| E4 | Qualify sustained service/fairness and reclamation debt | E1, assembled relevant owners | Frozen arrivals/mix; numerical per-class service, queue and eligible debt trace; last-owner automatic live/idle cleanup; no harness cleanup pump or dropped retained owners |
| Q1 | Final full-root/native/runtime public-API qualification | A3–A4, R2–R4, E3 | Real Store profiles; interleaved Saves/history outcomes; huge namespaces and dense >4-GiB acquisition; complete saved-root oracle after source removal; no hidden per-bind scan/restore |
| C1 | Separate milestone exit audits, tracker receipts and exact final handoff | All required packages | Actual exits satisfied or exact residual required dependency stated after independent work; no completion inferred from code existence or component tests |

### S9 supervision, authority and custody details

R1 binds existing `Runtime`/`Sessions`/`Service` and NativeInput/NativeOutput owners.
Socket workers own bounded bytes; the provider-serving scope owns stack-borrowed
Saves. Preserve protected demand/control capacity, byte/count backpressure,
Workspace/class rotation and same-Save ordering. One shared `Calls` connection
serializes bounded exchanges: do not call it pipelined or let one slow peer consume
all unrelated connections' service. Test blocked input and output independently.

R2 checks authenticated peer, Workspace incarnation, exact captured Branch/root,
Store/catalog/profile, inode scope and Save capability. Validate role-derived
references and contextual child meanings, not only canonical hash/root envelope.
Bound validation state in owning indexed storage and windows. Preserve reference
closure, cycles/aliases and permissions; do not bypass them to avoid cost. Initial
root construction is not a shortcut that resolves P6/P7/P13/P14 incremental Commit.

R3 separates connection disconnect, consumer-process restart, host-runtime restart
and provider unknown/quarantine. Close/join or another exact completion fence must
establish which original work can still run; a history read alone cannot. Retain
original queued bodies, attempted receipts and unpublished/live Save ownership in
their actual owner. Old capabilities never become fresh authority after restart.
Disposable backing does not acquire crash recovery from an in-memory registry.
Durable Store persistence does not by itself persist session/attempt knowledge.
Specify any required custody records and their publication/crash boundaries.

P10's existing unresolved resolver policy stays explicit. Implement the supported
typed terminal-unknown behavior and exact fences; do not invent an automatic
resolver. If completing a selected exit requires a new owning resolver policy,
state that exact obligation/dependency, complete independent work, and report it
unmet. S10 history/install recovery must not be silently claimed as S9 progress.

### S7 observations and cost closure details

E2 follows one original operation from caller input through admission, owner SQL,
required immutable demand, returned result/reply attempt, owner release and later
eligible cleanup. Count SQL in loops, bindings/copies, trigger executions, BLOB
calls, failed attempts/rollback, native framing/crypto/IO, queues and retained debt.
Separate nested/inclusive spans; do not sum them as exclusive elapsed time.

For N relevant indexed rows, K visited/returned rows, B processed bytes, Q admitted
owners/jobs, H retained lifetime and G eligible debt, derive all three cost forms.
Use source-qualified point O(log N), suitable covering keyset O(log N + K), and
explicit noncovering fetch, key comparison/page and payload costs. Initial import
must pay source/unique-file bytes and all indexing; it is not metadata-only time.
Reject repeated full scans, OFFSET prefix revisits, whole-file copy-up, quadratic
fragment/prefix work and hidden H-dependent depth. Account actual index write/split
and deletion work rather than assuming SQLite removes it.

E2/E3 include the entire 256-MiB reservation, high-water allocated disk, logical
page count, committed freelist credit, actual count triggers, identity observations
and every range-allocation attempt. Requested reservation bytes are not new disk
consumption; SQL freed rows are not file shrink or global history GC.

Use safe supported observations and external OS/device tools. The pinned safe
driver lacks some pager/internal counters; do not patch it, add unsafe code or
fabricate zeros. Register exact measurement precision/observer overhead and any
contract-permitted bound. If an owner-required exact dimension is unavailable,
keep that dimension incomplete with API/platform evidence; this does not stop
other accounting, observer design or numerical engine work.

E3 samples/attests operation baseline, peak and final residency in both the macOS
host and Docker/Linux owner/consumer scope, including queues, caches, SQLite dirty/
journal/pager state and backing file cache. A sampled peak is labeled sampled.
Do not treat `cache_size`, capacities, memory hints or cgroup lifetime peaks as
whole-system phase bounds. Observer/preparation work must not prime a cold phase.

E4 distinguishes live retained bytes from eligible debt. Freeze offered arrivals,
job/byte mix, per-class latency/progress, headroom and debt budgets before running.
Count drain after acknowledgement and idle cleanup. No next write/status/Commit
is required to drive normal reclamation; no manual benchmark maintenance pump.
Carry original failures and all registered unrun selections into the ledger.

## 6. Qualification and execution rules

Use [optimization policy](../../../../docs/general/optimization-guide.md),
[measurement workflow](../../../../docs/general/agent-measurement-policy.md),
[benchmark rules](../../../../docs/general/benchmark_rules.md),
[report template](../../../../benchmark_agent_report.md) and
[core harness routing](../../../benchmark/fs-bench-pro/AGENTS.md).
The speed-plan draft offers E/R/Q/P case candidates, not registered gates or a
runner. Reconcile each against current source before freezing it. The shared-Calls
and separate-connection cases are different topologies; batch endpoints and
single-ID consumers are different operations. Do not fabricate a speedup ratio.

Mandatory proof matrix:

- Acquisition: canonical-root equivalence; full ignored/dependency/cache/output/
  `.git/index` membership; empty/wide/deep/raw-name trees; broken/external/cyclic/
  opaque symlinks; hard links including outside-root links; distinct equal-byte
  files; source stability; refused kind/mode; source/backing overlap; capacity and
  partial failure; cleanup errors; huge namespaces; actual dense >4-GiB read/save.
- Runtime: actual authenticated host↔Docker delivery; all operation families;
  two interleaved Saves and same-Save reads; policy/serial/length consumers;
  authority revocation/stale capability; contextual invalid objects/roots;
  known conflict/discard, lost replies, terminal unknown/quarantine; precise header/
  body/dispatch/completion/delivery disconnects; process restart and slow peers.
- Engine: startup/readiness, one DB across multiple namespaces, bounded writes/
  composed reads/capture/ownership/reclamation, failure/rollback accounting,
  allocation and full reservation, growing population/lifetime/pressure, live and
  idle cleanup with actual numerical rates/debt and phase resource witnesses.
- Repeated bound use: complete root through public base/consumer boundaries,
  without another full scan/copy/materialization, dependency restore or provider/
  database open. This is component/runtime readiness, not an S8 mounted claim.

Build first using locked `--all-targets --no-run`; run actual changed packages.
Every Rust/Python test invocation has an explicit wall timeout ≤120 s. No
background test or replay/repeat loop. On expiry stop the process/owned children,
retain FAILED and diagnose source/output before any justified repair/rerun.
Docker uses an inner stop fence as well as the outer command limit. Correct the
ungated macOS Store fixture by honest platform gating plus an unsupported-platform
refusal proof; retain its historical Linux failure and do not enable an unsupported
global provider or claim it executed in Docker.

At final affected identity run required covering tests/examples, both applicable
platforms' warning-denying Clippy, fmt, boundary guard and scoped tooling tests.
Seal exact compiled binaries before/after, source/tree, manifests/lock, root Cargo
config, dependency/build/harness/oracle identities, workload/cache and image/kernel.
Reuse unaffected earlier proofs only with relevant scope identity; new acquisition
SQL/profile/behavior invalidates its own predecessor proof, not every old family.
No CI or aggregate pre-push gate; `tools/preflight.sh` remains retired.

Performance: registered release profile (SDK Init explicitly release-only), one
sample per case/arm, fresh append-only outcomes and separate independent proof.
Default complete performance command ≤15 s; declared exceptions ≤25 s; independent
performance proof <10 s, unless an existing owning frozen family has its scoped
contract. The 120-s functional ceiling does not relax these limits. Huge-root/file
cases need a feasible declared owning selection; do not shrink/skip a required case
and mark it passed, increase timeouts after failure or substitute sparse/compressed
logical size for actual dense bytes. If feasibility remains unresolved, it is
registration/engineering work and the selection remains NOT_RUN/INCOMPLETE.

Use `--setup clone` for supported post-initialization cases, an independent writable
byte copy; fresh for initialization/fresh outputs. Reuse setup/sealed builds/proofs,
never a mutated measurement. Declare/enforce each phase's own equal cache state;
no pre-touch, setup warmth, own-write warmth or earlier-phase cache credit. Unknown
or mismatched state is INELIGIBLE/INCOMPLETE. Preserve failed/unrun rows. Freeze
gates before sampling; no retrospective numerical targets or relaxed budgets.

Export `LAYERFS_CONSTRUCTION_WORKERS=1`. Namespace Init alone retains its supported
four constructors. Root ARM64 flags are `--cfg aes_armv8 --cfg polyval_armv8
--cfg chacha20_force_neon -C target-feature=+aes,+sha2`; run from root or repeat
them exactly. Use worktree-local targets/locks and no same-checkout overlap.

The macOS global Store's Durable/Disposable guarantees stay separate. Local
Overlay and disposable scratch get no fsync/fdatasync/sync_data/sync_all. No third-
party patches/forks/vendor/registry edits/new substitution, except the already
authorized exact fuser 0.18.0 signed-timestamp patch. Existing dependencies supply
SQLite. [Fuser provenance](FUSER-REGISTRY-PATCH-20261006.md) and accepted Docker
verification are settled; retain the Linux fractional-minimum FAIL as a platform
limitation. No new release/QEMU/custom-kernel wait; do not mark S8 complete.

Leave these unrelated containers untouched: `9cf2fe345496`, `ce75ac504df9`,
`d2433851ea59`, `d2550144998b`. Root `crates/`, excluded predecessor source,
historical receipts and both side-conversation documents stay preserved.

## 7. Next actions, milestone exits and stopping boundary

The next implementer should execute T0, then deliver A1 and begin R1/E1 as
independent work. A4's inherited FileBacking correction can proceed without waiting
for the new SQL capability. A2/A3 depend on A1's real ownership/compatibility
contract; R2/R3 depend on the assembled original owners. E2–E4 progressively close
the registered evidence at the affected final source. Do not stop at another
passing intermediate checkpoint while any useful authorized package remains.

S7 independent engine acceptance requires complete justified cost/resource/service
evidence, with every required observation/limit addressed. Mounted kernel request/
open/lookup/reply accounting needs S8 and remains a named deferred dimension. Do
not turn that exact dependency into a blanket prerequisite for independent S7.
If the owner's whole-operation S7 exit still includes the mounted dimension, keep
S7 unchecked and report that exact residual requirement after completing its
independent work. Do not quietly narrow the milestone to obtain completion.

S9 requires the actual authenticated assembled runtime, contextual authority,
exact one-attempt custody/fences, faithful bounded full acquisition and real Store/
resource proofs. Component tests, authentication alone or a directory's existence
do not close it. Any S8-dependent native mount/control proof stays explicitly
deferred; required S9 adapter work is still implementable independently.

Every commit records exact first-parent/final-staged/committed production LOC:
core, retained reference and combined before/after/signed delta, using unchanged
`tools/production_loc.py` SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`.
Count shipped SQL, application adapters and excluded predecessors; exclude tests,
docs, tools, manifests, third-party/build output and inline tests. Recompute after
any staged production change and verify the committed tree against the prepared
receipt. Current baseline is core 92680 + reference 65417 = combined 158097.
Docs-only commits report that full unchanged total, delta 0.

Append separate S7/S9 audits and #307 receipts with exact source/tree/binary/workload/
cache identities, original failures, reused scopes and all NOT_RUN/INELIGIBLE rows.
No permission re-request for already-authorized local commits/tracker evidence.
Keep the closed S5/S6 handoff intact. Final handoff must give achieved versus unmet
exits, exact commits/LOC/trees, dirty state, retained custody and concrete next-ready
work. Stop on actual batch completion or a precise required external/deferred gate
after exhausting useful independent work. Unwritten source, observers, registration
or an untested huge-root case are work remaining, not external blockers.

## 8. Plan delivery receipt

Plan/guard-guidance commit: `5900de7331202e8ead57b4c6c7c77941e8c56bbb`, tree `4a11ab6a242f27218a095bf5d2aeffe1d202df58`, first parent
`b2b979f91aadc23a7858d618a438a39a4e6a3edd`. Production LOC remains
core92680/reference65417/combined158097, signed delta0 in each scope, verified
against the exact prepared staged tree. [Committed LOC receipt](checks/s7-s9-plan-20261006/committed-loc.json).
Python compilation preceded 40 scoped tooling tests under a30s wall ceiling;
all passed, with no timeout. The boundary scanned609 production files and the
plan's30 initial local links resolved. Rust production code/schema/profile and
manifests were unchanged, so new Rust runtime checks were inapplicable.

T0's missing tracker receipts are now recorded separately:
[S7 remaining gate](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6010922165) and [S9 checkpoint/correction](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6010923803).
This append does not change the initial inspection's historical tracker state.
S7/S8/S9 remain unchecked. T0 reporting is delivered; A1/A4/R1/E1 are the next
ready implementation packages. None of A2–Q1 is completed by this plan.

## 9. Init working-table clarification (2026-10-06)

Owner-requested documentation update: specify the proposed `init_operation`,
`init_entry` and `init_native_file` tables in the existing global SQLite Store,
their indexed roles and retained schema lifetime. Successful Init deletes its
eligible operation-scoped working rows through bounded jobs; shared tables/indexes
remain. Exact outcome/uncertain custody and global saved objects/history remain
owned under their existing contracts. A1/A2 acceptance and the expected SQL folder
structure now include these requirements. No tables or acquisition APIs have been
implemented by this documentation change; S7/S9 remain incomplete.

First parent: `ff221cd8f3d3bc92a7cc1d2c3901b2dd27b07261`, tree
`a4e53bd1f9e9a68d7dd8d2a8632f6e197f898334`. This update retains core 92680,
reference 65417 and combined 158097 production LOC, delta +0 in each scope.
Its final staged/committed comparison uses the unchanged counter and scope in
section 7; the exact receipt is retained under `core/target/cluster2-307/loc/`.

## 10. A4 delivered and A1 written (2026-10-06)

A4 source: `8bd03d76243987c365a06453d83c45015f72d4a5`, tree
`c0f5436141425f1a7873aff843dfc6e8e7a7b0e3`, first parent
`ee4a647223ed859022f00260689e13e8f2857b5c`. Production LOC core 92680 → 92797
(+117), reference 65417 → 65417 (+0), combined 158097 → 158214 (+117), verified
against the staged tree; [receipt](checks/s9-acquisition-custody/committed-loc.json).
Findings 2, 3 and 4 of section 2 are corrected with public-API regressions, and
the ungated macOS Store fixture is platform-gated with a refusal proof. Checks,
the one retained test failure and limits are in the
[custody receipts](checks/s9-acquisition-custody/identity.json). Finding 1 is
A1–A3 and is not corrected by this commit.

A1 is written as [the acquisition contract](A1-ACQUISITION-CONTRACT.md). It
places the port in Storage and the SQL in Persistence, adds a third `Handles`
field, defines the three tables with the entry key `(operation, parent position,
name)`, selects new schema versions 4–6 rather than changing 1–3, and keeps
working-row cleanup before publication. Its section 9 holds two owner decisions
that gate A2. The current baseline is core 92797 + reference 65417 = combined
158214. A2, A3, R1–R4, E1–E4, Q1 and C1 remain open; S7/S9 remain incomplete.

## 11. A2 delivered (2026-10-06)

Owner decisions: acquisition tables opt-in at Store creation (versions 4–6);
Init on a version 1–3 Store after A3 is a typed refusal; no upgrade operation.

A2 source: `18ac1d9e52fb51d12509235cdc2aa5c241674cb1`, tree
`129e9cf46cb21acd0d679d5635aa12faa9817b6e`, first parent
`bc91285ac5f2625db9b876e682d13b61f5978a47`. Production LOC core 92797 → 94156
(+1359), reference 65417 → 65417 (+0), combined 158214 → 159573 (+1359), verified
against the staged tree; [receipt](checks/s9-acquisition-provider/committed-loc.json).
The Storage port, the three shared tables, 25 shipped statements, the provider
and the plan diagnostic exist and are covered through the public API on real
Stores; see [acquisition backing](../../architecture/44-acquisition-backing.md)
and the [receipts](checks/s9-acquisition-provider/identity.json).

A2's acceptance row is met for the provider itself: the initialized database
and shared tables are reused, jobs are bounded keyset windows, cleanup removes
operation rows without dropping tables, original errors are typed, and every
statement has a product-build plan with a correlated count profile. It is not
met for Project, which does not call the port yet, and no cost beyond statement
and VM-step counts is measured. The current baseline is core 94156 + reference
65417 = combined 159573.

Next ready: A3 (port scan, jobs, aliases and roots to the port following A1
section 7; add the external memory port for Project's tests; remove
`import/runs.rs` and the file-run parts of `import/scratch.rs`; replace
`InitRequest::scratch_parent`), then R1 and E1. S7/S9 remain incomplete.

## 12. A3 delivered (2026-10-06)

Owner decision for this package: the Init vehicle is re-registered as part of A3
with a new frozen identity. Tracker receipts for S7/S9 checkpoints are posted
on #307 without asking again in this session.

A3 source: `0d84badef97f468a7269f9991ab920a8f7077a83`, tree
`1f720b5ea731fabcf0c18e541d4fae4c8d3f5fad`, first parent
`abdb322f42befdbaca9e4fc6d4592f3b39fecbc7`. Production LOC core 94156 → 93991
(−165), reference 65417 → 65417 (+0), combined 159573 → 159408 (−165), verified
against the committed tree; [receipt](checks/s9-acquisition-port/committed-loc.json).

Project's scan, jobs, later-path recheck and roots run on the acquisition port;
the run reader, writer, sorter and record grammar are removed; attribute and
target roots are constructed during the scan; working rows and the operation
record are removed before publication. See
[backed initial acquisition](../../architecture/43-backed-initial-acquisition.md),
[A1 section 11](A1-ACQUISITION-CONTRACT.md) and the
[receipts](checks/s9-acquisition-port/identity.json).

Harness: the vehicle that drives Project's Init example is
`families/phase7_sqlite.py`, not the retained SDK `init_namespace` runner, which
drives the SDK example and was left untouched. `phase7_sqlite.py` now registers
`phase7-sqlite-init-{100,1000,10000,100000}-acquisition-v1` and the matching
`phase7-sqlite-disposable-init-…-acquisition-v1` identities at the default 15 s
command and 9.5 s proof budgets, and refuses every earlier Init selection in
both arms. None is sampled. The earlier Init identities used owner-doubled
30 s/19 s caps; carrying those to a new identity is an owner decision for E1,
not something this package assumed.

A3's acceptance row is met functionally and by count evidence. It is not a
qualification: no time, page, journal, synchronization or resident-memory cost
is measured, and Durable now pays a synchronized commit per write unit that the
scratch files did not. E2/E3 own that measurement.

Two issues outside this package were flagged, not fixed: an older pooled
value-group statement in Persistence binds `LIMIT` plainly and re-prepares per
execution, and 14 Persistence test files are not gated for the macOS-only
Store, so the whole package fails on Linux.

The current baseline is core 93991 + reference 65417 = combined 159408.
Next ready: R1 (host service supervisor and consumer attachment) and E1
(register the S7/S9 workload, cache, observer and verifier contract with a thin
runner). R2 needs R1 and A3's acquired-root witnesses, now available. S7/S9
remain incomplete.


## Namespace Init first-selection checkpoint (2026-10-06)

The owner directed a prospective budget lift for the fresh namespace benchmark. Local45e2b09e8 registers acquisition-v2 at30 s complete performance/19 s independent verification with all other gates unchanged. V1 remains unsampled at15/9.5 s. The initial four paired100/1000 Durable/Disposable cases have run exactly once per arm. All cold-content, sampled proof, root equality, cleanup and absolute-budget checks pass; only Disposable100 passes the joint gate, with three speed FAILs retained. Larger tiers remain NOT_RUN. This is a component follow-up after A3, not completion of E1–E4 or S7/S9, and starts no R1–R4 code. Next ready work is count-driven diagnosis of the retained speed failures and independent R1/E1 implementation.

Exact identities, commands, raw receipts, gates and next-work custody are in the
[immutable component report](NAMESPACE-INIT-ACQUISITION-RESULTS-20261006.md).
The S5/S6 stopping-boundary record remains unchanged.


## Acquisition window execution checkpoint (2026-10-06)

Owner-directed performance correction after A3: retain statement leases within entry/place units and batch file/directory roots through fixed32-row SQL inputs. Keep three acquisition tables, opt-in schema versions, public ports, row/byte windows and transaction profiles unchanged. Count/profile/atomicity evidence is captured in the execution-fix checks; changed-source candidate qualification is registered in docs/roadmap/0.1/0.1.7/namespace-init-acquisition-window-fix-20261006.md, reusing qualifying unchanged reference rows. No new R1–R4 implementation or S7/S9 completion is claimed.

[Source, diagnostics, checks and retained failures](checks/init-acquisition-fix-20261006/identity.json).


## Execution-fix qualification checkpoint (2026-10-06)

Source4c03b41bf has qualified changed-source component receipts, reusing exact
unchanged reference evidence. All cold-content/functional/root/cleanup/absolute
checks pass. Disposable100 remains the only joint PASS; Durable100/1000 and
Disposable1000 retain relative-speed FAIL. Disposable1000 is145382875 ns versus
152242291 ns before and124912209 ns reference. The full speed issue is not resolved.
[Exact report, reuse, arithmetic, source LOC and next work](ACQUISITION-WINDOW-FIX-RESULTS-20261006.md).
Milestones remain CHECKPOINT; no full-root/runtime/engine/resource acceptance is inferred.
