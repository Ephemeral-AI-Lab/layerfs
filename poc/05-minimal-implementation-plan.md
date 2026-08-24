# Minimal implementation plan

Status: historical implementation plan for the Apple/APFS PoC. Completion and
current evidence are recorded in `poc/17`; this page does not turn G5
measurements or G6 research into product evidence.

Related decisions and contracts:

- [scope and decisions](00-scope-and-decisions.md)
- [architecture and file structure](01-architecture-and-file-structure.md)
- [data structures and algorithms](02-data-structures-and-algorithms.md)
- [operation workflows](03-operation-workflows.md)
- [Apple/APFS materialization and recovery](04-apple-apfs-materialization-and-recovery.md)
- [correctness and fast verification](06-correctness-and-fast-verification.md)
- [native workspace and Bash verification](08-native-workspace-and-shell-verification.md)
- [portability and Apple completeness](09-portability-and-apple-completeness.md)
- [final handoff freeze](10-handoff-freeze.md)

## 1. One milestone, four work packages

Collapse G6 and project phases 5–8 into one vertical result:

```text
fresh PoC store
  -> immutable payload CAS
  -> byte-measured persistent file tree
  -> byte-bounded persistent namespace trees
  -> SQLite root/history publication
  -> APFS native workspace
  -> managed edit or real Bash/editor edit
  -> capture a new immutable root
  -> reopen/materialize exact bytes
  -> fork and rollback retained roots
```

There are four dependency-ordered work packages, not four independent evidence
programs:

| Package | Product result | Exit condition |
|---|---|---|
| A. Core | canonical file/namespace codecs, persistent byte-measured file tree, persistent byte-bounded directory trees, reads and mutations | file and namespace differential models, codec goldens, malformed input, retained roots pass |
| B. Engine | one reusable SQLite store, expected-head publication, history/fork/rollback | atomic publication, restart, reader/writer and parity tests pass |
| C. Universal VFS + Apple driver | OS-neutral ProjectionDriver/conformance, Apple cold/warm materialize, optional clone/patch, hard links/metadata, Bash/editor session, capture, recovery | in-memory/fault and Apple drivers pass one conformance suite; exact directory round trip and native faults pass |
| D. Facade | minimal SDK, one example, one compact smoke evaluator | fresh release build completes the end-to-end sequence once |

Do not create a separate G6 benchmark implementation. Do not create parallel
CD32–64 and B+ implementations. The selected structure in
[`02-data-structures-and-algorithms.md`](02-data-structures-and-algorithms.md)
must be implemented directly in the reusable product path.

## 2. Source reality and extraction gate

The following are checked-source facts, not architecture claims:

| Current source | What is real now | Consequence |
|---|---|---|
| `layerfs-core/src/content/mod.rs` | `LogicalFile` owns a complete `Vec<ChunkReference>` | full-file metadata is resident; it is not the target tree |
| `content/persistence.rs` and `canonical_v2.rs` | overlapping file mapping codecs and different reference shapes exist | choose one fresh-profile codec and de-duplicate semantics before product integration |
| `cow/tree.rs` | directories use `Arc` plus cloned `BTreeMap`; IDs are explicitly provisional | retain as legacy/model input; do not use as the new product mutation algorithm |
| `layerfs-engine/src/lib.rs` | reusable `layerfs_*` SQLite schema, object BLOBs, parent check, one capture transaction | extend this implementation instead of copying a benchmark Store |
| `phase4_create_edit_benchmark.rs` | G5 `wp4m_*` Store, trust logic, instrumentation and fixed fixtures live in a 21k-line benchmark binary | accepted behavior is a reference; benchmark code is not a product module |
| `phase4_g3_materialization.rs` | projector, exact/latest mailbox, native publication and fixtures live in a roughly 10k-line benchmark source | extract semantics once; reject fixture hashes, runners and literal counters |
| `layerfs-os` | host probing only | APFS operations still need implementation |
| `layerfs-vfs`, `layerfs-sdk` | component constants only | materialization, capture and public workflow do not exist yet |
| G5 terminal artifacts | warm, narrow benchmark mechanisms passed | they do not prove product extraction, arbitrary files, cold I/O or hostile paths |
| G6 CD32–64 documents | detailed research and analytical models | not implemented and not measurement authority |

### Extraction/de-duplication gate

Before Package B or C may be called integrated:

1. Name one canonical fresh-profile file codec and one decoder/validator.
2. Name one authoritative SQLite schema. The PoC uses `layerfs_*`; it does not
   introduce `wp4m_*` tables.
3. Add parity tests for every accepted old behavior being promoted:
   identity validation, expected-head rejection, one transaction/COMMIT,
   TrustedLocalDev boundaries, per-file native old-or-new publication and
   incomplete-tree live-authority handling.
4. Move the semantic implementation into library modules. Make any retained
   benchmark call those modules.
5. Reject benchmark-only inputs in product code: fixed roots, fixture paths,
   size labels, digest literals, environment semantic switches and synthetic
   counters.
6. Only then remove the active benchmark binaries. Historical reports and raw
   evidence stay untouched.

Passing a benchmark that still executes copied `wp4m_*` logic does not satisfy
this gate.

## 3. Minimal target files and ownership

The full target tree is in
[`01-architecture-and-file-structure.md`](01-architecture-and-file-structure.md).
The minimum implementation slice is below. Add no file until its listed owner
has real code.

| File | Owns | Must not own |
|---|---|---|
| `layerfs-core/src/content/extent.rs` | canonical extent/node/root fields and strict structural validator | SQLite, paths, APFS, timers |
| `layerfs-core/src/content/extent_codec.rs` | fresh-profile canonical bytes and exact decode checks | traversal, transactions, native state |
| `layerfs-core/src/content/rope.rs` | build, locate, range, stream, split, concat, insert, delete, replace, rebalance, counters | transactions, workspace state |
| `layerfs-core/src/namespace.rs` | directory/file/symlink values plus persistent lookup/create/remove/rename/path-copy | host paths, SQLite, process execution |
| `layerfs-core/src/namespace_codec.rs` | canonical variable-name leaf/branch and directory/symlink state bytes | traversal, host normalization |
| `layerfs-core/src/inode.rs` | stable canonical InodeId/table, checked links and shared hard-link mutation | host device/inode numbers |
| `layerfs-core/src/metadata.rs` | portable fields and typed opaque extension map | xattr/ACL syscalls or platform interpretation |
| `layerfs-core/src/content/mod.rs` | stable content exports and frozen FastCDC composition | a second file representation on new writes |
| `layerfs-core/src/error.rs` | exact core errors and checked overflow | backend error strings |
| `layerfs-core/tests/extent_codec.rs` | independent bytes/ID goldens and malformed records | performance thresholds |
| `layerfs-core/tests/extent_model.rs` | deterministic `Vec<u8>` differential model and retained-root oracle | wall-time assertions |
| `layerfs-core/tests/namespace_model.rs` | deterministic ordered-map/path oracle, deep split/merge and rename | wall-time assertions |
| `layerfs-engine/src/lib.rs` | small exports | benchmark fixtures |
| `layerfs-engine/src/store.rs` | schema/object BLOB logic, one-fetch/one-auth borrowed reader, ordered batch-64 payload reads, configuration and counters | canonical algorithms or packed carrier |
| `layerfs-engine/src/integrity.rs` | Verified/TrustedLocalDev scopes and receipts | benchmark process shape |
| `layerfs-engine/src/refs.rs` | retained roots, generation-CAS refs, fork, rollback and reachability | online GC |
| `layerfs-engine/src/publication.rs` | expected head, object/delta/root publication, one COMMIT, reconciliation | projection or SDK policy |
| `layerfs-engine/src/generation.rs` | neutral StoreGenerationDriver selector/sync port | projection operations or platform cfg |
| `layerfs-engine/src/compaction.rs` | exclusive retained-union mark, sibling-Store copy/verify/swap and recovery | concurrent or in-place deletion |
| `layerfs-engine/tests/store_and_publication.rs` | store, atomicity, concurrency, history and parity | large campaign harnesses |
| `layerfs-engine/tests/faults_and_reopen.rs` | restart, ambiguous outcome and trust transitions | benchmark schedules |
| `layerfs-vfs/src/driver.rs` | universal ProjectionDriver port, capabilities and typed native results | concrete syscalls or platform cfg |
| `layerfs-vfs/src/materialize.rs` | OS-neutral exact/warm/clone-patch/full-stream route selection | canonical identity rules or concrete APFS calls |
| `layerfs-vfs/src/workspace.rs` | workspace lifecycle, provenance and managed change evidence | SQLite SQL |
| `layerfs-vfs/src/capture.rs` | managed local capture and external full-workspace capture | a second CAS/store |
| `layerfs-vfs/src/external.rs` | ordinary-workspace exposure, external state transition and child/quiescence admission | shell parsing or canonical semantics |
| `layerfs-os/src/apple/mod.rs` | AppleDriver implementation and native factory | VFS policy or canonical identity |
| `layerfs-os/src/apple/{workspace,apfs,metadata,store,ffi}.rs` | APFS handles/clone/replace/sync, links, metadata, Store selector installation and sole unsafe boundary | roots, publication or benchmark logic |
| `layerfs-vfs/tests/poc_workflow.rs` | directory oracle, actual Bash child, faults, reopen and end-to-end sequence | benchmark populations |
| `layerfs-sdk/src/lib.rs` | `open`, managed/external materialize, capture, discard and stable errors | rusqlite or libc types |
| `layerfs-sdk/examples/apple_poc.rs` | runnable user story | test-only hooks |
| `tools/layerfs-eval/src/apple_poc.rs` | one small diagnostic smoke sequence | alternate product logic |

Three small files for the core tree are deliberate: structural types,
canonical bytes and algorithms fail for different reasons. Do not split each
operation into a new module. `store.rs` should start as the current reusable
engine moved with minimal edits; do not rewrite working SQL for aesthetics.

### Dependencies

Use only dependencies already present in the workspace. The single driver port
is explicitly required; do not add a plugin registry or backend framework:

```text
blake3      canonical identity
rusqlite    durable engine
libc        narrowly isolated macOS syscall/FFI wrappers
std         files, threads, synchronization, test fixtures
```

Move the macOS-only `libc` dependency from `layerfs-engine` to `layerfs-os`
after the benchmark binary no longer needs it. The current OS crate uses
`#![forbid(unsafe_code)]`, which cannot be overridden in a child module. Change
that crate policy to `#![deny(unsafe_code)]`, retain safe Rust everywhere else,
and place all required `openat`/`fstatat`/`fclonefileat`/`pwrite`/`fsync`/
`renameatx_np` unsafe calls in one tiny reviewed `apple::ffi` submodule with
safe wrappers, checked conversions, and exact errno mapping. Do not add
`async`, a runtime, a pool,
`tempfile`, `proptest`, `criterion`, a generic backend registry, or a platform
plugin system. The single VFS `ProjectionDriver` port is required.

## 4. Package A — canonical file tree

### 4.1 Freeze only the bytes that must be durable

One short design checkpoint freezes:

- fresh profile/version ID;
- extent slice fields and integer widths;
- leaf/internal/root role tags;
- minimum/maximum occupancy and root exceptions;
- child measures (`subtree_bytes`, `subtree_extents`, child ID);
- maximum depth and checked length/count rules;
- whether `FileStateRoot` is operational/history-shaped;
- separate optional full-byte `ContentDigest` semantics.

Do not freeze APFS facts, SQLite row IDs, cache sizes, native inode numbers,
workspace paths or benchmark counters into canonical bytes.

### 4.2 Reuse rather than rebuild

Reuse unchanged:

- the 9-byte canonical object header plus the 4-byte `Bytes` value length
  (`Object::Bytes` envelope total 13 bytes);
- `ObjectId` domain-separated BLAKE3 identity;
- frozen FastCDC 8/16/32 KiB scanner;
- immutable no-replace CAS rules;
- canonical names/paths and existing delta ordering;
- checked arithmetic and typed decode failures.

New writes use the new file profile only. Keep K64/F64 decoding read-only while
old fixtures/tests require it; do not dual-write old and new mappings. No
existing-store migration is required for the fresh PoC.

### 4.3 Minimal store port

Use the narrow `ObjectRead`/`ObjectWrite` capabilities defined in
[`01-architecture-and-file-structure.md`](01-architecture-and-file-structure.md).
They have exactly two consumers: the deterministic memory model and the SQLite
engine. Do not add a generic backend factory or strategy registry. If closures
make the implementation smaller, use closures instead of traits.

### 4.4 Exit

Package A exits when all deterministic byte operations agree with `Vec<u8>`,
every reachable node validates, retained roots still reconstruct, and ordinary
managed edits satisfy the structural bounds in section 8. Wall time is not an
exit criterion.

## 5. Package B — durable store and history

Start from `layerfs-engine/src/lib.rs`, not from the benchmark Store.

### Required product behavior

```text
prepare bounded operation/evidence
  -> BEGIN IMMEDIATE
  -> compare expected visible head
  -> stream, authenticate and insert immutable object rows
  -> insert canonical delta/root/history metadata
  -> update visible head
  -> exactly one COMMIT dispatch
  -> on uncertain return, open fresh observation and reconcile
```

Keep `DELETE`, `synchronous=FULL`, `temp_store=FILE`, `mmap_size=0`, one writer
and exact errors. Replace the current reusable engine's 100 ms `busy_timeout`
with zero for PoC v1 and return `Busy`/`Locked` exactly. Add no SQLite,
internal, or application retry. This is a fixed runtime policy, not a canonical
format/profile identity field.

Promote only the narrow G5 integrity policy:

- `Verified` remains default;
- `TrustedLocalDev` is explicit and Store-lifetime;
- fetched/new/incumbent identity checks remain unconditional;
- expected head, receipt decode, transaction, COMMIT and reconciliation are
  common code;
- a Verified reopen after trusted history performs the required scrub;
- trusted assumptions never become Verified authority;
- rollback freshness remains explicitly `NotProtected` without external
  authority.

History is immutable root metadata. Fork creates another label/reference to a
retained root. Rollback is an expected-head change to a retained root in one
transaction. Neither copies file bytes.

### Compaction boundary

Package B includes one explicit offline path: acquire exclusive maintenance
authority, require zero readers/writers/workspaces/recovery pins, mark the
authenticated union of every retained root, stream it into a same-directory
sibling SQLite Store, verify exact closure/refs, durably swap Store generation,
freshly reopen, then remove the old backup. Concurrent, in-place and background
reclamation remain deferred.

## 6. Package C — universal VFS plus Apple materialize/capture driver

Implement correctness routes before accelerators:

1. Complete verified stream into a private temp.
2. Sync temp, atomic rename and directory sync.
3. Exact no-op under uninterrupted process-lifetime live authority.
4. APFS whole-file clone into a private temp plus same-offset patches.
5. Managed calls mutate the private native workspace and append ordered
   descriptors plus exact replacement bytes to an owned bounded spool;
   capture replays them in call order once.
6. Arbitrary external-editor capture using an honest full-workspace scan.
7. An actual `/bin/bash` child uses the materialized directory as `cwd`; it is
   waited/reaped before capture, and a live/background writer makes capture
   return `WorkspaceBusy`.

Route contracts are in
[`04-apple-apfs-materialization-and-recovery.md`](04-apple-apfs-materialization-and-recovery.md).

The APFS clone route changes physical work only. It never changes canonical
bytes, roots, durability, validation or output. Failure discards its private
temp and starts full fallback from a fresh temp. If visibility may have changed,
reconcile before any fallback or retry.

### Capture classes

| Capture | Available evidence | Required work |
|---|---|---|
| managed `write_at/insert/delete/truncate` | ordered current-state coordinates, exact bytes in owned spool, expected base | replay in call order; FastCDC only replacement bytes; structural slice/tree work; publication |
| external ordinary-directory edit | cooperative quiescence; no authoritative complete path/range journal | complete namespace walk, unique-inode digest pass, changed-file CDC reread, prior digest, metadata and disk-backed hard-link grouping |

Do not claim APFS, FSEvents, timestamps or inode metadata reveal a complete
changed-path set or exact byte ranges. Full-workspace external capture is a
compatibility route, not a failed optimized route.

PoC v1 runs projection synchronously. Do not promote the G5 mailbox, worker, or
coalescing machinery merely to preserve a benchmark process shape. If a real
asynchronous caller later appears, its separate design must conserve Exact
requests and may bound scheduling to one in-flight plus one replaceable pending
Latest request.

## 7. Package D — minimal facade and runnable result

The first public facade is:

```text
LayerFs::open
LayerFs::materialize_managed
LayerFs::materialize_external
ManagedWorkspace::capture / into_external / discard
ExternalWorkspace::path / capture_quiescent / discard
```

`LayerFs::open` returns `OpenedLayerFs { fs, head }`. It opens an existing
store or initializes exactly one canonical empty root in a new store and
returns that exact head; no separate configuration builder, public `init`, or
backend query is needed.

Managed edit helpers may remain explicitly PoC-level on
the workspace types. Public types contain no SQLite rows, APFS handles,
canonical codec internals, benchmark receipts or G5 counters.

One example must execute without test-only configuration:

```text
open fresh store -> receive canonical empty head
  -> materialize returned empty head
  -> populate the workspace and capture root A
  -> materialize root A
  -> materialize/convert to ExternalWorkspace and run a real Bash script
  -> direct redirect/dd/mkdir/mv/rm/chmod/symlink operations
  -> wait for quiescence and full-scan capture the shell result
  -> managed overwrite + insert + delete + rename
  -> capture root B
  -> reopen process/store
  -> materialize B to a second directory
  -> exact tree comparison
  -> fork A twice and diverge
  -> rollback one label to A
  -> exact reads from A, B and both forks
```

This example, not a benchmark harness, is the PoC deliverable.

## 8. Required complexity and resource budgets

Notation:

```text
F  logical file bytes
B  supplied changed/replacement bytes
K  replacement chunks/extents created by FastCDC within `B`; no suffix rejoin
E  extents in the file
H  file-tree height
X  extents intersecting a read
R  returned bytes
D_i entries in directory component `i`
V  retained revisions
U  unique retained objects
```

### Algorithmic requirements

| Operation | PoC requirement | Honest exception |
|---|---:|---|
| locate byte | `O(H * bounded fanout)`; treated as `O(log E)` under fixed node bounds | none |
| range read | `O(log E + X + R)` | full read remains `Theta(F)` |
| managed overwrite/insert/delete | `O(B + K + log E) = O(B + log E)` | unchanged suffix is reused as extent subtrees; it is never a CDC rejoin input |
| append/truncate | `O(B + log E)` / `O(log E + boundary)` | payload reclamation deferred |
| mapping objects created | `O(H + replacement_tree_nodes)` plus bounded split/merge allowance | pure `O(H)` only when replacement fits a frozen constant node count |
| full import/external capture | import `Theta(F + E)`; external uses explicit digest/changed-CDC/prior/metadata/grouping passes and remains `Theta(workspace bytes)` | required because arbitrary editors provide no complete path/range authority |
| cold/full materialize | `Theta(F)` | unavoidable contiguous native output |
| warm exact materialize | uninterrupted live-managed generation/mutation authority plus required path checks; zero content rewrites | reopened/unmanaged state has no authority and must verify or rebuild, potentially linear |
| same-size APFS projection | application work `O(B + ranges)` after clone | APFS internal clone cost is observed, not assumed |
| snapshot/fork | zero object-byte copies; `O(log refs)` indexed DB operation | first divergent write pays normal path-copy |
| rollback | zero object-byte copies; `O(log refs)` indexed update plus expected-head validation | freshness remains limited as declared |
| exact historical range | root lookup plus `O(log E + X + R)` | SQLite index lookup may add `O(log U)` |
| reachability | `Theta(V + U + strong edges)` | not edit-path work |
| offline compaction | `Theta(indexed objects + strong edges + surviving physical bytes)` | exclusive maintenance; one sibling Store plus disk-backed mark set |
| path resolution | `sum [O(log D_i) + O(log I)]` | directory and global inode-table lookup per component |
| file-content mutation after lookup | `O(log I)` | stable directory name maps remain unchanged |
| namespace mutation | direct-parent `O(log D)` plus bounded `O(log I)` paths | full listing remains `Theta(D)` |

### Memory and CPU requirements

| Resource | Hard/diagnostic rule |
|---|---|
| file-sized buffers | forbidden |
| individual owned buffer | `<=1 MiB` |
| operation-owned streaming memory `Q` | `<=8 MiB`, terminal exactly zero |
| decoded tree state | `O(H * node_size)`; no `Vec` of all extents |
| pending projection state | no background queue; one synchronous caller-owned request |
| SQLite connections | exactly 1 writer + at most 2 query-only readers; no pool; `busy_timeout=0` |
| Apple page-cache profile | `cache_size=1280` pages/connection with observed 4 KiB page size, reported as configured budget not Q/RSS |
| file descriptors/connections/temp files | bounded and terminal baseline/zero residue |
| CPU for supplied bytes | hashing/chunking `Theta(B)`; full routes `Theta(F)` |
| process RSS | observe; target `<=32 MiB` for the one-process small PoC, but fail structurally on growth proportional to `F` before arguing about an absolute number |

The 32 MiB RSS value is a prospective diagnostic ceiling, not inherited G5
evidence. `Q`, RSS, SQLite page cache and kernel/APFS cache are separate.

### Storage requirements

| Storage | Requirement |
|---|---|
| payload | one immutable canonical object per unique chunk |
| live file mapping | target `<1%` only for files `>=1 MiB` in the frozen deterministic FastCDC population; report tiny-file absolute overhead |
| one local file revision | unique payload `O(B)` plus file mapping `O(log E)` and one inode-table path `O(log I)` |
| retained history | `O(unique payload + V*changed paths)`; never one full mapping/file copy per revision |
| managed replacement spool | process/workspace-lifetime private bytes `O(sum replacement B)`; streamed with `<=1 MiB` RAM window and deleted on capture/discard/crash cleanup |
| full-fallback native temp | at most one target-sized private temp |
| clone/patch temp | apparent size may be `F`; allocated blocks must be observed separately when available |
| rollback journal | bounded by one publication transaction; measure high-water |
| old/unreachable objects | retained during normal operation; offline compaction omits only authenticated-unreachable objects from the verified replacement Store |
| offline compaction available-space preflight | new sibling generation allocated upper bound + disk-backed mark database + candidate SQLite rollback-journal/temp high-water bound + `CURRENT.tmp` and safety margin |
| offline compaction total peak report | retained old generation + new generation + mark database + candidate journal/temp + selector temporary bytes |

## 9. Implementation order and deletion order

```text
1  freeze the handoff decisions in 10-handoff-freeze.md
2  invert Cargo dependencies; compile ProjectionDriver and StoreGenerationDriver with fault drivers
3  implement mode-free file, namespace, inode and metadata codecs/validators
4  implement file and namespace/inode differential algorithms
5  implement one-fetch/one-auth batch-64 reader and sole Publication with StoreId/inode allocator
6  promote refs, trust, one-COMMIT and fresh reconciliation semantics
7  implement Apple full-stream materialize and cooperative full-scan capture
8  expose ManagedWorkspace/ExternalWorkspace and run Bash/mmap flow
9  add hard-link topology and frozen Apple metadata round trips
10 implement checksummed-generation offline compaction and crash matrix
11 add APFS clone/patch behind the already-correct driver route
12 add SDK example and compact evaluator
13 run one release closure
14 remove active Phase-4 binaries from Cargo after parity
15 remove unreachable duplicate writers only after repository search proves no caller
```

Deletion rules:

- never delete preserved `implementation-detail/phase-4` evidence;
- keep old K64/F64 decoding while accepted old stores/tests need it;
- delete old *write* paths after every new write routes through the fresh profile;
- remove `wp4m_*` product candidates rather than maintaining schema parity;
- do not copy benchmark code into a second library before deleting the first;
- no dual authoritative roots, schemas or projectors.

## 10. Timebox and stop rules

| Work | Focused budget |
|---|---:|
| Package A | 4–6 days |
| Package B | 3–4 days |
| Package C | 5–7 days |
| Package D and closure | 2–3 days |
| Total | 14–20 focused implementation days |

These are focus budgets, not promises. Stop and repair the design when:

- same final operation produces different streamed bytes or a malformed tree;
- a managed bounded edit reads/rewrites the unaffected suffix or allocates by
  `F`/`E`;
- a new root becomes visible with missing objects/delta;
- COMMIT outcome cannot be freshly classified as requested/prior/different/
  ambiguous;
- root/history reads depend on a native projection;
- benchmark code or fixed fixtures are required by a product API;
- two authoritative codecs, schemas or projection workers remain;
- correctness requires APFS clone success;
- RSS or owned memory grows linearly with file size for local operations.

Do **not** stop for an honestly labeled external-workspace full scan, full
native materialization, Verified scrub, all-root reachability, or offline
copy-compaction; those operations have linear lower bounds.

Do not respond to a stop by adding a framework, cache, worker pool, retry loop,
second representation or larger benchmark. Reduce the failing boundary and fix
the shared product path.

## 11. Explicitly deferred

The Apple PoC excludes:

- existing-store v2-to-new-profile migration;
- public FUSE, FSKit, File Provider or kernel mount;
- guaranteed edit-sized capture for arbitrary external editors;
- background, concurrent or in-place GC/compaction;
- compression, packs/carriers, payload deltas and remote hydration;
- PostgreSQL, remote/distributed storage and multi-host locks;
- branch merge/rebase, conflict UI, search, policy and portable bundles;
- multi-process projection queues and cross-process persistent seeds;
- Linux/Windows performance qualification;
- 500 MiB campaigns and production SLOs.

Add a deferred item only after the vertical PoC passes and a real caller or
measured owner requires it.
