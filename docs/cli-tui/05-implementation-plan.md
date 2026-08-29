# LayerFS Phase One backend implementation plan

Status: **binding handoff plan**

Phase One cold-implements the complete LayerFS backend and standalone CLI. It
includes every production responsibility except `layerfs-tui`. The goal is the
smallest coherent final implementation, not the smallest patch against the
current checkout and not a compatibility wrapper around two architectures.

The handoff agent owns implementation, migration, focused verification,
performance proof, and terminal closure. A failed gate, `REVISE`, or `No-Go`
requires root-cause correction and replanning; it is not a stopping condition.

## 1. Binding inputs and precedence

Read completely before editing:

1. `00-overview.md`
2. `01-topology-and-source-tree.md`
3. `02-cli-sdk-contract.md`
4. `04-monitoring-dedup-performance.md`
5. this plan
6. `03-tui-design.md`, only to understand the frontend contract that Phase One
   must expose; do not implement its Ratatui/Crossterm presentation

The exact source-tree manifest in `01-topology-and-source-tree.md` is binding,
except that `layerfs-tui` is absent until Phase Two. Reconcile or explicitly
supersede conflicting older application-facing sections in `docs/`; never
leave both old and new architectures normative.

## 2. Phase boundary

```text
PHASE ONE — this plan

Content -> Storage -> Layer/Stack/Branch Stores
                    -> Workspace -> FUSE/materialization
                    -> Monitor -> thin SDK -> standalone/reusable CLI
                    -> benchmarks and terminal evidence

PHASE TWO — 06-tui-implementation-plan.md

layerfs-tui -> frozen layerfs-cli frontend contract
```

Phase One must not create, scaffold, compile, test, or depend on
`layerfs-tui`, Ratatui, or Crossterm. It ends with a fully usable headless
product and a frontend-contract fixture. Phase Two must not be required to add
a backend operation, query, event, lifecycle transition, or metric formula.

APFS immutable-base/clone acceleration, GC, rollback policy, automatic retry,
and elaborate crash/network recovery remain deferred. Portable materialization,
real host/container FUSE, SQLite atomicity, exact CAS, ordering, bounded
resources, and no implicit Commit remain mandatory.

## 3. Non-negotiable architecture

```text
LayerStore (1)
|-- StackStore (0..N)
|     `-- BranchStore (0..N)
`-- direct BranchStore (0..N)

BranchStore (1)
`-- Branch (0..N)
      `-- Workspace session (0..N)
```

Publication routes:

```text
direct:  Workspace -> BranchStore -> LayerStore
stacked: Workspace -> BranchStore -> StackStore -> LayerStore
```

Deployment may be embedded, local, remote, or hybrid without changing Store
semantics. SQLite is local to the process that owns it; remote access uses a
typed Store endpoint, never a network-mounted SQLite path.

The standalone CLI requires persistent runtime ownership:

```text
terminal A --\
terminal B ----> bounded local typed socket -> one host per CLI context
future TUI ---/                              |-- Store connection graph
                                                |-- Monitor collector
                                                `-- Workspace worker w1..wN
```

The context host lives in `layerfs-cli`; it is not a new package, public
server, or generic daemon. Each Workspace UUID has one logical worker that owns
its transient COW state, projection, executions, writer gate, output, and
lifecycle serialization. Workers do not independently open Store databases.
Host, worker, CLI, FUSE, helper, and container exits never imply Commit.

## 4. Smallest frozen application surface

### 4.1 Store construction: six typed methods

```rust
LayerStore::create(location)
LayerStore::connect(location)
StackStore::create(location, layer_endpoint)
StackStore::connect(location, layer_endpoint)
BranchStore::create(location, parent_endpoint)
BranchStore::connect(location, parent_endpoint)
```

There is no generic role-dispatched constructor, ambiguous `open`, public raw
storage handle, Direct/Stacked constructor pair, or hidden parent.

### 4.2 Store semantics: twelve application methods

```rust
LayerStore::initialize(LayerInitialization)
StackStore::pull_layer(LayerId)
LayerStore::add_layer(LayerSource)

StackStore::create_stack(LayerId)
StackStore::pull_stack(StackId)
StackStore::add_stack(BranchCommit)
StackStore::push_stack(StackId)

BranchStore::create_branch(BranchSource)
BranchStore::merge(source_branch_id, target_branch_id)
BranchStore::pull_branch(BranchId)
BranchStore::push_branch(BranchId)
BranchStore::pull_commits(BranchId)
```

The lower Store layer retains the exact validation/CAS primitives required by
the storage specification. The application surface derives history IDs and
expected heads, unifies equal-effect source variants, and exposes no SQL,
object admission, transfer frame, wire, or `FullStorage` API.

### 4.3 Workspace lifecycle: three explicit methods

```rust
Workspaces::create_workspace_session(request)
Workspaces::commit_workspace_session(session_id)
Workspaces::end_workspace_session(session_id, Clean | Discard)
```

Create requires Branch plus placement and accepts only an optional
`Fuse|Materialize` override. Commit computes the final delta and exact-CASes
once. End performs cleanup only; dirty Clean refuses and Discard drops the
delta. No alias named `begin`, `finalize`, or implicit `capture` survives.

Workspace execution is only `exec(nonempty_argv)`, `shell()` using the selected
environment's default shell, and `stop(execution_id)`. Workspace queries are
sessions, one session detail, diff, and output. Do not add separate execution
list or Workspace history methods; detail/list already contain them.

### 4.4 Monitor and frontend

```rust
Monitor::snapshot(MonitorScope)
Monitor::analyze_dedup(BranchStoreId)

CliSession::open(context_location)
CliSession::parse_line(input)
CliSession::plan(command)
CliSession::execute(command)
CliSession::complete(input, cursor)
CliSession::snapshot(query)
OperationHandle::interrupt()
```

Recorder creation is internal. One bounded `CliEvent` enum carries Started,
Progress, ordered Output, Snapshot, and Finished(Result + receipt). Human
output, JSON, the test client, and Phase Two consume these exact types. No
one-implementation trait, factory, generic event framework, or formatted-output
parser is justified.

## 5. Stage B0 — freeze contracts and cold-reset packages

### Work

1. Freeze public value manifests for Content, Storage, Store records/results,
   Workspace, Monitor, SDK, CLI commands/plans/events/snapshots, typed IDs, and
   JSON schema version.
2. Freeze the exact CLI grammar from `02-cli-sdk-contract.md` with parser
   fixtures before behavior.
3. Rename/reorganize `layerfs-core` to `layerfs-content`.
4. Replace old `layerfs-storage` and rename/reorganize current
   `layerfs-storage-core` into one clean `layerfs-storage`.
5. Keep and refocus one `layerfs-workspace`; do not create Overlay/Core/session
   sibling packages.
6. Rename `layerfs-mount` package/binary/container references to
   `layerfs-fuse`.
7. Add `layerfs-monitor` and `layerfs-cli` with only the exact files from the
   exact source tree. Do not add `layerfs-tui`.
8. Delete fixed public Direct/Stacked façades, ambiguous public Store `open`,
   SDK Workspace/Monitor implementations, mount-owned topology, implicit
   finalize-on-exit, and compatibility wrappers without a real consumer.
9. Establish dependency bans with `cargo tree` and source audits.

### Gate

```text
cargo check -p layerfs-content
cargo check -p layerfs-storage
cargo check -p layerfs-layer-store
cargo check -p layerfs-stack-store
cargo check -p layerfs-branch-store
cargo check -p layerfs-workspace
cargo check -p layerfs-materialization
cargo check -p layerfs-fuse --no-default-features --features proxy
cargo check -p layerfs-monitor
cargo check -p layerfs-sdk
cargo check -p layerfs-cli
```

Prove Content purity, Store independence, no domain dependency on Monitor, no
SDK Workspace/Monitor implementation, no TUI dependency in CLI, no SQLite/Store
dependency in the FUSE proxy, and no TUI package.

## 6. Stage B1 — Content and Storage foundation

### Work

1. Implement the exact Content dependency direction:

   ```text
   object <- {file, tree} <- filesystem
   ```

2. Delete overlapping legacy `identity`, `content`, `namespace_*`, and
   `logical` axes after moving their responsibilities.
3. Keep ObjectId, canonical encoding/authentication, CDC, file state,
   paths/directories/inodes/metadata, references, and pure whole-filesystem
   read/apply/diff/merge in Content.
4. Keep typed Layer/Stack/Branch/Commit records, exact DDL/SQL, candidate
   memory/scratch spill, merge-base selection, admission, exact CAS,
   missing-only transfer, wire, and raw domain receipts in Storage.
5. Move spool paths and readers to Workspace. Content receives bounded readers;
   no host `PathBuf` or Store-domain ID enters canonical records.
6. Implement final-state canonicalization. A deterministic final view must not
   inherit tree partitioning, new-inode allocation, or file-root differences
   from the sequence of edits that created it.

### Focused proof and gate

```text
Content works against an in-memory ObjectStore
same bytes/metadata/hard-link graph -> same root and ObjectIds
same pinned base/final view through different edit sequences -> same root
candidate memory and scratch spill emit identical canonical sequences
stored canonical transfer never rechunks, re-encodes or remints ObjectIds
no duplicate reference/CDC/merge implementation in Storage

cargo test -p layerfs-content
cargo test -p layerfs-storage identity
cargo test -p layerfs-storage candidate
cargo test -p layerfs-storage merge_base
cargo tree -p layerfs-content
```

## 7. Stage B2 — minimal Store schemas, Create/Connect, topology

### Binding schemas

```text
BranchStore: 3 tables / 9 columns
StackStore:  8 tables / 24 columns
LayerStore:  8 tables / 24 columns, identical Full DDL to StackStore
```

Workspace, output, connection context, retry, recovery, transfer, metrics, and
monitoring add zero Store tables.

### Work

1. Implement explicit Create and Connect for every Store role.
2. Create fails rather than adopting or replacing an existing Store.
3. Connect rejects missing, wrong-role, wrong-schema, and incompatible parent
   without creating or mutating.
4. Local SQLite supports Create/Connect. A remote endpoint supports Connect;
   remote Create exists only when the endpoint explicitly exposes management
   capability.
5. Validate parent compatibility from immutable lineage using fixed pages; do
   not add a generic topology table or one RPC per Branch.
6. Preserve one Layer -> N Stack/direct Branch and one Stack -> N Branch.
7. Implement the minimum `layerfs-cli::{context,host,control}` runtime now: one
   bounded local typed socket, one context-scoped process, and one owner for
   each connected local Store. Later stages add Workspace/FUSE/Monitor behavior
   to this owner; they do not invent another runtime. Workspace workers never
   reopen Store databases.
8. Protect host startup with one atomic context lock, a `0700` owner-only
   runtime directory, owner-only socket permissions, peer-UID validation where
   supported, and a spawn/READY handshake. The losing concurrent starter joins
   the winner. Stale PID/socket removal requires the lock plus ownership/type
   validation and never implies Workspace recovery or Commit.
9. Refuse dependency-breaking disconnect: active Workspace -> BranchStore,
   attached BranchStore -> StackStore, and dependent Stack/direct BranchStore
   -> LayerStore all return a typed busy/dependency result.
10. Freeze the internal optional Store observation capability: sorted paged
    `(ObjectId, encoded_length)` inventory and Store-owned DB/WAL/SHM snapshot.
    Unsupported remote endpoints return `ObservationUnavailable`; never infer.

### Focused proof and gate

```text
create new succeeds; create existing makes zero changes
connect existing succeeds; connect missing never creates
wrong role/schema/parent rejects before writes
exact 3/9 and 8/24 schema manifests; Full DDL byte-identical
one Layer -> N Stacks + N direct BranchStores
one Stack -> N BranchStores; no silent reparenting
second local Store owner returns StoreBusy
two simultaneous starters yield one READY host and one connected loser
socket/runtime permissions and peer UID block another local user
validated stale endpoint cleanup never commits or fabricates recovery
dependent Store disconnect refuses without changing context
WAL and synchronous=FULL are applied consistently

cargo test -p layerfs-storage schema
cargo test -p layerfs-layer-store connection
cargo test -p layerfs-stack-store connection
cargo test -p layerfs-branch-store connection
```

## 8. Stage B3 — Store operations, transfer, CAS/CDC deduplication

Complete lower Store operations and receipts before hiding them behind the
smaller application SDK.

### Rules

```text
Push   = transfer missing immutable objects/facts and visibility only
Add    = conflict check + create Stack/Layer + exact history-head CAS
Commit = create one immutable Commit + exact Branch-head CAS
Pull never performs hidden Merge; Add never performs hidden Pull
```

Every Add Stack and Add Layer uses the shared pure three-way algorithm and
returns one clean result, UpToDate, or the first deterministic conflict. Branch
Merge pins the target head and exact-CASes once. There is no automatic
overwrite, retry loop, or last-writer-wins.

For canonical objects and every typed fact kind separately:

```text
announced = preexisting ⊎ missing
sent      = missing
missing   = inserted ⊎ raced_existing
```

Known roots prune whole subtrees. The receiver performs indexed membership
queries; the sender transmits stored canonical bytes only for missing IDs.
Network, hashing, CDC, signature verification, and three-way evaluation never
run inside a SQLite write transaction. Visible heads publish last.

Hot-path limits:

```text
membership page     512 same-kind IDs
missing bitmap      fixed 64 bytes
object insert batch <=128 rows, normally <=4 MiB
large singleton     <=16 MiB
fact insert batch   <=128 rows, <=64 KiB
```

Let `J` be the number of object insertion batches and `F` the number of typed
fact insertion batches after membership negotiation. SQL trace tests enforce:

```text
local Commit              <= max(1, J) write transactions
clean Add Stack/Layer     <= max(1, J) write transactions
Pull/Push + visible head  <= max(1, J + F) write transactions
pull_commits              <= J + F write transactions
all-known UpToDate        = 0 writes
Conflict/rejected preflight = 0 production-row writes
```

The final object batch folds Commit/AddResult plus exact CAS into its last
transaction; Pull/Push folds visibility into the last admission transaction.
Do not add a separate transaction merely to publish a head.

Hard application-memory gates, excluding the frozen SQLite page cache and fixed
runtime overhead:

```text
transfer object buffers                       <34 MiB
three-way + candidate/deferred memory          <=8 MiB before scratch spill
one active Store operation                     <42 MiB
Workspace final-delta memory                   <=configured cap (default 8 MiB)
live output tail per execution                 <=configured cap (default 1 MiB)
queued Store callers allocate no operation working set before admission
```

### Focused proof and gate

```text
fixed indexed membership query and short-page NULL padding
known-root pruning before payload reads
child-before-parent admission; visible ref/head last
first cold and all-known no-op Pull/Push
all-known path reads zero payload and writes zero rows
object/fact receipts and local/race reuse remain separate
SQL trace satisfies every J/F transaction equation
concurrent same-ID admission partitions inserted/raced exactly
large file/Stack provenance remains bounded and streamed
64 MiB final file, large DAG/provenance and unbounded output stay within caps
linear/diamond Commit DAG traversal is not quadratic
exact-head race returns HeadMoved without overwrite/retry
direct/stacked embedded and loopback semantics match

cargo test -p layerfs-storage admission
cargo test -p layerfs-storage transfer
cargo test -p layerfs-storage ancestry
cargo test -p layerfs-storage receipt
cargo test -p layerfs-layer-store
cargo test -p layerfs-stack-store
cargo test -p layerfs-branch-store
```

## 9. Stage B4 — Workspace COW and final-delta Commit

### Worker ownership

```text
owns: pinned Branch head/base root, COW namespace, bounded spool/dirty index,
      projection handles, executions, writer gate, output and lifecycle

does not own: Store DB/authority, Push/Add, Monitor formulas, terminal UI
```

The B4 internal creation path pins `(branch_id, expected_head_commit_id,
base_root_id)` and allocates UUID/worker/COW/spool. It writes zero Store rows.
The public `create_workspace_session` is not complete and must not return until
B5 attaches the requested projection and reaches READY.

`commit_workspace_session` performs exactly this order:

```text
1. stop accepting new executions and writes
2. boundedly drain already-entered short filesystem callbacks
3. if an execution, interactive shell, long-lived writable handle, or backend
   writer remains, return WorkspaceBusy immediately, reopen admission, and
   preserve the exact Workspace state; never wait indefinitely
4. quiesce projection handles and freeze mutation generation
5. compare final view with pinned base
6. collapse intermediate/cancelled actions
7. emit one canonical-path-ordered final delta
8. stream changed final files through the one CDC/canonical path
9. reuse unchanged base subtrees; deterministically rebuild touched spines
10. stage missing candidate objects through Storage
11. create at most one Commit and exact-CAS the Branch once
12. success -> read-only; HeadMoved/error -> preserve active final delta
```

Initially, stream each changed final file from its beginning through CDC. An
incremental chunk-rejoin optimization is allowed only after it proves identity
equivalence with complete canonical rechunking.

`end_workspace_session(Clean)` only cleans a clean/committed session; dirty
state returns `WORKSPACE_DIRTY`. `Discard` drops transient state. End never asks
Content for a delta, admits an object, creates a Commit, or moves a head.

### Focused proof and gate

```text
Create changes zero Store rows; concurrent sessions remain isolated
write/truncate/fsync variants with same final file -> same root/ObjectIds
create/delete and rename/undo cancellation -> no durable change
10,000 intermediate tool/FUSE operations -> one Commit maximum
zero durable tool-operation rows or facts
unchanged final view -> UpToDate, zero Commit/head mutation
WorkspaceBusy, HeadMoved preservation, read-only success, second-Commit reject
dirty Clean End rejects; Discard creates no Commit/head move
worker/host/unmount/process exit never commits

cargo test -p layerfs-workspace
cargo test -p layerfs-content final_state
```

## 10. Stage B5 — local context host, FUSE first, materialization

Implement `layerfs-cli::{host,control,context}` early enough to own Store
handles and Workspace workers across separate CLI processes. Use one bounded
typed local socket and the frozen Command/Event values. Do not invent a network
service, auth system, plugin protocol, or new crate.

`layerfs-fuse` owns only its port, callbacks, inode/handle mapping, mount
lifecycle, bounded session protocol, and thin proxy. It never opens a DB,
constructs topology, invokes Branch Commit, or commits on unmount.

Host FUSE is the first FUSE semantic implementation and functional proof.
Portable materialization is the host fallback/control and must produce the same
canonical root. Docker thin-FUSE is the authoritative repeatable performance
environment. APFS clone acceleration remains deferred.

### Focused and real-FUSE proof

```text
context host owns each Store once; workers use typed host-owned access
create returns only after projection READY; later CLI process reconnects
host/worker crash never commits
lookup/getattr/readdir/open/read/write/create/rename/unlink/fsync/release
read-only projection after Commit
FUSE/materialized equivalent final view -> same root/ObjectIds
unmount/end never commits
```

On a FUSE-capable Linux host/runner, run real `/dev/fuse` direct and stacked
functional routes. For each, save under the Phase One evidence root: exact CLI
commands, `mountpoint`/`findmnt`, source tree hash, stdout/stderr, semantic root,
operation receipt, explicit Commit + Branch reopen/read, explicit End/unmount,
process/mount cleanup, and proof that exit created zero extra Commit. This is a
functional gate, not the authoritative cross-machine performance comparison.

## 11. Stage B6 — Docker thin-FUSE

The container receives only Workspace UUID, host endpoint, ephemeral
capability, mount path, and a thin injected/read-only helper. It receives no
Store DB/path/credential, SDK, or Store implementation. One trusted FUSE-ready
container may host multiple isolated UUID mounts.

### Focused proof and gate

```text
proxy cargo tree has no rusqlite/Store/SDK
container has no DB path/file/credential
host/container projection behavior matches
two UUID mounts remain isolated; ending one leaves sibling usable
capability A cannot address Workspace B
helper/container crash never commits
stdout/stderr remains host-side after removal

cargo test -p layerfs-fuse
cargo test -p layerfs-workspace container_workspace
```

Then run one real direct and one real stacked Docker `/dev/fuse` smoke.

## 12. Stage B7 — Monitor

Raw counters/receipts are created by their domain owners during B1–B6. Monitor
composes them; it does not add a second hot-path algorithm.

It owns operation IDs/spans, queue/service timing, receipt composition, per-DB
CAS and DB/WAL/SHM allocation, dedup formulas/coverage/placements/route union,
Workspace transient versus committed separation, CPU/RSS sampling, Branch
aggregation, fixed histograms, bounded JSONL retention, and frontend-neutral
snapshots. It owns no mutation, Store table, execution output, daemon, or
per-object/FUSE-call trace.

### Focused proof and gate

```text
all semantic outcomes close one service receipt; standalone invocations close
one post-render outer receipt sharing the same OperationId
per-process nested/self timings reconcile; cross-process durations never add
as aligned children; queue remains separate
object/fact/local/raced formulas reconcile
known-root result is not-measured rather than fake 100%
10 equivalent installs -> 90%, 10 -> 1, saved bytes, 10x
2/3 Store placements remain separate; transient bytes excluded
route union uses O(number of Stores) application memory
100,000 receipts and JSONL retention stay bounded
passive instrumentation of a semantic operation adds zero Store
query/turn/hash/CDC/transaction
explicit Monitor snapshot/analyze queries are read-only, separately timed,
outside the writer gate, and never attributed to the semantic operation
unsupported remote observation renders unavailable rather than estimated

cargo test -p layerfs-storage receipt
cargo test -p layerfs-monitor
```

## 13. Stage B8 — thin semantic SDK

SDK owns Store graph composition, source values/history derivation, internal
expected-head pinning, typed Workspace/Monitor handles, frontend-neutral query
results, and receipt forwarding. It does not implement Workspace, Docker,
projection, Monitor formulas/sampling/retention, output, CLI grammar, or UI.

### Focused proof and gate

```text
one Layer -> N Stack/direct Branch; one Stack -> N Branch
source IDs derive correct history; wrong history rejects before writes
Merge pins target and detects race
direct/stacked application behavior matches
same accepted final state through either route -> same Layer root
all-known operations remain zero-write/no-payload
SDK has no Workspace/Monitor implementation module

cargo test -p layerfs-sdk api
cargo test -p layerfs-sdk topology
cargo test -p layerfs-sdk conflict
cargo test -p layerfs-sdk dedup
```

## 14. Stage B9 — complete standalone/reusable CLI

Implement only the compact grammar in `02-cli-sdk-contract.md`:

```text
db        create/connect/use/disconnect/list
layer     init/pull/add/list/show
stack     create/pull/add/push/list/show
branch    create/merge/pull/push/pull-commits/list/show/diff
workspace create/shell/exec/stop/commit/end/list/show/diff/output
monitor   db/dedup/workspace/branch/operation/process
```

The CLI persists only non-secret context/socket/Store locations; reconnects
each invocation to the context host; parses argv/quoted lines once; resolves
non-mutating plans; dispatches typed calls; emits one bounded event stream;
renders human/versioned JSON from it; preserves stdout/stderr order; exposes
completion, paged snapshots/output, follow/interrupt; and works without TUI.

### Focused proof and gate

```text
every command/invalid combination; no aliases/legacy grammar
local/remote Create/Connect capability matrix
cross-process host/session flow and quoted argv preservation
human/JSON equivalence; large output bounded and nonblocking
typed exit codes; direct and stacked headless lifecycles
one Branch with simultaneous host/Docker Workspaces
CLI cargo tree has no TUI library

cargo test -p layerfs-cli
```

A non-Ratatui fixture must open context, parse/complete/plan, execute/interrupt,
consume every event, page topology/history/Workspace/output/Monitor snapshots,
and observe stable IDs, parents, generations, cursors, and timing receipts.

## 15. Stage B10 — Phase One terminal closure

### Commands

```text
cargo fmt --all -- --check
cargo test -p layerfs-content
cargo test -p layerfs-storage
cargo test -p layerfs-layer-store
cargo test -p layerfs-stack-store
cargo test -p layerfs-branch-store
cargo test -p layerfs-workspace
cargo test -p layerfs-fuse
cargo test -p layerfs-materialization
cargo test -p layerfs-monitor
cargo test -p layerfs-sdk
cargo test -p layerfs-cli
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The Phase One Cargo workspace has no TUI crate, so whole-workspace closure is a
backend/CLI proof.

### Structural proof

```text
exact Phase One tree; layerfs-tui absent
no old core/storage-core/mount/fixed Direct/Stacked façade
no handwritten production file >1,500 lines, excluding sql.rs and schema.rs
declaration/re-export-only lib.rs; bootstrap-only binaries
no common/utils/manager/product/repository catch-all
no duplicate Content/Storage/transfer/Workspace algorithm
no Store table beyond 3/9 and 8/24
no SDK Workspace/Monitor implementation
no FUSE DB/topology/Commit ownership; no Docker DB/credential
no APFS clone scaffolding or unidentified compatibility façade
```

### Runtime and performance proof

```text
direct/stacked headless DB-to-Layer lifecycle
same-final-state identity across different operation histories
multi-writer HeadMoved with preserved Workspace delta
host-FUSE direct/stacked functional matrix on a capable runner
Docker thin-FUSE direct/stacked authoritative performance matrix
host materialization functional/control matrix
multiple Workspaces per Branch/trusted container
ten-install exact dedup/placement receipts
bounded output/receipt retention and consistent elapsed spans
frontend fixture with no Ratatui
```

### Frozen `fs-bench` campaign

Use the unchanged repository benchmark:

```text
script:  containers/layerfs-fuse/fs-bench.sh
SHA-256: 0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef
verify:  containers/layerfs-fuse/verify_fs_bench.py
```

Before every campaign, verify the checksum and prove `/workspace` is the real
intended kernel FUSE mount using `mountpoint`, `findmnt -T /workspace`, and
container inspection. It must not be a bind, volume, ordinary directory,
tmpfs, materialized substitute, or benchmark-only path.

After source freeze, run four authoritative Docker thin-FUSE populations:

```text
direct route  + BASE=/var/tmp
direct route  + BASE=/tmp
stacked route + BASE=/var/tmp
stacked route + BASE=/tmp
```

Each uses:

```text
MOUNT=/workspace
REPS=3
WARMUP=1
RANDOMIZE_TARGETS=1
SCENARIOS=<all twelve unchanged offline scenarios>
OUTPUT_JSON=<artifact>/raw.json
bash /usr/local/bin/fs-bench.sh > <artifact>/stdout 2> <artifact>/stderr
```

Run measured containers offline with the frozen architecture/resource settings
from the existing authoritative workflow: native `linux/arm64`, one CPU,
512 MiB memory, 512 PID limit, `/dev/fuse`, `SYS_ADMIN`, non-privileged,
`/tmp` as a 1 GiB tmpfs, and no bind/volume at `/workspace`. The verifier must
exit zero with `PASS_OPTIMIZED`; do not weaken its existing functional or hard
performance gates after observing results.

Write new artifacts under
`poc/evidence/phase-one-backend-<UTC>/` and retain raw JSON/stdout/stderr,
verification receipt, SHA-256 values, mount/container inspection, source HEAD
plus dirty-tree hash, image/executable identity, environment, topology and DB
locations, semantic roots, resource receipts, and cleanup proof. Because the
unchanged authoritative run has `n=3`, report median/min/max and label p95 as
insufficient; do not publish the script's nearest-rank max field as a
statistically supported p95. Any separate p95 claim requires at least 20 raw
samples.

Also run Store-transfer fixtures for cold/all-known Push, 2-DB/3-DB,
large/many-small files, large Stack provenance, ten equivalent installs,
concurrent admission/writers, and Monitor off/on. Retain semantic result,
object/fact bytes, pages, frames/turns, transactions, commit-sync,
queue/service time, peak memory, query plans, inventories, and DB/WAL/SHM
allocation. Ignore isolated sub-percent noise; investigate reproducible or
hard-bound regressions.

## 16. Handoff execution rules

1. Implement B0 through B10 in dependency order. Parallelize only independent
   work with disjoint file ownership.
2. Prefer deletion and one shared owner over adapters, compatibility layers,
   duplicate algorithms, or topology-specific implementations.
3. Run the smallest focused gate after each semantic/responsibility change,
   then broader dependent suites only after it passes.
4. Fix root causes in the shared owner. Never weaken tests, remove gates, retry
   unchanged failures, or patch each caller around a bad shared API.
5. Preserve unrelated user changes in the dirty worktree.
6. Do not stop at `REVISE`, `No-Go`, a failed test, conflict, or regression.
   Replan, correct, and continue until every applicable gate has raw evidence.
7. Claim terminal pass only from the frozen final source and current raw proof,
   never prose, historical output, or an earlier revision.
8. Do not begin Phase Two. Hand off the frozen CLI frontend contract and Phase
   One evidence to `06-tui-implementation-plan.md`.
