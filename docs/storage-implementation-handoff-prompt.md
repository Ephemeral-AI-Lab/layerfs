# LayerStack local storage implementation handoff prompt

> **Historical and superseded.** Do not execute this prompt for V2; use
> [`v2/implementation-handoff-prompt.md`](v2/implementation-handoff-prompt.md).

Copy the prompt below into the implementation owner's task. This is an
execution mandate. The five binding documents named below remain authoritative
for architecture, operations, database mechanics, source ownership, and phase
gates.

---

You are the sole implementation owner for the LayerStack cold storage rewrite
in:

```text
/Users/yifanxu/Ephemeral-AI-Lab/layerfs
```

Your objective is to implement and independently verify both supported local
topologies on the exact target architecture:

```text
Direct / two physical databases

application or mount
        |
        v
BranchStore.sqlite ----------------------> LayerStore.sqlite
       local work          Push Branch      central accepted truth
                            Add Layer
```

```text
Stacked / three physical databases

application or mount
        |
        v
BranchStore.sqlite ------> StackStore.sqlite ------> LayerStore.sqlite
       local work           intermediate history      central accepted truth
                 Push Branch / Add Stack   Push Stack / Add Layer
```

Both modes are mandatory. A green direct mode with stacked mode missing, or a
green stacked mode with direct mode bypassed, is not completion. The databases
must be physically distinct local SQLite files opened only by their owning
Store. Do not emulate either topology with one database, attached schemas,
temporary copies, in-memory substitutes, or role flags in a shared file.

Real Linux FUSE is the priority presentation and performance path. As soon as
the direct storage slice works, route it through the real kernel FUSE mount and
keep that route green while stacked mode is added. After the stacked storage
slice works, prove it through the same mount implementation. Do not defer FUSE
until every secondary consumer is complete, and do not spend time on
materialization/APFS before both FUSE topologies pass their functional and
authoritative `fs-bench` gates.

Continue until the same frozen source earns an honest
`PASS_LAYERSTACK_LOCAL_STORAGE`. A plan, scaffold, compatibility facade,
partial vertical slice, passing unit test, benchmark smoke, `REVISE`, or
implementation-caused `NO-GO` is not completion.

## 1. Read before editing

Read these files completely, in this exact order:

```text
docs/model.md
docs/rule.md
docs/db-transaction-transfer-model.md
docs/source-tree.md
docs/implementation-plan.md
```

Then inspect the current workspace, manifests, callers, retained Core
primitives, mount path, evaluation tooling, Docker/FUSE harness, and previous
`fs-bench` evidence. Use `rg` to trace every caller before moving a shared
owner. Historical `poc-*`, research, old Durable/Working documents, and current
legacy crates are implementation evidence only; they do not override the five
binding documents.

Authority order is:

1. `model.md`: three Stores, histories, identities, and 3/8/8 schema boundary.
2. `rule.md`: exactly fourteen public operations and their semantic rules.
3. `db-transaction-transfer-model.md`: Store API, SQL, transaction,
   transfer, deduplication, ordering, memory, and performance mechanics.
4. `source-tree.md`: exact production file ownership and SRP boundaries.
5. `implementation-plan.md`: deletion order, implementation phases, test
   gates, performance gates, and terminal proof.

If the documents genuinely contradict one another, find the smallest coherent
correction that preserves the agreed mental model, update all affected binding
documents, record the reason, and immediately resume implementation. Do not
silently pick the current code's behavior. Do not stop for routine
clarification that the repository and binding documents can answer.

Before the first edit, record:

```text
git rev-parse HEAD
git status --short
cargo metadata --no-deps
```

Preserve unrelated user changes. Never use destructive Git commands.

## 2. Fixed mental model

`LayerStack` names the complete architecture. It is not a Store, table, ID,
history, or object.

```text
BranchStore
    builds Branches and immutable Commits
    stores only locally created CAS payload
    reads accepted Layer/Stack payload through its configured parent
    never copies the accepted base merely to create or pull a Branch

StackStore
    is optional
    creates and owns selected linear StackHistories
    receives Branches and creates Stacks through Add Stack
    transfers signed Stack suffixes and complete accepted Branch provenance
    is never the central Layer authority

LayerStore
    is the single central accepted truth
    stores complete central Branch/Commit/Stack/Layer provenance
    owns authoritative LayerHistory heads
    creates Layers only through Add Layer
```

The supported cardinalities are:

```text
LayerStore:  exactly 1
StackStore:  0 or more
BranchStore: 1 or more
```

For this task, prove one local direct topology and one local stacked topology.
The SDK must accept explicit Store handles/endpoints and explicit database
paths. No YAML, environment-selected hidden Store, automatic database creation
outside the explicit constructor, runtime topology guessing, or raw database
handle may cross the SDK boundary.

The only histories are:

```text
CommitHistory = Commit parent DAG; no history ID or table
StackHistory  = strict immutable Stack list with one exact-CAS head
LayerHistory  = strict immutable Layer list with one exact-CAS head
```

Push transfers missing data. Add performs three-way evaluation and moves a
history head. Never collapse Push and Add, hide Add inside Push, or make Add
perform a hidden Pull.

BranchStore has no Add Stack or Add Layer authority. StackStore alone creates
Stacks for a writable StackHistory. LayerStore alone creates Layers.

## 3. Exact public surface and local routes

Implement exactly the fourteen operations frozen in `rule.md`:

```text
create_branch_from_layer(layer_history_id, layer_id)
create_branch_from_stack(stack_history_id, stack_id)
create_branch_from_commit(source_branch_id, source_commit_id)
commit(branch_id, expected_head, changes)
merge(source_branch_id, target_branch_id, expected_target_head)
pull_branch(source_branch_id, local_branch_id)
push_branch(branch_id)
pull_commit_history(branch_id)
create_stack_history_from_layer(layer_history_id, layer_id)
pull_layer_history(layer_history_id, through_layer_id)
pull_stack_history(stack_history_id, through_stack_id)
add_stack(stack_history_id, branch_id, commit_id)
push_stack(stack_id)
add_layer(layer_history_id, source)
```

Direct publication is exactly:

```text
BranchStore.commit
    -> BranchStore.push_branch to LayerStore
    -> LayerStore.add_layer(BranchSource)
```

Stacked publication is exactly:

```text
BranchStore.commit
    -> BranchStore.push_branch to StackStore
    -> StackStore.add_stack
    -> StackStore.push_stack to LayerStore
    -> LayerStore.add_layer(StackSource)
```

`pull_branch(source_branch_id, local_branch_id)` is non-destructive. A fresh
local ID creates a local Branch at the pinned source base/head. Same-ID Pull may
create, return UpToDate, or fast-forward by ancestry CAS. It never rewinds a
local-ahead Branch and never overwrites divergence. Resolve divergence by
Pulling into a fresh local Branch, then using the existing Merge operation.

## 4. Exact production structure and responsibility

Follow `docs/source-tree.md` exactly. The target storage surface is:

```text
crates/
├── layerfs-storage-core/
│   └── src/
│       ├── lib.rs          declarations and re-exports only
│       ├── ids.rs          storage IDs; ObjectId delegation
│       ├── records.rs      seven records plus AddResult; manual bounded endpoint codec
│       ├── schema.rs       exact 3/9 and 8/24 DDL and indexes
│       ├── sql.rs          typed/Object reads, exact inserts, final-intent transactions, CAS
│       ├── merge_base.rs   indexed Commit -> Stack -> Layer base selection
│       ├── three_way.rs    Clean or first lexicographic Conflict
│       ├── merkle.rs       traversal, pruning, build, semantic equality
│       ├── admission.rs    fixed membership, bounded batches, authenticated admission
│       ├── contract.rs     fourteen-operation values, outcomes, internal phase envelopes
│       └── wire.rs         byte framing, checksums, bounded backpressure
│
├── layerfs-branch-store/
│   └── src/
│       ├── lib.rs
│       ├── branch_store.rs
│       ├── create_branch.rs
│       ├── commit.rs
│       ├── merge.rs
│       ├── branch_transfer.rs
│       ├── layered_read.rs
│       └── snapshot.rs
│
├── layerfs-stack-store/
│   └── src/
│       ├── lib.rs
│       ├── stack_store.rs
│       ├── writer.rs
│       ├── create_history.rs
│       ├── add_stack.rs
│       ├── history_pull.rs
│       ├── commit_pull.rs
│       ├── branch_transfer.rs
│       ├── push_stack.rs
│       ├── remote.rs
│       └── bin/layerfs-stack-store.rs
│
├── layerfs-layer-store/
│   └── src/
│       ├── lib.rs
│       ├── layer_store.rs
│       ├── provision.rs
│       ├── add_layer.rs
│       ├── transfer.rs
│       ├── remote.rs
│       └── bin/layerfs-layer-store.rs
│
└── layerfs-sdk/
    └── src/
        ├── lib.rs
        ├── direct.rs
        ├── stacked.rs
        ├── endpoint.rs
        └── binding.rs
```

Retain `layerfs-workspace` as the sole zero-table transient runtime and keep the
presentation chain exact:

```text
layerfs-mount -> layerfs-sdk -> layerfs-workspace -> layerfs-branch-store
              -> configured StackStore/LayerStore parent
```

`layerfs-sdk/binding.rs` owns only the Workspace/materialization caller-to-
BranchStore/Branch binding. It is not a transparent Store wrapper and does not
forward the fourteen domain operations.

Workspace owns generic transient COW/overlay changes, dirty spool ownership,
one staged mutation set, and the direct commit/discard lifecycle. BranchStore
owns exact Branch snapshot reads, persistent Branch/Commit/local-CAS state, and
layered snapshot/object reads. Mount owns only FUSE callbacks, kernel inode and
open-handle mappings, errno/attribute translation, and session/bootstrap. The
Workspace crate adds no SQLite file/table, Store authority, Push/Add,
deduplication policy, or transport.

The target has four storage crates, no transfer/sync/server crate, 42
production Rust files including SDK composition, five declaration-only
`lib.rs` files, and two bootstrap-only named binaries. The SDK is composition,
not a fifth storage owner.

Do not create `product.rs`, `common.rs`, `utils.rs`, `manager.rs`,
`repository.rs`, a generic repository abstraction, one interface per Store, or
request/validator/executor files for short cohesive operations. No `mod.rs`.
No implementation-bearing `lib.rs`. No catch-all or god file under another
name. The hard handwritten production-file limit is 1,000 LOC. Do not split a
cohesive 350–999-line file merely for size; split whenever distinct surviving
responsibilities remain. Total production LOC and duplicate-algorithm deletion
matter more than small-file targets.

Implement the clean cold architecture. Do not preserve old crates, adapters,
facades, schemas, names, or duplicated algorithms merely to reduce the diff.
The goal is the smallest coherent production system after completion, not the
smallest patch against the current workspace.

## 5. Exact database ownership

BranchStore owns exactly 3 tables / 9 columns:

```text
objects(object_id, bytes)
commits(commit_id, root_id, parent_id, merge_parent_id)
branches(branch_id, head_commit_id, base_id)
```

StackStore and LayerStore each own the identical 8 tables / 24 columns:

```text
objects
commits
branches
layer_histories
layers
stack_histories
stacks
add_results
```

Their DDL is identical; their authority is not. Do not add transfer, session,
request, progress, metrics, closure, object-location, ownership, lease, GC,
rollback, UI, or recovery tables.

Every SQLite file has one owning Store process/handle, one connection, one fair
serialized mutation/transfer queue, and one active working buffer set. A second
owner returns `StoreBusy`. Concurrent callers queue before allocating the
working set. Independent Store database files may progress independently.

All stores use WAL and `synchronous=FULL`. Network waits, CDC, encoding,
hashing, signature verification, Merkle traversal, semantic digests, and
three-way evaluation occur outside a SQLite write transaction. One read-only
recursive CTE snapshot may remain open while its bounded ancestry pages move;
no writer gate or write transaction may span that movement.

## 6. Transaction and insertion order

For every Store admission, preserve this dependency and visibility order:

```text
canonical child objects
    -> canonical parent/tree objects
    -> closure-complete root
    -> immutable Commit / Stack / Layer facts
    -> transferred AddResult and frozen Branch provenance where applicable
    -> Branch ref, observed history head, or copied Stack head
    -> authoritative exact CAS last
```

Parents never become admissible before their children. Product-visible refs
never point at incomplete facts. Fold final visibility into the last bounded
object/fact transaction. If no admission batch is needed but a ref must move,
use one small visibility transaction. Conflict or an injected illegal CAS
movement exposes no candidate result.

Do not add internal Add retries. A queued Add reads the current head once,
evaluates once, and exact-CASes once. The next queued caller sees the completed
head. An illegal injected movement returns `HeadMoved` and rolls back the final
candidate/AddResult/head unit.

## 7. No avoidable Store or database round trips

The only cross-Store exchanges allowed are those required to discover and move
missing data or to invoke the explicit next semantic operation. There must be
no per-object Store call, per-object SQL query, duplicate head read, polling,
hidden refetch, sender-delete acknowledgement, full-database copy, or Add-time
network lookup.

Use the binding algorithm:

```text
source emits one dependency-ordered typed/Object page
    -> destination performs one set membership query per exact table/page
    -> destination returns separate fixed 64-byte missing bitmaps
    -> source sends only missing stored canonical bytes/facts
       while piggybacking the next announcement
    -> destination authenticates and admits bounded batches
    -> final visibility/CAS is folded into the last admission transaction
```

Fixed bounds:

```text
Object membership page       512 ObjectIds
typed membership page        <= 512 IDs from one exact typed table
missing bitmap               exactly 512 bits / 64 bytes
object insert batch          <= 128 rows and normally <= 4 MiB
valid oversize singleton     <= 16 MiB
typed/provenance fact batch  <= 128 rows and <= 64 KiB
final metadata statements    <= 8
```

Prepare one 512-placeholder primary-key membership statement for `objects` and
one per relevant typed table. NULL-pad short pages and remap unordered results
to source positions. Never generate 1..512 statement families. Never query a
typed ID against `objects` or an ObjectId against a typed table.

Commit ancestry uses one prepared `UNION`-deduplicating recursive CTE cursor,
stepped in pages of at most 512 rows. SQLite owns visited state, deduplication,
page cache, and temp spill. Do not rerun with `LIMIT/OFFSET` and do not build an
unbounded Rust ancestry set.

Embedded local calls use typed Store endpoints. Loopback tests use the same
contracts encoded by `records.rs` and framed as opaque bytes by `wire.rs`.
There is one membership/admission/visibility
algorithm, not separate local and remote semantics. The byte carrier never
owns SQL, ObjectId lookup, deduplication, history, CAS, or three-way behavior.

Push and Add remain two explicit operations. This semantic boundary is not an
avoidable protocol round trip. However, Add must use data already admitted by
Push and perform zero hidden transfer or parent query.

## 8. CAS, CDC, and deduplication requirements

There is one object identity pipeline:

```text
new or replacement bytes
    -> frozen FastCDC 8/16/32 KiB
    -> canonical object encoding
    -> ObjectId::for_bytes(canonical bytes)
    -> objects(object_id PRIMARY KEY, bytes)
```

Transfer never runs CDC, re-encodes an object, or generates a new identity.
The sender streams already-stored canonical bytes; the destination
authenticates each missing frame once. Local new objects encode/hash once.
Scratch-spill rereads are the only separately counted exceptional
reauthentication.

Within each physical Store, deduplication is exact by ObjectId primary key.
Across separate databases there is no imaginary global CAS: each physical
database may own one necessary copy. Efficiency comes from never storing a
second copy in the same receiver and never sending a receiver-known object.

Prove these boundaries independently:

| Boundary | Required result |
|---|---|
| Repeated local writes in one BranchStore | Same canonical ObjectId has one row; identical streams from the same base/edit path approach one payload set plus small metadata. |
| BranchStore -> LayerStore | LayerStore receives exactly its missing set; a repeated Push sends zero known payload. |
| BranchStore -> StackStore | StackStore receives exactly its missing set, including objects it did not already obtain from LayerStore Pull. |
| StackStore -> LayerStore | LayerStore drops every object/fact it already has from prior Branch Push, Stack Push, or history Pull; only the missing signed suffix/provenance moves. |
| Multiple BranchStores -> one receiver | All senders converge on one row per ObjectId in that receiver. |
| Race at receiver | `INSERT ... ON CONFLICT DO NOTHING RETURNING` partitions inserted versus raced-existing IDs without a per-object follow-up query. |

For every transfer, assert the exact set equation:

```text
announced IDs
    = already-present IDs
    union receiver-missing IDs

sent IDs
    = receiver-missing IDs

receiver-missing IDs
    = newly inserted IDs
    union raced-existing IDs
```

Also assert byte equality for each set. A second identical Push must have zero
payload bytes, zero CDC invocations, zero re-encodes, and zero duplicate object
rows.

The ten-install storage analogy is a required fixture:

```text
ten Branches in one BranchStore
    start from the same Layer or Stack base
    apply the same deterministic installation byte stream through the same edit path
    commit independently
```

Expected physical result in that BranchStore:

```text
approximately one unique payload-object set
+ O(10) Branch/Commit and changed structural metadata
```

Run the same fixture through direct and stacked publication. Record for every
physical database:

```text
object row count
sum(length(bytes))
typed fact row counts
announced IDs/bytes
missing IDs/bytes
sent IDs/bytes
inserted IDs/bytes
raced-existing IDs/bytes
CDC bytes scanned
encode/hash invocation counts
```

The exact acceptance rule is set-based, not an arbitrary compression ratio:
each receiver's final ObjectId set equals the mathematical union of required
objects, with exactly one row per ObjectId. Explain unavoidable per-database
copies separately from duplicate rows. Do not claim cross-database physical
deduplication.

## 9. Push Stack provenance

For every `BranchId -> StackId` AddResult in a pushed suffix, Push Stack must
carry missing-only:

```text
the AddResult
the frozen same-ID StackStore Branch base/head
the exact accepted Commit DAG
required Commit root objects
the predecessor and result Stack manifests/roots
```

The creator signature binds the predecessor Stack, accepted Branch/AddResult,
exact Commit and DAG/root IDs, result Stack/root, ordered suffix, and complete
typed/Object provenance frontier. LayerStore verifies signature, IDs,
relationships, canonical facts, and closure. It never recomputes `three_way`
during Push Stack. The copied StackHistory head becomes visible only after all
missing provenance is admitted. After creator StackStore is unavailable,
LayerStore must still serve `pull_commit_history` for every centrally accepted
suffix Branch.

## 10. Implementation order

Follow `implementation-plan.md` dependency order. Do not build all crates in
parallel before their shared owner is stable.

```text
P0  deletion-first workspace reset and binding matrix
P1  layerfs-storage-core schemas, SQL, admission, merge-base, three-way, wire
P2  complete direct two-DB vertical slice
P2F route direct mode through real FUSE; close functional oracle and smoke
P3  complete stacked three-DB vertical slice without regressing direct FUSE
P3F route stacked mode through the same real FUSE; close functional oracle and smoke
P4  cross-base Branch merge
P5  embedded/loopback contract equivalence and bounded byte path
P6  SDK plus FUSE/evaluation closure; materialization only after FUSE gates
P7  both-topology deduplication, authoritative fs-bench, performance, and workspace closure
```

After each responsibility move or semantic change:

```text
trace callers
    -> make one root-cause implementation change
    -> run the smallest focused test owned by that responsibility
    -> preserve raw failure/pass evidence
    -> continue
```

Do not run the complete workspace suite after every edit. Do not repeatedly
rerun unchanged passing populations. Use focused tests until a phase gate is
green, then run dependent suites. Run the full workspace closure once after
source freeze and again only if a later source change invalidates it.

## 11. Local topology verification

Use explicit temporary directories and physically distinct files. A typical
fixture layout is:

```text
<run>/direct/
    branch.sqlite
    layer.sqlite
    mount/
    artifacts/

<run>/stacked/
    branch.sqlite
    stack.sqlite
    layer.sqlite
    mount/
    artifacts/
```

Never place the SQLite files inside the benchmark mount. Never benchmark over
NFS, SMB, a remote FUSE path, or copied live WAL/SHM files.

Direct-mode proof:

```text
provision LayerHistory
 -> create Branch from Layer
 -> mount/use BranchStore
 -> Commit
 -> Push Branch to LayerStore
 -> Add Layer from BranchSource
 -> Pull into a fresh second BranchStore
 -> compare root, bytes, histories, and table manifests
```

Stacked-mode proof:

```text
Pull LayerHistory into StackStore
 -> create StackHistory
 -> create Branch from Stack
 -> mount/use BranchStore
 -> Commit
 -> Push Branch to StackStore
 -> Add Stack
 -> Push Stack with complete signed Branch provenance
 -> Add Layer from StackSource
 -> Pull from LayerStore after creator StackStore is unavailable
 -> compare root, bytes, histories, and table manifests
```

Run ten concurrent callers against each shared Store. They must queue fairly,
complete without starvation, retain one active working set, preserve
child-before-parent and facts-before-visibility ordering, exact-CAS last, and
produce correct final heads with no partially visible closure. Run independent
direct and stacked Store files concurrently to prove there is no global queue.

## 12. `fs-bench` verification

FUSE is the primary product path for this task. The benchmark target must be a
real Linux kernel FUSE mount served by `layerfs-mount`, not a direct SDK
benchmark, bind mount, ordinary directory, tmpfs substitution, in-memory fake,
or benchmark-only daemon. Reuse one mount implementation for both topologies;
only the explicit SDK Store composition differs.

Before timing either topology, prove through the mounted path:

```text
dedicated /dev/fuse-backed mount at /workspace
Docker inspection shows no bind or volume mounted at /workspace
create/read/write/append/truncate
nested mkdir/readdir/find
rename/unlink/open-unlink behavior
symlink/readlink and hard-link inode identity
supported metadata
mmap and fsync behavior required by the retained FUSE contract
exact digest and RootId agreement with BranchStore logical reads
clean unmount and no owned mount/process/temp residue
```

The mount must remain BranchStore-focused. A filesystem syscall may update the
private transient Branch workspace and use bounded layered reads, but it may
not Push, Add, contact LayerStore/StackStore for mutation, materialize/capture a
backing tree, or perform a per-syscall Store round trip. Explicit Commit and
publication happen outside the timed syscall window.

Use the exact unchanged benchmark source:

```text
/Users/yifanxu/Ephemeral-AI-Lab/cloudflare-computer-bench/upstream/script/fs-bench.sh
SHA-256 0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef
```

The repository copy and verifier are:

```text
containers/layerfs-fuse/fs-bench.sh
containers/layerfs-fuse/verify_fs_bench.py
```

Verify the script checksum before every authoritative campaign. Do not patch,
wrap, replace, skip, recognize, or special-case benchmark scenario names in
production code. Product code must expose the same path used by ordinary SDK
and mount callers.

Run the exact twelve offline scenarios:

```text
create 1000 files
stat 1000 files
rm 1000 files
mkdir tree (10x10x10)
find tree
write 64 MiB
copy 64 MiB
read 64 MiB
pure read 64 MiB
pure copy 64 MiB
overwrite 64 MiB
git init + commit 100 files
```

Iteration order:

```text
focused logical/storage test
 -> real mounted-path functional oracle
 -> one causal fs-bench scenario
 -> three-scenario smoke
 -> authoritative campaign only after source freeze
```

Three-scenario smoke:

```text
REPS=1
WARMUP=1
RANDOMIZE_TARGETS=0
SCENARIOS=create 1000 files,stat 1000 files,pure read 64 MiB
```

Authoritative settings for each topology and control population:

```text
REPS=3
WARMUP=1
RANDOMIZE_TARGETS=1
MOUNT=/workspace
BASE=/var/tmp     # overlay control campaign
BASE=/tmp         # tmpfs control campaign
SCENARIOS=<the exact twelve-scenario filter>
OUTPUT_JSON=<artifact path outside mount and control target>
```

Use the retained sealed measurement class unless a concrete host constraint is
recorded:

```text
Docker context       desktop-linux
platform             linux/arm64 on the current native Apple-Silicon host
init                 enabled
CPU                  --cpus 1
memory               --memory 512m
PIDs                 --pids-limit 512
FUSE                 --device /dev/fuse:rwm --cap-add SYS_ADMIN
privileged           false
network              none during measured runs
/workspace           LayerFS kernel FUSE only; no bind or volume
/var/tmp             overlay/native control
/tmp                 tmpfs,size=1g,mode=1777 control
```

The build may use the network before the image is sealed. Each measured
container is offline. Capture `mountpoint /workspace`, `findmnt -T /workspace`,
container inspection, native architecture, image ID, executable hash, source
commit/tree, and the benchmark script hash before accepting timing rows.

Run four authoritative populations against the same frozen source:

```text
direct topology  + /var/tmp control
direct topology  + /tmp control
stacked topology + /var/tmp control
stacked topology + /tmp control
```

Use `containers/layerfs-fuse/verify_fs_bench.py` to produce immutable
verification receipts. Preserve raw JSON, stdout, stderr, checksum, environment,
image/executable identity, topology/database paths, mount inspection, resource
counters, and cleanup evidence.

The timed `fs-bench` window measures the mounted BranchStore filesystem path.
It must not hide Commit, Push Branch, Add Stack, Push Stack, or Add Layer inside
syscall latency. Those explicit publication operations are measured separately
with Store timers and transfer counters. Conversely, do not report only the
fast transient mount path: run the publication/dedup campaigns and prove the
resulting LayerStore state after each topology.

During timed mount activity:

- no Push or Add occurs;
- no per-syscall LayerStore/StackStore RPC occurs;
- accepted base reads may use the configured parent through bounded layered
  reads;
- no whole-file, whole-tree, or whole-database materialization occurs; and
- all benchmark-specific production branches/caches are forbidden.

Treat a wall-clock difference below 5% or below three median absolute
deviations as noise unless it reveals an asymptotic/query/byte regression. Do
not spend critical-path time on differences like 9.4045 ms versus 9.2618 ms.
Full scans, duplicate bytes, extra Store turns, whole-copy fallbacks, or
unbounded memory are never noise.

## 13. Performance and resource gates

Prove, do not merely claim:

- fixed prepared membership statements use primary-key/index SEARCH plans;
- normal transfer performs no full `objects`, Commit, Stack, Layer, or history
  scan;
- known roots prune complete subtrees;
- repeated mapped/equal-root Add reads zero descendants; a newly accepted equal-root Branch still creates one same-root Stack provenance node;
- COW replacement scans only replacement bytes and keeps unchanged extents;
- the sender sends exactly receiver-missing canonical bytes;
- the receiver performs bounded set queries and bounded insert transactions;
- transfer CDC/re-encode counters remain zero;
- Add performs zero Store/network lookup after Push;
- one transfer uses the binding coalesced `P + 1` turn bound;
- direct publication uses no turns beyond Push data movement and the explicit
  Add command;
- stacked publication uses no turns beyond the two Push data movements and
  two explicit Add commands;
- no SQLite write transaction spans network or CPU-heavy preflight;
- one active Store operation stays below the binding application-memory bound;
- queue wait is reported separately from operation service time; and
- no Store file contains duplicate ObjectId rows.

Do not optimize by weakening WAL/FULL, bypassing authentication, changing
canonical IDs, skipping closure verification, moving SQL into the byte carrier,
or adding a local-only shortcut with different semantics.

## 14. Required tests and evidence

At minimum, complete every focused and terminal gate in
`implementation-plan.md` and `db-transaction-transfer-model.md`, including:

```text
exact 3/9 and 8/24 schema manifests and wrong-shape rejection
all fourteen operations in legal direct/stacked routes
zero accepted payload rows in BranchStore after create/pull/read
direct and stacked end-to-end publication
two-ID Pull outcomes and explicit divergent Merge workflow
cross-base Commit -> Stack -> Layer merge-base outcomes
first lexicographic Conflict and zero-write conflict proof
signed Push Stack Branch/Commit/root provenance completeness
fixed typed/Object membership and query-plan evidence
bounded object/fact transactions and visibility-last order
ten-caller fair serialized Store tests
direct/stacked dedup set and byte equations
ten-install same-base/edit-path storage fixture
large-file streaming and maximum-object singleton
embedded and loopback contract equivalence
both topology fs-bench smoke and authoritative populations
real FUSE functional oracle for direct and stacked Store composition
```

Tests and evaluators may observe counters and canonical tables. They may not add
production metrics tables, duplicate algorithms, or expose a benchmark-only
API. Preserve raw evidence under a clearly named run directory; do not edit a
failed row into a pass.

## 15. Continuation and replanning mandate

Do not stop at `NO-GO`, `REVISE`, a compile error, failed test, benchmark
regression, structural violation, schema mismatch, incorrect query plan, or
deduplication failure. These are replanning inputs, not completion states.

On every failure:

1. preserve the exact command, raw output, source revision, environment, and
   relevant database/byte/query counters;
2. classify the root cause as production code, source ownership, schema/SQL,
   fixture, evaluator, environment, or concrete platform limitation;
3. trace the shared owner and all callers;
4. update the implementation plan in dependency order;
5. make the clean root-cause correction in the owning module;
6. run the smallest focused proof that would have caught the defect;
7. rerun only the invalidated dependent population; and
8. continue toward terminal pass.

Never remove a gate, weaken a threshold after seeing results, retry unchanged
source hoping for noise, patch only one caller of a shared defect, add a
compatibility layer to avoid an ownership move, or label a fallback as an
optimized path.

If a genuine external platform blocker exists, exhaust all safe in-repository
alternatives, preserve concrete evidence, state the exact external action
required, and continue every independent task. An implementation-caused
failure is never an external blocker.

## 16. Final structural and workspace closure

Before terminal disposition, run:

```text
cargo metadata --no-deps

rg -n 'name = "layerfs-storage"|layerfs-(working-store|durable-store|sync|service)' \
  Cargo.toml crates tools

rg --files crates/layerfs-{storage-core,branch-store,stack-store,layer-store,sdk,workspace,mount} \
  | rg '(^|/)(mod|main|product|common|utils|manager|repository)\.rs$'

find crates/layerfs-{storage-core,branch-store,stack-store,layer-store,sdk,workspace,mount} \
  -name '*.rs' -print0 | xargs -0 wc -l

cargo fmt --all -- --check
cargo test -p layerfs-core
cargo test -p layerfs-storage-core
cargo test -p layerfs-branch-store
cargo test -p layerfs-stack-store
cargo test -p layerfs-layer-store
cargo test -p layerfs-sdk
cargo test -p layerfs-workspace
cargo test -p layerfs-materialization
cargo test -p layerfs-mount
cargo test -p layerfs-eval
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Expected structure:

```text
exactly four storage crates
no obsolete storage/sync/service package or import
one zero-table `layerfs-workspace`; no duplicate generic COW/spool owner
no forbidden filename in target storage crates, SDK, Workspace, or mount
no implementation-bearing lib.rs
no handwritten production file above 1,000 lines
no cohesive 350–999-line file split only to satisfy a size target
storage/package/workspace LOC estimates reviewed for unnecessary surviving lines
aggregate LOC is not an automatic PASS/FAIL threshold
```

Package figures such as storage-core 2,400, SDK 350, storage plus SDK 6,100,
and full-workspace totals are review estimates, not terminal gates or permission
to compress correctness into dense files. Do not move tests mechanically or
split/merge responsibilities to hit a number. First remove duplicate
algorithms/types/round trips, low-level public protocol, compatibility code,
stubs, and wrong ownership. Judge minimality by whether every surviving line
has one necessary responsibility. Architecture, canonical-model reuse,
transaction/transfer correctness, measured speed/space, SRP, public API
minimality, and evidence take priority. A current storage-plus-SDK count around
7.9k is a review signal, not by itself a terminal blocker.

## 17. Terminal disposition

Return terminal `PASS_LAYERSTACK_LOCAL_STORAGE` only after the same frozen
source has raw evidence for all of these classes:

```text
PASS_EXACT_SOURCE_TREE
PASS_SCHEMA_3_9_AND_8_24
PASS_DIRECT_TWO_DB
PASS_STACKED_THREE_DB
PASS_FUSE_DIRECT_TWO_DB
PASS_FUSE_STACKED_THREE_DB
PASS_ALL_FOURTEEN_OPERATIONS
PASS_TRANSACTION_ORDER_AND_VISIBILITY
PASS_CAS_CDC_IDENTITY
PASS_DIRECT_DEDUP
PASS_STACKED_DEDUP
PASS_TEN_INSTALL_STORAGE
PASS_NO_EXTRA_STORE_TURNS
PASS_REASONABLE_CONCURRENCY
PASS_FS_BENCH_DIRECT_OVERLAY
PASS_FS_BENCH_DIRECT_TMPFS
PASS_FS_BENCH_STACKED_OVERLAY
PASS_FS_BENCH_STACKED_TMPFS
PASS_WORKSPACE_FORMAT_TEST_CLIPPY
PASS_LAYERSTACK_LOCAL_STORAGE
```

The final report must include:

1. frozen commit and exact production tree;
2. phase completion table with owning files and focused tests;
3. direct and stacked database paths, schema manifests, row/byte counts, and
   final history heads;
4. transaction/insert-order and ten-caller concurrency evidence;
5. direct and stacked announced/missing/sent/inserted/raced ID and byte
   equations;
6. ten-install per-database storage accounting;
7. query plans, SQL/transaction counts, Store turns, buffer peaks, and lock
   timings;
8. all four authoritative `fs-bench` receipts plus raw artifacts and
   checksums;
9. direct and stacked real-FUSE mount inspection, functional oracle, syscall
   counters, unmount, and cleanup evidence;
10. focused crate tests and full workspace format/test/Clippy results;
11. any retained deferred items, clearly separated from active terminal
    requirements.

Until every required class passes, replan and continue. Never fabricate a
terminal pass.

---
