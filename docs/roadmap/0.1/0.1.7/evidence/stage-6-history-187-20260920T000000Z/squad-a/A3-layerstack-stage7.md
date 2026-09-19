# A3 — The LayerStack layer: what would Stage 7 supply that core cannot?

> **Status: diagnostic.** Source-read study plus read-only arithmetic over recorded
> artifacts. **No product source was modified.** No benchmark lane was run by this
> squad; **no timing number appears in this report and none was taken** (the machine
> is shared and any wall time here would be invalid by the campaign's own rule).
> Every byte and count below is load-independent.
>
> **Evidence classes used, and labelled on every claim:**
> - **[S]** source read at `66bce8378` (file:line).
> - **[R]** arithmetic over a *recorded* receipt already on disk
>   (`benchmark-results/repository-history/stride-10/deepseek-stride10/performance-result.json`).
> - **[P]** read-only SQLite probe of two Stores produced by **another agent's
>   in-flight runs** (`/tmp/base187`, `/tmp/s0_on`, `/tmp/s0_off`). Not my runs,
>   no receipt published yet; labelled as such wherever used.
> - **[H]** hypothesis — explicitly not proven.

---

## 1. Does the Stage 7 specification exist?

**No. There is no Stage 7 specification document in this repository.** [S]

The complete normative text for Stage 7 in-tree is **one table row**:

```
docs/roadmap/0.1/0.1.7/component-decoupling/implementation-issues.md:21
| [#172](.../issues/172) | Stage 7: integrate one qualified Workspace/FUSE and
  host-daemon runtime with the new core | #171 |
```

Every other mention is a *deferral* or a *scope exclusion*, never a design:

| file:line | what it says |
| --- | --- |
| `docs/roadmap/0.1/0.1.7/README.md:158` | "Stage 7 (#172) integrates the later Workspace/runtime shape." |
| `docs/roadmap/0.1/0.1.7/README.md:348` | "It makes no Commit, LayerStack, Branch, FUSE, daemon or cgroup claim — Stage 7 (#172) owns that half." |
| `docs/roadmap/0.1/0.1.7/retained-history-storage.md:33-35` | the history lane may not claim Commit/LayerStack/Branch/FUSE/daemon/cgroup; "this lane has no runtime envelope at all, and that is the point of it" |
| `core/docs/benchmark/fs-bench-pro-storage-content/CONTRACT.md:48-49,255` | runtime acceptance is Stage 7; "No `init_namespace` 2.7 s target — that is Stage 7." |
| `core/docs/architecture/01-boundary.md:26` | the adapter box is drawn and labelled "(Stage 7 / #172 — not part of core today)" |
| `core/docs/architecture/README.md:224-226` | "**No adapter design.** … Stage 7 (#172) owns what the runtime integration actually becomes." |
| `core/docs/architecture/07-importing.md:35` | the importer proposal is not Stage 7 |

Command that establishes the negative (run from the workdir):

```sh
grep -rn "Stage 7\|stage 7\|Stage7\|#172" --include=*.md --include=*.rs --include=*.py --include=*.toml . \
  | grep -v "^./target"
```

**Consequence for this squad:** question 4 can only be answered as a *bounded
interface sketch grounded in the reference implementation*, not as compliance with
a written spec. It is labelled **[H]** where it goes beyond what the reference code
already does.

---

## 2. Q1 — What the LayerStack layer owns that `core/crates/` does not

The replacement core is three packages and nothing else [S]:
`core/crates/layerfs-content` (C1), `core/crates/layerfs-storage` (C2),
`core/crates/layerfs-telemetry` (timer). C2's own module doc states the separation
explicitly:

```
core/crates/layerfs-storage/src/lib.rs:5-7
"It accepts already-finalized canonical objects from C1 (or from any other producer)
 and can save and read them without running file construction, a Workspace, a history
 entity or a mount."
```

C2's public surface is `cas · encoding · pack · policy · sqlite`
(`core/crates/layerfs-storage/src/lib.rs:30-39`). A grep for the nouns
`commit|layer|branch|workspace` over both core `lib.rs` files returns only
unrelated prose. **There is no history entity of any kind in core.**

The reference LayerStack layer (`crates/layerfs-layerstack-store/`, 30,774 lines of
Rust incl. tests) owns eight responsibility groups that core has no counterpart for:

| # | Responsibility | Reference evidence [S] | Core counterpart |
| --- | --- | --- | --- |
| 1 | **Typed history identity** — `LayerStackId`, `LayerId`, `CommitId`, `BranchId` with tagged byte encodings | `src/ids.rs:10-13` (`BRANCH_TAG=0x11`, `COMMIT_TAG=0x12`, `LAYER_STACK_TAG=0x31`, `LAYER_TAG=0x32`) | none |
| 2 | **History records** — stack/layer/commit/branch rows and their parent links | `src/records.rs:84-115` (`LayerStackRecord{head_layer_id}`, `LayerRecord{parent_layer_id,root_id,source_branch_id,source_commit_id}`, `CommitRecord{root_id,parent_commit_id,base_layer_id}`, `BranchRecord{base_layer_id,head_commit_id}`) | none |
| 3 | **Initialization and layer growth** | `src/layerstack.rs:21` `initialize_layerstack`; `:105-117` writes the first `LayerRecord`+`LayerStackRecord`; `:210` `add_layer` | none |
| 4 | **Workspace lifecycle and the commit pipeline** — lease, pinned snapshot, staging, candidate publication, head movement, up-to-date short-circuit | `src/workspace.rs:19-64` (`WorkspaceLease`, `SnapshotReader`, `PinnedSnapshot`, `CommitOutcome`), `:82` `acquire_workspace_lease`, `:89` `pin_branch`, `:278` `commit_candidate`, `:423` `commit_workspace_candidate` (head-moved check at `:494-505`) | none — C2 has one save owner at a time |
| 5 | **Cross-commit predecessor correspondence** (Q2) | `src/objects.rs:1189-1194`, `:2812-2870`, `:3311-3368`; `src/workspace.rs:677` | none |
| 6 | **Store-wide query, diff and statistics** — records by page, layer/commit history pages, `visit_diff`, canonical/reachable/storage snapshots | `src/query.rs:17-206`, `:263-398` | none |
| 7 | **Storage/commit telemetry receipts** — `PhysicalStorageReceipt`, `WorkspaceCommitReceipt`, `predecessor_hints` | `src/telemetry.rs:11`, `:139-158`, `:210-213` | C2 returns `SaveOutcome`/`DeltaCounters` only |
| 8 | **Runtime capture that feeds the commit** — COW tree, change coalescing, per-file predecessor supply | `crates/layerfs-workspace/src/cow_tree.rs:505-513`; `crates/layerfs-workspace/src/changes.rs:1577-1616`, `:1837` | none |

Items 1–7 live in the reference **store** crate; item 8 lives in the reference
**runtime** crate `crates/layerfs-workspace`, which is a *dependency of the
LayerStack layer's commit path*, not of core. Both are absent from `core/`.

---

## 3. Q2 — What supplies the CROSS-COMMIT delta base in the reference design?

Two different facts have two different owners. Neither is a guess by the store.

### 3.1 The fact "this path's previous version in the previous commit was root X"

Lives in the **Workspace COW tree**, materialized from the **parent layer's inode
record** [S]:

```
crates/layerfs-workspace/src/cow_tree.rs:481-483
    let record = inode_record_lookup(&core, inodes, inode, ...)   // base layer's inode table
crates/layerfs-workspace/src/cow_tree.rs:505-513
    InodeKind::RegularFile => { ... Data::File(FileData::Base {
        root: FileContentRoot(record.content_root), len }) }
```

So the knowledge is *the base inode record's `content_root`*, held per path in the
working tree's COW node. The **Workspace layer** (the runtime) is what holds it.

### 3.2 The fact "which stored object covered these bytes in that previous version"

Computed by the **LayerStack store** during the commit, not by the caller [S]:

```
crates/layerfs-workspace/src/changes.rs:1577-1586   before: Option<InodeRecordV1>, predecessor: Option<FileContentRoot>
crates/layerfs-workspace/src/changes.rs:1836-1842   if let Some(predecessor) = predecessor {
                                                        objects.set_physical_predecessor(self.reader.clone(), predecessor, ...)? }
crates/layerfs-layerstack-store/src/objects.rs:3311-3367  set_physical_predecessor -> DeferredObjectStore.predecessor = Some((reader, FileStateRoot(root), budget, available))
crates/layerfs-layerstack-store/src/objects.rs:2812-2827  on delivery: PredecessorCursor::new(FileStateRoot(root.0))
crates/layerfs-layerstack-store/src/objects.rs:2841-2855  per output object with a first_span: object.prior_ids = cursor.hints(&CoreReader(reader), start, len, ...)
crates/layerfs-content/src/file/rope/read.rs:395-430    hints(): up to 4 payload_object_id hints for the span, from the PREVIOUS version's rope
```

Those `prior_ids` are then the delta-base candidates the admission encoder tries:

```
crates/layerfs-layerstack-store/src/objects/admission.rs:648-649  stats.absent_predecessors += !has_predecessor; if let Some(id) = prior_ids().first()
crates/layerfs-layerstack-store/src/objects/admission.rs:675      stats.predecessor_hints += 1
crates/layerfs-layerstack-store/src/objects/admission.rs:1720-1760  DeltaSearch::candidate(): iterates prior_ids, fetches the base, encodes PREFIX
crates/layerfs-layerstack-store/src/objects/read.rs:669-676        writer declines when prior.depth + 1 > CHAIN_EDGES (8)
```

### 3.3 Answer in one sentence

**The cross-commit base is supplied by the Workspace layer (it owns
path → previous content root, from the parent layer's inode record) and converted
into per-object base ids by the LayerStack store's correspondence cursor over the
previous version's rope; the store never searches for a predecessor on its own
initiative from a path.** The only "search" in the reference store is a *span*
lookup inside a root it was handed.

---

## 4. Q3 — In the v0.1.7 core-only world, is there ANY layer that could legitimately declare a cross-commit base?

### 4.1 C2 cannot [S]

The only route from a caller declaration into C2's delta selection is the advisory
list, read in exactly one place:

```
core/crates/layerfs-storage/src/cas/save.rs:100
    let advisory: Vec<ObjectId> = object.predecessors().ids().collect();
core/crates/layerfs-storage/src/encoding/delta/select.rs:342-354
    fn acquisition(input, role, advisory, depth_cap) { for id in advisory { if probe(...) { return Some(id) } } }
```

C2 has no query for "what did the previous save write for this path", no commit
identity, and no cross-save memory; the candidate cache is operation-local. C2 can
therefore only *obey* a declaration. It cannot originate one.

### 4.2 C1 can — but only for roots it is handed [S]

Three C1 sites already attach a cross-state base, and each requires the caller to
supply the previous identity:

| site | what is declared | where the previous identity comes from |
| --- | --- | --- |
| `core/crates/layerfs-content/src/file/edit/apply.rs:120-122` | whole-file edit declares `AdvisoryPredecessors::explicit(view.root())` (`OriginalBase`) | the `EditRequest`'s base view — caller-supplied |
| `core/crates/layerfs-content/src/filesystem/sorted/page.rs:382-389` | a rewritten sorted page declares its stored `page.origin` (`UnchangedPrefix`); `origin` is set from the base page at `page.rs:473` and `merge.rs:188` | the base tree the caller passed as `FilesystemInput.base` |
| `core/crates/layerfs-content/src/file/mapping/build.rs:119-130` | `push_chunk(raw, predecessor, …)` declares a predecessor chunk | the caller; the only production caller that passes one is the edit path (`edit/apply.rs:325`) |

This is why the baseline lane shows **18,344 whole-file objects with a base even
though the driver declares none**: those bases come from C1's own tree/update path,
not from a cross-commit producer. **[P]** confirms the shape: in
`/tmp/base187/sample.sqlite` the WholeFile role (stored `object_role = 1`,
1-based per `core/crates/layerfs-storage/sql/schema.sql:54`) reaches **max chain
depth 1** across 44,148 objects — i.e. every whole-file base is intra-save.

### 4.3 The only holder of the missing memory in this lane is the harness driver — which is not a layer

```
core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs:610-626
    let mut bases: BTreeMap<ObjectId, ObjectId> = BTreeMap::new();   // new content root -> previous content root
    if faithful { for changed in &transition.changed { ... bases.insert(new_root, old_root) } }
core/benchmark/.../src/ops/history.rs:682-687
    object = object.with_predecessors(AdvisoryPredecessors::explicit(*base)?)
core/benchmark/.../src/ops/history.rs:692-696,702
    previous_content.insert(path, root); previous_root = Some(built.root);
```

That map *is* the LayerStack layer's job, re-implemented inside the benchmark
driver, behind the diagnostic switch `LAYERFS_HISTORY_ADVISORY`
(`history.rs:84-106`), and the switch is an **uncommitted working-tree change** at
HEAD `66bce8378` (`git status --porcelain` → ` M .../src/ops/history.rs`; the only
commit touching that file is `2c63c4fb5`). `core/AGENTS.md` and
`benchmark/AGENTS.md` both put `core/benchmark/` outside product source.

**Answer to Q3: No shipped layer can. The capability exists in C1's API
(`apply_edits` / `update_filesystem`), but the *producer* — the object that
remembers path → previous content root across states — is Stage 7's, and in the
v0.1.7 core-only world it exists only inside the benchmark driver.** That is the
whole of the gap.

---

## 5. Q4 — Minimal Stage 7 surface to make a history save declare cross-commit bases

**Interface sketch only — no implementation, and not a spec (none exists, §1).**
Bounded by what the reference already does [S] and by what core's C1/C2 already
accept.

### 5.1 Inputs

| input | type / shape | source | reference precedent |
| --- | --- | --- | --- |
| previous state identity | one opaque `RootId` (the previous filesystem root) | the caller's own history bookkeeping | `workspace.rs:89 pin_branch`, `PinnedSnapshot` |
| the changed-path set | `[(path, new_root)]` for this state | C1 `update_filesystem` output | `changes.rs` change coalescing |
| per-path previous root | `path -> content_root` resolved **from the previous root**, not from caller memory | a store read of the previous root's inode table | `cow_tree.rs:481-513` |
| the new content | bytes or edit operations | the caller | `changes.rs:1789-1813` |

The critical property: **the previous root must be read back through the Store**,
so the declaration is verifiable rather than asserted. The history lane already
does this for the tree (`history.rs:23-27`: "The base is read back through the
Store").

### 5.2 Outputs

| output | shape | reference precedent |
| --- | --- | --- |
| per-object advisory base | `new_object_id -> base_object_id` with provenance `OriginalBase` | `objects.rs:2841-2855` (`prior_ids`), `admission.rs:1720-1760` |
| a commit/state record | root id + parent id | `records.rs:100-106 CommitRecord` |
| a receipt | declared bases, accepted bases, declined bases and why | `telemetry.rs:210-213`, v0.1.6 receipt fields `base_fetches`, `delta_selected`, `absent_predecessors` |

### 5.3 Cost model (all bounds are the reference's own, so a Stage 7 budget is not invented)

| quantity | bound | evidence |
| --- | --- | --- |
| correspondence scratch per file producer | **576 KiB** (`CORRESPONDENCE_MEMORY`), skipped if the partition cannot afford it | `objects.rs:3329-3330` |
| per-file fetch reservation | **131,136 B** per cursor query, file cap **1 MiB** | `objects.rs:2846`, `:2860` |
| cursor descriptor budget | **4,096 descriptors** then exhausted (no hints, no failure) | `crates/layerfs-content/src/file/rope/read.rs:427-429` |
| hints per output object | **4** | `rope/read.rs:406,418` |
| per-target / per-batch read budget | **512 KiB** target, **8 MiB** batch (encoded and decoded) | `objects/read.rs:59-74` |
| metadata pool budget | **8 MiB** target, **64 MiB** batch | `objects/read.rs:48-57` |
| stored whole-file chain | **8 edges**; the writer declines rather than storing deeper | `objects/delta.rs:7`, `objects/read.rs:669-676` |
| reader tolerance | **50 edges** (deliberately wider than the writer) | `objects/whole.rs:19,364-367` |

Cost shape: one inode-table lookup per changed path (previous root read), then one
bounded rope walk per changed file, then one candidate probe per hint. Measured
shape of that cost on the real corpus is in §6.3.

### 5.4 Non-negotiable preconditions (each is a live blocker today)

1. **One variable.** The declaration must be the only difference between arms
   (`history.rs:98-100` already does this via the environment switch).
2. **Depth discipline.** The producer must not hand C2 a base whose stored chain
   is already at the policy cap — see §7, where exactly that currently aborts the
   lane.
3. **No fallback.** A declined base must remain a FULL record, never a retry or a
   silent alternate route (`core/AGENTS.md`, "One attempted operation").

---

## 6. Q5 — Was v0.1.6's LayerStack present **and wired**? (the crux)

**PROVEN — yes, on three independent legs: source identity, runtime path, and the
run's own counters.**

### 6.1 Leg 1 — the code that supplies cross-commit bases is byte-identical at the campaign's pinned commit and at HEAD [R]+[S]

The v0.1.6 stride-10 receipt records its exact source:

```
benchmark-results/repository-history/stride-10/identity.json -> "source"
  LAYERFS_SOURCE_COMMIT : ac729dfeb4ee923b0a42106a53d1c7de4cd4cfcb
  LAYERFS_SOURCE_DIRTY  : "false"
  LAYERFS_SOURCE_TREE   : 688a9af298f84b87ca17d5eb9c1d3ae4efe02d2f
  LAYERFS_PRODUCT_SEAL  : 970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd
  LAYERFS_COMPILATION_SEAL / LAYERFS_DEPENDENCY_SEAL also recorded
```

```sh
git cat-file -t ac729dfeb4ee923b0a42106a53d1c7de4cd4cfcb     # -> commit
git rev-parse HEAD:<f> ac729dfe...:<f>                        # blob identity
```

| file | blob at `ac729dfe` | blob at HEAD `66bce8378` | identical |
| --- | --- | --- | --- |
| `crates/layerfs-layerstack-store/src/objects.rs` | `5f623d5b9c434498587c626c3028283dc4860e70` | same | **yes** |
| `crates/layerfs-workspace/src/changes.rs` | `532993552dd7a83859cfc357a44ec766e5c5809f` | same | **yes** |
| `crates/layerfs-content/src/file/rope/read.rs` | `e9c6addefb3b8cfd250e242f50fd893fe6fe6c41` | same | **yes** |

And the pinned tree contains the producer at the same line numbers:
`changes.rs:1611,1837,1864,1882,1887` (`set_physical_predecessor`,
`build_complete_with_predecessor`, `build_small_file`),
`objects.rs:2821,2835,2838,3311` (`PredecessorCursor::new`,
`has_predecessor |=`, `set_physical_predecessor`).

### 6.2 Leg 2 — the campaign ran through the runtime, not a low-level store API [R]

```
performance-result.json.command =
  [".../target/release/fs-benchmark-pro", "storage-smoke-session",
   ".../stride-10/deepseek-stride10/host-runtime",
   "f5b65efd...", "performance", "deepseek-full", "-"]
records[0].receipts -> kind "storage-smoke-execution",
  output "storage_smoke_mount=fuse\nhistory-import-ok files=276\n"
  receipt "… transport: Daemon …"
records[0].receipts -> kind "storage-smoke-calls", commit_call_count: 1
manifest: "deepseek-stride10/host-runtime/branch-id", ".../layer-id", ".../store.sqlite"
```

The 17 states are 17 commits into **one** LayerStack: each `records[k]` carries
`commit_id`, `full157_index` ∈ {1,11,…,151,157}, `ordinal` = index×100. The
workload writes through a FUSE mount (`benchmark/fs-bench-pro/workload/storage_smoke.rs:90-157`,
`history-import-ok` at `:156`), i.e. through the same capture→commit path that
calls `changes.rs:1837`.

### 6.3 Leg 3 — the run's own commit-phase receipts show the cross-commit cursor active [R]

Per state, from the `storage-smoke-phase`/`phase: "commit"` receipt's
`physical_storage` block:

| state (full157 index) | 1 | 11 | 21 | 31 | 41 | 51 | 61 | 71 | 81 | 91 | 101 | 111 | 121 | 131 | 141 | 151 | 157 |
| --- | --: | --: | --: | --: | --: | --: | --: | --: | --: | --: | --: | --: | --: | --: | --: | --: | --: |
| `base_fetches` | 0 | 16 | 242 | 736 | 1463 | 1656 | 2522 | 2338 | 3314 | 4066 | 7052 | 7573 | 7858 | 10870 | 9766 | 9908 | 10981 |
| `delta_selected` | 68 | 740 | 1418 | 1588 | 3127 | 2875 | 3953 | 2795 | 3210 | 3185 | 6324 | 6142 | 5978 | 8374 | 8073 | 7134 | 7950 |
| `correspondence_descriptors` | 0 | 0 | 12 | 20 | 17 | 38 | 50 | 53 | 55 | 66 | 66 | 38 | 88 | 58 | 98 | 102 | 103 |
| `absent_predecessors` | 18 | 34 | 24 | 25 | 86 | 66 | 60 | 28 | 103 | 12 | 58 | 108 | 62 | 162 | 119 | 150 | 95 |

Arithmetic (sums over the 17 states):

- `base_fetches` = 0 + 16 + 242 + 736 + 1463 + 1656 + 2522 + 2338 + 3314 + 4066 + 7052 + 7573 + 7858 + 10870 + 9766 + 9908 + 10981 = **80,361 predecessor base hops**, of which **80,361 (100 %)** occur after the initial state.
- `delta_selected` = **72,934** records stored as PREFIX (delta) records.
- `correspondence_descriptors` = **864** descriptors walked by the predecessor cursor.
- `absent_predecessors` = **1,210** objects offered with no predecessor at all.
- Ratio of base hops to delta records: 80,361 / 72,934 = **1.102** hops per selected delta (a chain of depth d costs d hops, so an average depth slightly above 1 is exactly what a per-state delta chain looks like).
- `base_fetches` is incremented at `objects/whole.rs:368-371` on each base hop, and `diag_cursor_attached` is 1..7 in these same receipts — i.e. the cursor was **attached**, not merely available.

`retained_disk` in the same receipt = **allocated 49,344,512 B, apparent 49,315,940 B**,
matching the figure under investigation.

### 6.4 Verdict, and what is *not* proven

**Proven:** v0.1.6's LayerStack store and its Workspace runtime are present, wired
into the recorded campaign's execution path, and demonstrably exercising the
cross-commit predecessor machinery (80,361 base hops across 16 transitions).
**v0.1.7 core has no such layer at all** (§2, §4), and the lane that produced
128,864,256 B ran with the driver declaring **zero** cross-commit bases
(`history.advisory_model = 0`, `/tmp/s0_off/trace.jsonl`; the key does not exist
in the older `/tmp/base187/trace.jsonl`, which predates the switch).

**Therefore the 2.65× is explained in principle by the missing producer.** Two
caveats, both material, neither refuting it:

1. The comparison is **not** layer-for-layer: v0.1.6's pin is a *runtime* pin
   (FUSE + daemon + workspace capture + LayerStack commit), the core pin is a
   *C1/C2-only* pin driven by the benchmark driver. Part of the difference could
   belong to any of the runtime's other choices, not only to the predecessor
   declaration. **One variable is not isolated by the existing pair**; the
   `LAYERFS_HISTORY_ADVISORY` A/B is the isolating experiment, and it is not yet
   complete (§7).
2. **Unproven by me:** I did not re-run the v0.1.6 campaign, and I did not verify
   that the v0.1.6 *store format* accepted every one of those 72,934 deltas (the
   receipts show selections, and `absent_predecessors` shows declines, but I have
   not decoded the v0.1.6 Store's pack directory to attribute bytes).

---

## 7. Adjacent finding (NOT mine — another agent's in-flight runs): the advisory arm currently *aborts* core

This changes what the parent should do next, so it is recorded here with its
provenance. **[P]** Read-only probes of Stores produced at 14:12–14:13 today by a
sibling agent; **no receipt has been published** (`.../step0/` is empty). I did not
run the lane. No timing is reported.

`/tmp/s0_on` (advisory switch on) **failed** the lane:

```
/tmp/s0_on/trace.jsonl:12
  {"kind":"gate","key":"g1.o1-chain-complete",
   "value":"INCOMPLETE|product error: Integrity(\"dependency chain depth\")|every state of the selection is saved, in order"}
/tmp/s0_on/sample.sqlite = 38,912,000 B apparent, phases-perf.json operation_ns = 0
```

Arithmetic over the three Stores (recursive CTE on `objects.base_object_id`;
`object_role` is 1-based per `core/crates/layerfs-storage/sql/schema.sql:54`, so
`1 = WholeFile`, `6 = InodeLeaf`):

| Store | objects | canonical B | with a base | WholeFile max chain depth | InodeLeaf max chain depth |
| --- | --: | --: | --: | --: | --: |
| `/tmp/base187` (advisory **off**, complete) | 52,032 | 380,921,300 | 19,261 (37.0 %) | **1** | 8 |
| `/tmp/s0_off` (off, aborted on `database is locked`) | 18,755 | 133,364,062 | 6,390 (34.1 %) | 1 | 8 |
| `/tmp/s0_on` (advisory **on**, aborted) | 30,437 | 220,750,700 | 20,401 (67.0 %) | **9** | 8 |

Depth histogram of `/tmp/s0_on`: d0 10,036 · d1 7,198 · d2 4,824 · d3 3,161 ·
d4 2,098 · d5 1,373 · d6 862 · d7 506 · **d8 317 · d9 62** (sum 30,437 ✓).

**[S] The off-by-one that makes this fatal** — selection and reconstruction
disagree about what "depth 8" means:

```
core/crates/layerfs-storage/src/encoding/delta/select.rs:373   Ok(depth < depth_cap)          // base admitted when depth < 8
core/crates/layerfs-storage/src/encoding/delta/select.rs:310   depth: base_cost.depth + 1     // so the new object is stored at depth <= 8
core/crates/layerfs-storage/src/encoding/delta/read.rs:159-168 role_depth = delta_depth_for_role(role);
                                                               chain.push(current);
                                                               if chain.len() > role_depth { Err(Integrity("dependency chain depth")) }
```

For an object at depth D the read loop pushes D+1 nodes and errors iff some
j ≤ D−1 has j+1 > role_depth, i.e. **iff D ≥ role_depth**. Selection admits
D = role_depth exactly; the reader rejects D = role_depth exactly. Defaults:
`whole_file_delta_max_depth = 8` (`core/crates/layerfs-content/src/policy.rs:20`),
`chunk = 4` (`:22`), `metadata = 8` (`core/crates/layerfs-storage/src/policy.rs:141`),
ceiling `MAXIMUM_DELTA_MAX_DEPTH = 50` (`:24`). So the **317 WholeFile objects at
depth 8 are storable and unreadable**, and the 62 at depth 9 are storable because
the depth cache under-reported them.

The reference design does not have this asymmetry: it declines on the write side at
`prior.depth + 1 > CHAIN_EDGES (8)` (`objects/read.rs:669-676`, `objects/delta.rs:7`)
and tolerates up to `EDGES = 50` on the read side (`objects/whole.rs:19,364-367`) —
a reader bound **strictly wider** than the writer's. That is why 80,361 base hops
completed there and why the same declaration aborts here.

**Labelled [H], not proven:** that this off-by-one is the *sole* cause of
`/tmp/s0_on`'s abort. It is consistent with the error string's only source
(`read.rs:168`), with the observed depth-8 population, and with the off-arm's
depth-1 WholeFile ceiling — but I did not reproduce the run. A confirmation run
with the advisory switch on, plus a probe of the first object whose read fails, is
the cheap next step, and it is *not* mine to take (§8).

---

## 8. Negative results (what I ruled out, how, and with what number)

1. **A Stage 7 specification exists in-tree — RULED OUT.** Global grep over
   `*.md|*.rs|*.py|*.toml` (excluding `target/`) finds only deferrals and one
   scope row (`implementation-issues.md:21`). No design, no interface, no budget.
2. **Core has a hidden history/commit layer — RULED OUT.** `core/crates/` contains
   exactly 3 packages; C2's public surface is `cas/encoding/pack/policy/sqlite`
   (`lib.rs:30-39`); the nouns commit/layer/branch/workspace appear only in prose
   (`lib.rs:5-7` explicitly denies a Workspace or history entity).
3. **C2 could originate a cross-commit base — RULED OUT.** Exactly one advisory
   read site (`cas/save.rs:100`) and one consumer (`select.rs:342-354`); the
   candidate cache is per-save (`encoding/delta/candidates.rs`).
4. **The harness driver is a layer — RULED OUT.** It lives under
   `core/benchmark/`, which `core/AGENTS.md` and `benchmark/AGENTS.md` both place
   outside product source; the only commit touching `history.rs` is
   `2c63c4fb5`, and the advisory switch is an uncommitted working-tree change.
5. **v0.1.6's predecessor machinery was dormant — RULED OUT** by 80,361 base hops,
   864 correspondence descriptors and `diag_cursor_attached > 0` in the recorded
   commit receipts (§6.3), with the pinned source blob-identical to HEAD (§6.1).
6. **The core off-arm already has over-deep WholeFile chains — RULED OUT.**
   `/tmp/base187` WholeFile max depth = **1** over 44,148 objects. The over-depth
   chains appear only in the advisory-on Store.
7. **A workload difference explains the 2.65× — RULED OUT by the parent's own
   numbers** (v0.1.6 stride-3 union/canonical = 583,508,923 / 589,423,458 =
   1.010×), which I did not re-derive; I record it as inherited, not measured here.

---

## 9. What I did not do, and what remains unproven

- I did **not** run any lane (no `fs-bench-storage-content` invocation), and
  **no timing** in this report is a measurement.
- I did **not** decode the v0.1.6 Store's pack directory, so I cannot attribute the
  v0.1.6 49,315,940 B to delta bytes versus other savings. **Unproven.**
- I did **not** reproduce `/tmp/s0_on`'s abort; the off-by-one in §7 is a code-read
  argument plus an artifact probe, labelled **[H]** where it claims causation.
- The exact `ObjectRole` discriminant mapping (1 = WholeFile) is inferred from
  `schema.sql:54` (1..13) plus the enum order in
  `core/crates/layerfs-content/src/object/output.rs:13-43`; the count match
  (role 1 = 44,148 objects in `/tmp/base187` ≈ the parent's whole-file total
  18,344 + 25,804 = 44,148) corroborates it. **Inference, not a decoded table.**
- Whether a Stage 7 producer should live in a new crate, in C2, or as an adapter
  is **not decided here** — `core/docs/architecture/README.md:224-226` leaves it
  open and no spec closes it.

---

## 10. Exact commands used

```sh
# Stage 7 spec search (negative result §1)
grep -rn "Stage 7\|stage 7\|Stage7\|#172" --include=*.md --include=*.rs --include=*.py --include=*.toml . \
  | grep -v "^./target"

# Reference-layer responsibility map (§2, §3)
grep -n "pub fn \|pub struct \|pub enum " crates/layerfs-layerstack-store/src/{workspace,query,layerstack}.rs
grep -n "CREATE TABLE" crates/layerfs-layerstack-store/sql/*/*.sql
sed -n '2812,2870p;3311,3368p' crates/layerfs-layerstack-store/src/objects.rs
sed -n '470,545p;1560,1900p' crates/layerfs-workspace/src/{cow_tree,changes}.rs

# v0.1.6 pin identity (§6.1)
git cat-file -t ac729dfeb4ee923b0a42106a53d1c7de4cd4cfcb
git rev-parse HEAD:crates/layerfs-layerstack-store/src/objects.rs \
               ac729dfeb4ee923b0a42106a53d1c7de4cd4cfcb:crates/layerfs-layerstack-store/src/objects.rs
git show ac729dfe...:crates/layerfs-workspace/src/changes.rs | grep -n set_physical_predecessor

# v0.1.6 receipt arithmetic (§6.3)
python3 - <<'EOF'
import json
d=json.load(open('benchmark-results/repository-history/stride-10/deepseek-stride10/performance-result.json'))
for k in ('base_fetches','delta_selected','absent_predecessors','correspondence_descriptors'):
    v=[r['physical_storage'].get(k) for rec in d['records'] for r in rec['receipts']
       if isinstance(r,dict) and r.get('kind')=='storage-smoke-phase' and r.get('phase')=='commit']
    print(k, v, 'sum', sum(v))
print(d['retained_disk'], d['records'][0]['receipts'][1]['output'])
EOF

# Store probes (§4.2, §6.4, §7) — read-only
python3 - <<'EOF'
import sqlite3
q='''WITH RECURSIVE chain(id,role,d) AS (
       SELECT object_id,object_role,0 FROM objects WHERE base_object_id IS NULL
       UNION ALL SELECT o.object_id,o.object_role,c.d+1 FROM objects o JOIN chain c ON o.base_object_id=c.id)
     SELECT role,max(d),count(*) FROM chain GROUP BY role ORDER BY max(d) DESC'''
for p in ('/tmp/base187/sample.sqlite','/tmp/s0_off/sample.sqlite','/tmp/s0_on/sample.sqlite'):
    c=sqlite3.connect('file:%s?mode=ro'%p,uri=True)
    print(p, c.execute('select count(*),sum(canonical_length),sum(base_object_id is not null) from objects').fetchone(),
          list(c.execute(q)))
EOF
```

---

## 11. One-line answer to the squad's question

**Stage 7 would supply the only object that can remember, across states, that a
path's previous version was content root X — and therefore the only object that can
hand C2 the advisory base C2's delta selector needs. v0.1.6 had it (proven:
byte-identical pinned source, a runtime execution path, and 80,361 recorded
predecessor base hops). v0.1.7 core does not, and the lane's own driver — which is
not a layer — was run with that declaration switched off.**
