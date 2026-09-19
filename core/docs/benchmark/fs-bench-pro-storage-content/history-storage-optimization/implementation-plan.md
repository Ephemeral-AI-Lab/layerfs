# Implementation plan and rollout

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract and not a receipt.
> No figure in this document is a measurement.

## 1. Scope

**In:** the harness at
[`core/benchmark/fs-bench-pro-storage-content/`](../../../../benchmark/fs-bench-pro-storage-content/)
and this documentation. **Out:** every product source file. If a step appears to need a
product change, stop and report it rather than widening a product API — `Store::path()` is
already public, which is enough for the runner to `stat` the Store.

Also out: the 217 admission rows. They keep their registry, cardinality, golden table,
`--lane full` composition, `full` verification default and their `InProcess` / `ObjectSet` /
`BaseStore` / `InputTree` preparation declarations. This lane adds a group and three lanes,
never members.

Paths are relative to the harness root unless stated otherwise.

## 2. Files

### 2.1 `src/workload/history.rs` — new

**Responsibility.** Read and authenticate the corpus, and hand the driver, per state, its
identity and its changed bytes. It constructs nothing, stores nothing and times nothing.

```rust
pub const MANIFEST_SHA256: &str = "03f21acf…";
pub const SOURCE_TIP: &str = "b0a7d2ce…";

/// One of the three selections.
pub enum Row { Stride10, Stride3, Stride1 }

impl Row {
    /// `full157` indices, ascending. Stride10 = `range(1,158,10) ∪ {157}` (17),
    /// Stride3 = `range(1,158,3)` (53), Stride1 = all (157).
    pub fn selection(self) -> &'static [u16];
    /// `"stride10" | "stride3" | "stride1"`, for trace keys and row IDs.
    pub fn token(self) -> &'static str;
}

/// One checkpoint's identity.
pub struct State {
    pub ordinal: usize,        // campaign index, 1-based, within the selection
    pub full157_index: u16,    // the original index in the manifest
    pub sha: String,           // source commit
    pub tree: String,          // source tree id
    pub manifest_sha256: String,
    pub oracle_path: PathBuf,  // oracles/<sha>.json
    pub oracle_sha256: String, // computed once at open
    pub logical_bytes: u64,
    pub paths: u32,
}

/// What one state changes relative to the previous *selected* state.
pub struct Transition {
    pub state: State,
    pub changed: Vec<ChangedPath>,
    pub blobs: BlobMap,        // oid -> bytes, for the changed set only
}

pub struct ChangedPath {
    pub path: Vec<u8>,         // hex-decoded from manifest.tsv
    pub mode: u32,
    pub oid: [u8; 20],
    pub size: u64,
    pub kind: Change,          // Added | Modified | Removed | MetadataOnly
}

pub struct Corpus { /* private: root, manifest, states, accumulated blob map */ }

impl Corpus {
    pub fn open(root: &Path, row: Row) -> Result<Self, HistoryError>;
    pub fn states(&self) -> &[State];
    /// Reads `inputs/<sha_k>/manifest.tsv` and `previous.tsv`, diffs them, and
    /// accumulates the blob map forward. `&mut self` because the map grows.
    pub fn transition(&mut self, ordinal: usize) -> Result<Transition, HistoryError>;
    pub fn oracle(&self, state: &State) -> Result<Oracle, HistoryError>;
    /// Path-states and cumulative logical bytes, for the pins.
    ///
    /// `path_states` is the entry count of `oracles/<sha>.json` — files **and**
    /// directories — and not `checkpoints[].files` or the `manifest.tsv` line
    /// count, both of which are short by about 18 % (erratum E1).
    pub fn pins(&self) -> Pins;
}
```

**Behaviour, and every refusal.**

| step | requirement | failure |
| --- | --- | --- |
| `open` | `sha256(checkpoint-manifest.json) == MANIFEST_SHA256` | `ManifestIdentity` |
| `open` | `manifest.tip == SOURCE_TIP` and `checkpoints.len() == 157` | `SourceTip`, `CheckpointCount` |
| `open` | the selection is strictly ascending, starts at 1, ends at 157, and has the declared length | `SelectionShape` |
| `open` | each selected `manifest_sha256` and oracle hash is recorded | `TreeManifest`, `OracleMissing` |
| `pins` | `path_states` is the **entry count of `oracles/<sha>.json`**, which counts files **and** directories (**erratum E1**); cumulative logical bytes come from the manifest | — |
| `transition` | `manifest.tsv` and `previous.tsv` parse as `mode \t oid \t size \t hex path` (**erratum E2**; the column order in the first revision of this section was wrong) | `TreeManifest` |
| `transition` | every blob served is re-hashed and equals both its oid and its `blob_digests` entry | `BlobIdentity` |
| any | the corpus root exists and is readable | `CorpusMissing` |

`HistoryError` carries one variant per row above, plus the offending path or oid. There is no
default and no fallback: an unidentifiable blob is refused, never assumed.

**Accumulation is the part that must not be got wrong.** `inputs/<sha>/blobs/` holds only the
blobs *that transition* changed, but a state's tree references blobs introduced by earlier
transitions. The reader therefore keeps a forward map across the whole selection and re-reads
nothing. A state whose tree needs an oid the map lacks is `BlobIdentity`, not a partial tree.

**A JSON reader is part of this file.** `checkpoint-manifest.json` and `oracles/<sha>.json` are
JSON and the harness has no parser for them; none may be added, because
`shared/test_lock_parity.py` fails a harness-only registry package and `serde_json` is absent
from `core/Cargo.lock` (erratum E4). The harness already hand-rolls SHA-256 for the same reason
(`workload/digest.rs`). The reader is written out here, strict and narrow: it accepts exactly the
two documents above, refuses a duplicate key, a trailing comma, an unescaped control character
and a non-UTF-8 string, and never allocates a value it does not need. It is not a general JSON
library and must not grow into one.

**Does not:** construct canonical objects, open a Store, call `Timing`, or mutate the corpus.
Every path is opened read-only.

### 2.2 `src/families/history.rs` — new

**Responsibility.** The three rows and nothing else — the same rule `families/mod.rs` states
for every other family. No operation, fixture or oracle body lives here.

```rust
pub const GROUP: &str = "history.*";
pub fn cases() -> Vec<Case>;
```

| field | `history-stride10` | `history-stride3` | `history-stride1` |
| --- | --- | --- | --- |
| `family` | `history.*` | `history.*` | `history.*` |
| `tier` | `u8::MAX` | `u8::MAX` | `u8::MAX` |
| `tier_label` | `""` | `""` | `""` |
| `profile` | `""` | `""` | `""` |
| `bytes` | 561,010,345 | 1,676,767,835 | 4,936,693,030 |
| `entries` | 17 | 53 | 157 |
| `smoke` | `false` | `false` | `false` |
| `cache` | `CreatedInSample` | `CreatedInSample` | `CreatedInSample` |
| `store` | `CreatedInSample` | `CreatedInSample` | `CreatedInSample` |
| `prepared` | `InProcess` | `InProcess` | `InProcess` |
| `admission` | `Admission` | `Admission` | `Admission` |
| `shape` | `Shape::History(Row::Stride10)` | `Row::Stride3` | `Row::Stride1` |

`bytes` is the cumulative logical bytes of the selection, so the byte axis and the state axis
are both carried by the existing `Case` fields.

**`Preparation::InProcess` is the correct declaration**, for the same reason `c1.construct.*`
carries it: the construction **is** the measured operation, and preparing it would move the
measurement into setup. No `ObjectSet`, no `BaseStore`, no `InputTree`.

**No row is in `--smoke`.** The smoke lane is one tier per family and these rows are whole
histories; `benchmark_rules.md` §15 forbids a default invocation launching an endurance run.

### 2.3 `src/ops/history.rs` — new

**Responsibility.** The driver, in three phases.

```rust
pub fn run(case: &Case, row: Row, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError>
```

#### `Phase::Prepare`

Authenticate the corpus, publish the pins, write nothing. No Store is created —
`phases::preparation_is_the_invocation()` already declares the whole invocation as
preparation, so `preparation_wall_ns` is a measurement rather than a remainder.

Counters: `history.states`, `history.logical_bytes`, `history.path_states`,
`history.manifest_sha256`, `history.source_tip`, `history.oracle_sha256.{ordinal}`.

#### `Phase::Perf` — the measured chain

```text
create the output directory
for k in 0..n:
    transition = corpus.transition(k)                    untimed, harness
    (result, report) = measure("history.state.{k+1}", |scope| {
        construct the changed content                    C1
        build_filesystem(base = previous root)           C1
        save                                             C2
    })
    publish the child's counters from `report`
close the Store
take the storage readings
```

State 1 builds a new filesystem (`base: None`); every later state passes the previous state's
root, **read back from the Store through `StoreProvider`**, so the chain genuinely goes
C1 → C2 in the read direction and not out of harness memory.

`Store::create` sits inside state 1's child and is published as `history.state.1.create_ns`, so
a fixed cost is visible and subtractable rather than buried in the chain.

**Per-state counters** (`Kind::Counter`):

| key | meaning |
| --- | --- |
| `history.state.{k}.root` | the state's filesystem root `ObjectId` — the O1 pin |
| `history.state.{k}.changed_paths` | entries the transition changed |
| `history.state.{k}.changed_bytes` | logical bytes the transition changed |
| `history.state.{k}.objects` | canonical objects the child emitted |
| `history.state.{k}.canonical_bytes` | canonical bytes the child emitted |
| `history.state.{k}.save_ns` | the C2 half alone, from the product's own tree |

**Row-level counters:** `history.states`, `history.path_states`, `history.logical_bytes`,
`history.operation_ns` (the sum of the children), `store.path`, `store.create_ns`.

**Storage readings** (`Kind::Resource`), taken **before state 1 and after the last state,
outside every timer**: `space.allocated_bytes`, `space.apparent_bytes`, `space.page_count`,
`space.freelist_count`, `space.pack_bodies_bytes`, `space.nonpack_bytes`,
`space.allocation_difference_bytes`, `space.canonical_bytes.{role}`,
`space.object_rows.{role}`, `space.before.allocated_bytes`.

**Resource counters:** `heap.peak_incremental_bytes`, `heap_charged_bytes`,
`heap_allocations`, `cpu.user_ns`, `cpu.system_ns`, `rss.phase_peak_bytes`,
`rss.incremental_peak_bytes`.

#### `Phase::Verify`

Reopen the Store **read-only** and, for every state: read the root back (O1 against the pinned
constant), read the tree back and compare with `oracles/<sha>.json` (O4), and read the
deterministic sample of that state's manifest back through the Store (O2). Counters:
`verify.states`, `verify.path_states`, `verify.sampled`, `verify.decoded_bytes`,
`verify.requested_bytes`, `verify.disagreements`.

#### Gates

| gate | class | requires | id |
| --- | --- | --- | --- |
| state roots | Correctness | every state's root equals its pin | `g1.o1-state-root` |
| pinned counters | Correctness | canonical bytes and object counts equal their pins | `g1.o3-pinned-counters` |
| state trees | Correctness | every state's tree equals its oracle | `g1.o4-state-tree` |
| sampled bytes | Correctness | the sampled files read back byte-exact | `g1.o2-sampled-bytes` |
| pack accounting | Correctness | `pack_bodies <= database` | `g1.pack-accounting` *(existing)* |
| handoff | Mechanism | objects C1 emitted equal objects the save acknowledged | `g2.handoff` *(existing)* |
| one file | Cleanup | exactly one Store file | `g5.one-file` *(existing)* |
| no sidecars | Cleanup | no `-wal` / `-shm` / `-journal` | `g5.no-sidecars` *(existing)* |
| store exists | Custody | the Store the run names is on disk | `g6.store-exists` *(existing)* |
| tree complete | TimingPurity | the product's timing tree is complete | `g7.tree-complete` *(existing)* |
| swaps | Resource | zero swaps | `gates::swap_gate` *(existing)* |

Existing ids are **reused, not re-spelled**: a second spelling of the same gate would let one
of them drift.

**Two claims are lane-level, not per-row.** "Per-state work time does not rise with the number
of states" and "peak heap does not rise with the number of states" are comparisons **across the
three rows**, evaluated by the runner from the three receipts. They are not driver gates, and
`implementation-plan.md` §3's Phase 4 and 5 exit criteria name them.

**Fail-closed.** A state whose root or tree disagrees fails the row and increments
`verify.disagreements`. The count is published; "156 of 157" is a failure, not a pass.

### 2.4 `tests/history_declarations.rs` — new, and **Phase 3** (erratum E3)

Asserting the rows exist is not possible before the registry rows do, and the registry change is
Phase 3's. Phase 1's exit criteria list this test by mistake in the first revision of §3; the
corpus reader is Phase 1's deliverable and this test is Phase 3's.

Asserts, without a corpus and without a product run:

- exactly three rows exist and their ids are `history-stride10`, `history-stride3`,
  `history-stride1`;
- `entries` are 17 / 53 / 157 and `bytes` equal the pinned cumulative logical bytes;
- every row declares `CreatedInSample` / `CreatedInSample` / `InProcess`;
- no row is in `--smoke`;
- each lane selects exactly its own row, and `--lane full` selects **none** of them;
- the group is not counted in the 217.

### 2.5 `tests/golden/history-expected.tsv` — new

The pinned table, embedded with `include_str!` from `src/workload/expected.rs` so the harness
identity covers it and a table edited without a rebuild is not the table that ran.

| column | rows |
| --- | --- |
| `case_id` | the three row ids |
| `kind` | `root` · `canonical_bytes` · `canonical_objects` · `allocated_bytes` · `path_states` · `logical_bytes` |
| `key` | the state's `full157_index` for `root`, empty otherwise |
| `value` | the pinned value |

`kind=root` is established by the first full run of each row and thereafter is a gate — the
mechanism `src/workload/expected.rs` already uses for the 217, whose constants came from the
last all-`PASS` run. `canonical_bytes`, `canonical_objects`, `path_states` and `logical_bytes`
are pinned from the v0.1.6 record for stride-3 and stride-1, and from the first run for
stride-10, which has no recorded canonical total.

### 2.6 `shared/history_corpus.py` — new

**Responsibility.** The runner's view of the corpus: the identity it records in every receipt,
and the pins it checks before a run starts. It does not read blob bytes — the driver does that.

```python
MANIFEST_SHA = "03f21acf…"
SOURCE_TIP = "b0a7d2ce…"
SELECTIONS = {"history-stride10": (...), "history-stride3": (...), "history-stride1": (...)}
PINS = {"history-stride3": {"path_states": 306_861, "logical_bytes": 1_676_767_835,
                            "canonical_bytes": 589_423_458, "canonical_objects": 73_476}, ...}

def identity(root: Path) -> dict      # manifest sha, tip, checkpoint count, totals
def selection(lane: str) -> tuple     # the full157 indices
def pins(lane: str) -> dict           # the pinned values, or {} where none is recorded
def self_check() -> dict
```

`identity` raises rather than returning a partial document: a missing corpus, a manifest whose
SHA differs, or a tip that differs is a refusal with the reason.

### 2.7 `shared/test_history_corpus.py` — new

- a mutated manifest SHA is refused;
- a wrong tip is refused;
- a truncated checkpoint list is refused;
- the three selections enumerate the declared indices, and only those;
- `pins` returns the recorded values for stride-3 and stride-1 and an empty mapping for
  stride-10, rather than inventing zeros.

### 2.8 Modified files

| file | change |
| --- | --- |
| `runner.py` | `--corpus` (default the pinned path, fail closed); the three lanes; `history_corpus.identity()` into every receipt's identity block; `history_corpus.pins()` checked before the run; the lifted budget for these rows (`DECLARED_EXCEPTIONS` or a declared per-lane ceiling); the storage delta composed into the derived receipt |
| `src/main.rs` | `--corpus PATH` and the three lane names; nothing else |
| `src/registry.rs` | `Shape::History(Row)`; the `history.*` group in the `ALL` / `GROUP_IDS` arrays; lane membership; a `FROZEN_CARDINALITY` entry of 3, kept **outside** the 217 |
| `src/families/mod.rs` | `pub mod history;` and the two group arrays |
| `src/ops/mod.rs` | `Shape::History(row) => history::run(case, row, context)` in the dispatch |
| `src/workload/mod.rs` | `pub mod history;` |
| `src/workload/expected.rs` | include `history-expected.tsv` and expose its lookups; the 217 table is untouched |
| `shared/space.py` | a `delta(before, after)` shape, and canonical bytes and objects grouped by `object_role` |
| `shared/phases.py` | the lane-scoped reconciliation of owner ruling 2 (**erratum E5**): for a row whose `operation_ns` is the sum of named children, require `root >= Σ children` and publish the difference as the harness's own untimed work, instead of failing closed on `root == operation_ns`. Every other row keeps the existing exact check. |
| `shared/analyze.py` | the report shape of [`measurement.md`](measurement.md) §5, and the two lane-level scaling claims |
| `tests/golden/registry.tsv` | the three rows appear, so the golden comparison covers them |
| `../CONTRACT.md`, `../README.md` | one line each: `history.*` is a separate claim under this document set, not an amendment |

**Unchanged:** `ops/c1.rs`, `ops/c2.rs`, `ops/fs.rs`, `ops/fs_fixture.rs`, `ops/pipeline.rs`,
`workload/artifact.rs`, `shared/copyladder.py`, `shared/residency.py`, every existing family,
and every existing golden row. This lane needs no artifact format, no copy ladder and no
de-warm.

## 3. The rollout

Seven phases. A phase is entered only when the previous phase's exit gate has **PASSED**.

### Phase 0 — specification and rulings

Deliverables: these five documents, the roadmap specification under
`docs/roadmap/0.1/0.1.7/`, and the sub-issue.

Exit: the README's seven owner decisions are ruled, the three row IDs and lanes are frozen,
and the pins of [`verification.md`](verification.md) §4 are written down. `benchmark_rules.md`
§1 is explicit that a family measured before its specification exists is exploratory with
`admission_eligible=false`, so no product run happens here.

### Phase 1 — corpus reader

Deliverables: `src/workload/history.rs`, `shared/history_corpus.py` and the self-checks. No
registry change, no product run.

Exit: the corpus authenticates against every identity in [`README.md`](README.md) §3; the three
selections enumerate exactly 17/53/157 at the right indices; the path-state and logical-byte
pins match — **101,477 / 561,010,345**, **306,861 / 1,676,767,835**, **904,143 /
4,936,693,030**; an unidentifiable blob is refused. `history_declarations` is **not** part of this gate — it
needs the registry rows, which Phase 3 owns (erratum E3).

### Phase 2 — the remaining harness gap

**Phase 2a is already closed**, by `2f8ebc90d` (*"the two unwired axes"*), which is why it is
not work here: `support/phases.rs` now reads `getrusage` at both phase boundaries and publishes
`cpu_user_ns`, `cpu_system_ns` and `process_peak_rss_bytes`; `shared/phases.py` reads them
tolerantly (a missing reading is absent, never a zero that would read as "used no CPU"); and
the runner composes them into the receipt's `resources`. The 10 ms `RssSampler` stays
deliberately unwired, with the reason recorded.

What remains:

| | gap | fix |
| --- | --- | --- |
| 2b | the full Store reading is published for six rows only, and is read once from a retained file by the runner's `verify` | the `delta(before, after)` shape and the `object_role` split in `shared/space.py`, plus the before/after pair taken inside the invocation |

`resources['artifact.data_bytes']` already exists per row, so what 2b adds is the **Store's**
before/after pair and the canonical-bytes-by-role split.

Exit: a full-lane re-run carries the new fields, every pinned counter is unchanged, and
`sum(operation_ns)` does not move.

### Phase 3 — `history-stride10` (17 states) — the P0 smoke tier

The registry rows, the driver, the gates, the golden table, and the first measured run. This
phase brings the **smoke tier live**, and it is where bug fixes and optimizations are first
tried from here on.

Exit: the row is `PASS`; corpus pins match; the final verification gate passes over all 17
states; the storage readings and the attribution are published; every counter reconciles;
`verify` re-derives all six phase numbers.

**This phase also measures the baseline from which the tier budgets are declared.** The
per-tier **complete-command** budgets are fixed here and recorded with their source, before
any optimization work — `benchmark_rules.md` §11 allows a target that needs an untouched
baseline to be frozen after that baseline, but not after candidate sampling.

The **verification** ceilings are already declared and are not derived here: **10 s** at 17
states, **20 s** at 53, **30 s** at 157 (owner direction, 2026-09-19; roadmap specification
§11.1). They are tighter than the 60 s contract default, which does not transfer in either
direction — v0.1.6's read-back was 199.8 s at 53 states and 570.6 s at 157. What makes them
reachable is fixed here too: O1 is a root read per state, O4 is metadata with no read-back,
and **O2 is deduplicated by distinct object before it is sampled**, so an object shared by
many states is decoded once rather than once per referencing file. A verifier that misses
its ceiling is `TARGET_MISS` with its measured wall, never a shrunken sample.

### Phase 4 — `history-stride3` (53 states) — the P0 intermediate tier

This phase brings the **intermediate tier live**: it is the second gate every candidate must
pass, at 53 states rather than 17.

Exit: canonical content **589,423,458 B / 73,476 objects**; **306,861** path-states;
**1,676,767,835** logical bytes; Store allocated **below** v0.1.6's 64,024,576 B; the
allocated ÷ cumulative-logical ratio at or better than 26.2×; the full attribution table; the
final verification gate passes over all 53 states; **per-state work time and peak heap have
not risen against Phase 3**.

### Phase 5 — `history-stride1` (157 states) — the run-only tier

**This tier is never optimized.** It is run to confirm, once per candidate that has already
passed Phases 3 and 4, and its result is recorded rather than iterated on.

Exit: canonical content **871,588,115 B / 104,705 objects**; **904,143** path-states;
**4,936,693,030** logical bytes; Store allocated **below** 83,947,520 B; ratio at or better
than 58.8×; the declared run ceiling met; the final verification gate passes over all 157
states; **per-state work time and peak heap have not risen against Phases 3 and 4**.
Explicitly selectable and never a default.

### Phase 6 — the optimization campaign

Promotion runs one way only, and never backwards. stride10 and stride3 are the **P0** working
tiers; stride1 is **run only**:

```text
iterate on stride10          P0, cheap reject
  → confirm on stride3       P0, matched n3 alternating pairs
    → run stride1 once       confirmation only, never tuned
```

Each ledger entry records the mechanism, before/after allocated, apparent and canonical bytes,
the `object_role` split, the read amplification and the verdict — with negative outcomes
preserved. The ledger is created at this phase as `optimization-ledger.md` beside these
documents, mirroring the v0.1.6 campaign's
`docs/roadmap/0.1/0.1.5/issue100/optimization-checklist-and-experiment-ledger.md`; it does not
exist yet.

## 4. Stop rules

- **A canonical pin that does not match at Phase 4 or 5 stops the phase.** It is a finding
  about the migration, not a fixture to adjust.
- **A Store allocated above v0.1.6's recorded bytes is a finding**, not a new baseline: the
  core Store carries strictly less metadata, so it must land below.
- **Per-state work time or peak heap that rises with the number of states is a finding** —
  each save would be rescanning history, or the chain would not be streaming.
- **A row that cannot fit its declared budget is reported with its measured wall.** The budget
  was declared before the run; it is never inflated afterwards to turn a miss into a pass.
- **A reconciliation failure is `INCOMPLETE`**, never a quiet pass.
- **No optimization targets stride1.** No constant, threshold, buffer, batch size or policy
  value is changed because of a stride1 number; stride1 runs once per candidate that has
  already passed stride10 and stride3; and a stride1 failure while both lower tiers pass is a
  scaling finding to investigate at stride10 and stride3, not to patch at stride1.
- **A tier-spanning rise in per-state work time or peak heap is P0 at stride3**, where it is
  cheap to see, rather than something to discover at stride1.
- **Nothing in a timed region reads the corpus**, and no constructed object is supplied to a
  measured child.

## 5. Production LOC accounting

`core/tools/check_product_boundary.py` scans only `core/crates/*/src` and `core/crates/*/sql`,
and the repository's LOC rule excludes benchmark harnesses from production LOC. Every commit in
this campaign therefore reports the **production total unchanged, delta 0**, with harness lines
stated separately. The 999-line ceiling does not bind here, though the harness's own convention
of one cohesive module per concern still does.
