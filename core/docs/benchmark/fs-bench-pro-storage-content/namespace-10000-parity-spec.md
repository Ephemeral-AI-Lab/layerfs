# Spec: a v0.1.7 row that performs the work `namespace-10000` measures

> **Status: proposed specification, pre-registered before any harness code.** It
> registers a **new** row and therefore amends a frozen contract
> ([`CONTRACT.md`](CONTRACT.md) §3, 217 rows) and its golden table
> (`core/benchmark/fs-bench-pro-storage-content/tests/golden/expected.tsv`). Under
> `docs/general/benchmark_rules.md` §1 that needs an owner ruling **before** the row
> runs. **No implementation is authorized by this page, and it makes no performance
> claim.** It exists so that "match the same work" is a definition rather than an
> improvisation.

## 1. Why this spec exists

The v0.1.6 case `namespace-10000` measures an operation that no v0.1.7 row performs.
`c1.fs.build-scale`'s `namespace-10000` carries the same id and the same 10,000-file /
100-directory topology, but it is a **tree build with no bytes**: its fixture recipe is
a pure function of `(profile, entries, directories, seed)` and its `InodeValue` is
`{kind, namespace_ref_count, content_root, metadata_root}` — there is no size field, and
each file's `content_root` is `ObjectId::for_bytes("layerfs/fs-bench/{profile}/{seed}/{label}")`.
Measured on the current tree it is 313–320 ms emitting **382 objects** and declaring
**0 bytes**, against the v0.1.6 row's **25,158 objects** and **302,182,831 canonical bytes**.

That difference is the whole point of this spec: v0.1.7 has no row whose timed phase
writes ~300 MB of file content through the Store as part of building a 10,000-entry
namespace.

## 2. The work to be matched, stated exactly

From `benchmark/fs-bench-pro/src/main.rs::namespace_init_diagnostic` and the sealed
receipt `benchmark-results/issue219/s1-ns10000-candidate-20260921T031259Z/perf.jsonl`
(source `b0260df3a`, dirty=false):

| element | value |
| --- | --- |
| family / case | `init_namespace` / `namespace-10000` |
| route | `namespace`, topology `host-store` |
| timer | `layerstack_init_ns` — the **product's own** telemetry root |
| setup | `fresh-output` (Store created inside the sample, outside the timer) |
| fixture | 10,000 regular files, **100** data directories, 300,000,000 logical bytes; 7,899 tiny + 1,500 small + 500 medium + 100 empty + **1 anchor of 100,000,000 bytes** |
| fixture profile | `synthetic-small-heavy-v2` |
| timed phase | scan the fixture (10,000 files / 300,000,000 bytes), construct content objects, admit them to the Store, commit |
| outside the timer | `setup_ns` (Store create, client, container binding) and `teardown_ns` |
| observed output | `candidate_objects` 25,158, `candidate_bytes` 302,182,831, `inserted_objects` 25,158, `store_database_bytes` 304,771,072, `initialize_admission_transactions` 73, `initialize_max_transaction_bytes` 4.15 MB |
| cache declaration | `fixture_cache_profile: reused-first-sample-uncontrolled`; `cache_contract: null` (the harness's cold contract covers only `namespace-100000`) |
| reference figures | 944.881 ms / 423.3 MB/s over the case's 400 MB (this machine, sealed); the historical 578.245 ms row is `SOURCE_DIRTY=true` and read 0.0 MB from storage |

**What "the same work" therefore means:** inside one timed phase, build a 10,000-entry
namespace over 100 directories **whose files carry real content totalling ~300 MB**, and
persist that namespace and its content through the Store with the save acknowledged.

## 3. The v0.1.7 row that comes closest, and how it falls short

`pipeline-filesystem-build` (`src/families/pipeline.rs`, driver `src/ops/pipeline.rs::filesystem`)
is the only v0.1.7 row that already does an integrated C1→C2 filesystem handoff. Its
timed phase is exactly:

```text
Store::open(sample) → begin_save → build_filesystem(input, SaveHandoff via CountingConsumer)
                    → operation.finish()
```

That is the right *shape*. It falls short in three ways:

| | `pipeline-filesystem-build` | `namespace-10000` needs |
| --- | --- | --- |
| files / directories | 1,000 / 10 | **10,000 / 100** |
| file content | none — empty inodes, `base_bytes: 0`, `PairProvider::new(&empty, &empty)` | **~300 MB of real content** |
| bytes declared | `artifact.data_bytes: 0` | 300,000,000 (+ the 100 MB anchor) |

Measured on the current tree (this session, `--verify full`):

| | value |
| --- | ---: |
| `operation_ns` | 8.342 ms |
| store growth | 200,704 B |
| canonical objects / bytes | 33 / 101,378 |
| `pipeline.handoff_objects` | 33 |
| receipt status | **FAIL** |

**It is failing today, and this blocks the spec's base case.** `child.stdout` reports
`PASS gates=12`, but the receipt is `FAIL` on
`g1.o3-pinned-counters: pipeline.commits 1 -> 43`. That pin is **not stale**: the golden
table (`tests/golden/expected.tsv:1312`) pins `1`, and `run-20260920T-fix1` through
`run-20260920T-fix5` all reported `PASS` with `8 of 8 pinned counters reproduced`.

**The cause is identified: the writer-budget refactor.** `pipeline.commits` moved from 1
to 43 because of `7075f338db36b209b59031e3e55ae11cf87eed57` — *"feat(core): replace the
fixed two-save model with a configured per-Store writer budget (#216)"*, 2026-09-21
08:28:48 +0800 — which is an ancestor of `HEAD` and **not** an ancestor of the closure
run's tree `66bce8378`. Its own message names the change:

> Schema 8 adds `store_policy.max_concurrent_writes` (1..=64, default 2) and widens
> `saves.active_slot` to the supported slot space (1..=64) instead of 1..=2. … The
> slot-1/slot-2 special case is gone …

and the commit-cadence consequence is visible in the code: `cas/lifecycle.rs:166-175`
now records *"every step commits before that lock is released"*, incrementing
`self.counters.commits` per step, against the old model's single acknowledgement. So
`SaveOutcome.commits` is a **step count under the writer budget**, and 43 is that
counter's value for a 33-object save on the current tree — not a regression in the
operation.

`git diff --stat 66bce8378..HEAD -- core/crates/layerfs-storage/src/` is **31 files,
+2,451 / −623**: the whole 217 lane's C2 half has moved, exactly as the Stage 6
re-check warned ("the full 217-row lane has **not** been re-run since").

**Consequence for this spec:** a parity row must not be gated on a pinned commit count
inherited from the pre-budget model. §5's pin list therefore carries
`pipeline.commits` as **report-only until the pin is regenerated from a passing run**
after the ruling in §8.3 — and the ruling must first decide whether 43 is the correct
value or whether the step-per-commit cadence is itself unwanted for a single-writer
save. That decision is the owner's, not this spec's.

## 4. The proposed row

Registered id **`pipeline-namespace-10000`**, in the existing `pipeline.*` group (which
`CONTRACT.md` §3 counts separately from the twenty families, so the group's own
cardinality changes from 4 to 5 and the lane total from 217 to 218).

| field | proposed value | source |
| --- | --- | --- |
| `family` | `pipeline.*` | existing group |
| `Shape` | new variant `Pipeline(PipelineOp::NamespaceScale)` | `src/registry.rs` |
| `cache` | `PreparedDewarmed` | as every other pipeline row |
| `store_state` | `OpenedFromCopy` | as every other pipeline row |
| `entries` | **10,000** | parity |
| `directories` | **100** | parity |
| content bytes | **300,000,000** across the 9,900 non-empty files | parity |
| largest file | **100,000,000** (the anchor) | parity |
| `smoke` | `false` | budget (§6) |

### 4.1 What the row must do inside the timer

One timed region, in this order, with the product's own telemetry root named
`pipeline_namespace`:

1. `Store::open(sample)` over a prepared, de-warmed byte copy.
2. `begin_save`.
3. Build the 10,000-entry / 100-directory namespace, with each file's
   `content_root` bound to **content that was constructed and supplied by the
   harness before the timer** — the C2 rule in `src/ops/c2.rs:1-22` ("C2 never runs C1
   file construction inside a measured phase. Every canonical object a C2 row saves is
   supplied by the harness").
4. `operation.finish()`, and record `SaveOutcome`.

**The construction-boundary decision is the one open design question, and it must be
ruled on rather than assumed.** v0.1.6's `layerstack_init_ns` *includes* construction —
it scans bytes and chunks them inside the timer. v0.1.7's C2 rule excludes it. Three
options, in the order this spec recommends them:

- **Option A (recommended): content supplied, build+save measured.** Pre-construct the
  ~300 MB into canonical objects before the timer (as `c2.rs` does), then measure
  `build_filesystem` + handoff + `finish`. This matches v0.1.6's *Store-facing* work and
  is faithful to the C2 rule. It under-measures relative to v0.1.6 by exactly the
  construction cost.
- **Option B: two rows.** One `pipeline-namespace-10000-construct` measuring
  construction, one `pipeline-namespace-10000-save` measuring the Store half. This is
  the honest decomposition — §5's attribution work says the byte path is where the cost
  is — but it is two rows and neither alone is "the same work".
- **Option C: construction inside the timer.** Faithful to v0.1.6 but violates the C2
  rule at `src/ops/c2.rs:1-22`; it would need that rule amended, which is a larger
  ruling than this spec.

A parity claim is only meaningful once the option is fixed, because the options differ
by the whole construction phase.

## 5. Gates and golden pins

The row reuses the existing pipeline gate set unchanged — `g4.residency`,
`g4.allocation-attribution`, `g7.tree-complete`, `g1.o1-replay-root`, `g2.handoff`,
`g1.o4-listing`, `g1.o2-root-readback`, `g1.o2-listing`, `g5.no-sidecars`, `g4.swaps`,
`g1.o3-pinned-counters`, `g1.o1-pinned-identity` — plus one new gate, because the point
of the row is bytes and no existing gate reads them:

| gate | limit | why it is new |
| --- | --- | --- |
| `g1.o5-content-bytes` | `SaveOutcome.inserted` canonical bytes == the declared content total, and `>= 300,000,000` | every existing gate is structural; without this a row that saved empty inodes would pass |

Golden pins to add (values are **not** written by hand — see §8.3):
`pipeline.declared_files` = 10,000, `pipeline.declared_directories` = 100,
`pipeline.bindings` = 10,100, `pipeline.handoff_objects`, `pipeline.inserted`,
`pipeline.reused`, `pipeline.commits`, `pipeline.objects_emitted`, and the new
`pipeline.content_bytes`.

`pipeline.commits` is the pin to watch, and per §3 it is **report-only until
regenerated**: the counter's meaning changed with the writer-budget refactor
(`7075f338d`), so the `1` in the table today is a value from the retired two-save
model. Pinning the new row's value before §8.3 is ruled on would freeze a number whose
definition is under review.

## 6. Budget and cache declaration

| | declaration | basis |
| --- | --- | --- |
| cache | `PreparedDewarmed`, de-warmed before the timed phase; `g4.residency` must report **0 resident pages** | as every pipeline row |
| store state | `OpenedFromCopy`, `closed-quiescent-byte-copy`, attribution `exclusive` | as every pipeline row |
| complete-command budget | **25 s, on the declared exception list** | v0.1.6's same work costs 6.4 s wall including a container; v0.1.7 has no container but must prepare and de-warm ~300 MB. It cannot fit 15 s on first estimate, so it is declared rather than discovered. |
| verification budget | 60 s, `--verify full` | a second byte-identical build and read-back over 10,000 entries |
| workers | `LAYERFS_CONSTRUCTION_WORKERS=1`, exported by `runner.py:326` | **note the tension:** `AGENTS.md` §3.8 keeps multi-worker for `init_namespace` only. A v0.1.7 row for this work runs single-worker, so it is **not** an exact re-run of v0.1.6's parallelism and any parity claim must say so. |

## 7. The matched v0.1.6 arm

The row is only useful paired. The arm is the sealed S1 receipt
(`benchmark-results/issue219/s1-ns10000-candidate-20260921T031259Z`, 944.881 ms) with
the identities recorded in
`docs/roadmap/0.1/0.1.7/evidence/issue219-s1-reproduction-20260921T031259Z/README.md`.

Three constraints on the pairing, all already established:

1. **`--source-arm baseline` cannot supply the arm** — for `init_namespace` the
   performance branch takes no arm argument (`main.rs:2936-2944`).
2. **v0.1.6-the-tag is not a distinct arm** — `git diff --stat v0.1.6..HEAD -- 'crates/*/src/**'`
   is empty, so the tag compiles the same product as `main`. The reference arm must be
   the current reference product, or a commit whose production source actually differs.
3. **The cache states differ and must not be pooled** — v0.1.6's row is
   `reused-first-sample-uncontrolled` with `cache_contract: null`; this row is
   `PreparedDewarmed` with a residency gate. The pair is therefore a comparison of two
   declared, different cache states, and the report must say so rather than implying
   one contract covers both.

## 8. What must be ruled on before implementation

1. **The construction boundary** (§4.1, Options A/B/C). Nothing else can be specified
   until this is fixed, because it decides what the number means.
2. **The contract amendment** — adding a 218th row to a frozen cardinality, or
   replacing an existing `pipeline.*` row. Replacing would retire a registered
   selection, which `AGENTS.md` forbids doing silently.
3. **The `pipeline.commits` 1 → 43 movement** (§3). The delta is now **explained** —
   `7075f338d` replaced the fixed two-save model with a configured per-Store writer
   budget and made every step commit before releasing the arbitration lock — but
   explaining it does not settle it. Two questions remain for the owner: (a) is 43 the
   correct value for a 33-object single-writer save, or is a step-per-commit cadence
   unwanted here; and (b) may the pin be regenerated? The Stage 6 re-check's rule
   applies unchanged: a pin is regenerated only through
   `shared/pin_expected.py counters --run <passing run>` + `merge`, never hand-written.
   Until then `pipeline-filesystem-build` stays FAIL, and a parity row cannot inherit
   its counters.
4. **Whether parity is even the goal.** `CONTRACT.md` forbids v0.1.7-vs-v0.1.6 claims
   for C1/C2 families. A parity row would be the first exception, so it needs an
   explicit amendment to that prohibition — including whether the resulting comparison
   is admission evidence or diagnostic.

## 8b. Implementation progress (append-only; updated as work lands)

| step | state | where |
| --- | --- | --- |
| 1. Byte plan for the 10,000-file / 300 MB fixture | **landed, 5 unit tests pass** | `core/benchmark/fs-bench-pro-storage-content/src/ops/namespace_content.rs` |
| 2. Tree whose file inodes bind to constructed content roots | not started | — |
| 3. `PipelineOp::NamespaceScale` driver (content → handoff → save) | **landed, compiles** | `src/ops/pipeline.rs` |
| 4. `g1.o5-content-bytes` gate | **landed** (in the driver) | `src/ops/pipeline.rs` |
| 5. Registry entry `pipeline-namespace-10000` | **written, run, reverted — blocked on §8e** | `src/families/pipeline.rs` |
| 6. Golden pins regenerated from a passing run | **the `pipeline.commits` pin is regenerated and verified; the new row's own pin is blocked on §8e** | `tests/golden/` |

### Why step 5 is written but not landed, and it is not a formality

The registry entry itself is a four-line change and was implemented. Registering it
breaks **four** tests, and none of them can be satisfied today:

| test | failure | can it be fixed now? |
| --- | --- | --- |
| `registry_negative::the_frozen_cardinality_array_is_what_the_registry_holds` | `ADMISSION_CASES` is `217`; the row makes 218 | yes, by amending the frozen constant |
| `registry_negative::the_lane_sizes_and_admission_split_are_the_frozen_ones` | same | yes, same amendment |
| `registry_negative::the_registry_is_clean_under_its_own_self_check` | the registry's own `admission != ADMISSION_CASES` check | yes, same amendment |
| `pinned_expectations::every_admission_case_pins_at_least_one_counter` | the new row pins nothing | **no** |

That last one is the blocker, and it is circular by construction: a pin may only be
regenerated from a **passing** run (`shared/pin_expected.py counters --run`), the new
row cannot be run to a passing state until it has pins, and its `pipeline.commits` pin
cannot be defined until §8.3 is ruled on — because `pipeline-filesystem-build` is
*already* FAILing on exactly that counter (`1 -> 43`, §3).

So §8.3 is not a formality before step 6; it is the gate on step 5 as well. The
sequence is forced: rule on `pipeline.commits` → regenerate that pin from a passing
`pipeline-filesystem-build` run → run the new row → pin it → amend `ADMISSION_CASES`
to 218 and regenerate `tests/golden/registry.tsv` from the binary's own
`--emit-registry-tsv`. Only then is the row registrable.

`tests/golden/registry.tsv` was regenerated during this attempt and **reverted**, so the
frozen table is untouched. The driver and its gate are landed and compile; the row is
simply not reachable until the ruling.

**Step 1 detail, and one declared deviation.** The plan reproduces the v0.1.6
fixture's five declared class counts (100 empty, 7,899 tiny, 1,500 small, 500
medium, 1 anchor), its single 100,000,000-byte anchor, its 10,000-file / 100-directory
shape, its tree serials (asserted equal to `fs_fixture::Recipe::prepare`'s), and its
300,000,000-byte total — `plan()` returns an error rather than a different shape if
those cannot be placed.

It does **not** reproduce v0.1.6's per-path size assignment. v0.1.6 permutes class
bands with a SHA-256 sort key; this harness has no SHA-256 dependency and `AGENTS.md`
§4 forbids adding one, so the permutation uses the harness's own `fixture::noise`
primitive instead. The arithmetic that turns band weights into sizes is the same
largest-remainder pass, but individual paths do not receive v0.1.6's sizes. That is a
declared difference in the plan, not in the measured work: the byte total, the file
count, the directory count and the anchor are identical.

A second finding from implementing it: v0.1.6's declared per-class ranges
(`1..=8`, `32..=256`, `1_024..=8_192`) are **relative weight bands, not size caps**.
Its own declared weights sum to 102,555,546 bytes and the plan is then scaled up to
the declared logical total, so a "tiny" file there is larger than 8 bytes. Sizes here
are therefore not confined to the bands either, which is faithful rather than slack.

## 8c. Worked design for steps 2-5 (recorded so the remaining work is mechanical)

Every fact below was established by reading the current tree; none is a guess. The
remaining work is transcription against this design, not further investigation.

### The load-bearing discovery

**`build_filesystem` emits metadata only.** It never reads or emits file content: in
`crates/layerfs-content/src/filesystem/update.rs`, `contents` is a
`BTreeMap<u64, ObjectId>` of **directory** roots (`contents.insert(update.parent, ...)`
at `:295`, consumed at `:323-340`), and a file's `content_root` is copied out of the
input `InodeValue` without being demanded from the reader. The `FilesystemObjects`
reader is used for inode tables and directory pages (`lookup_many`, `read_batch` in
`sorted/page.rs:259`), not for file bytes.

Consequence: a parity row **cannot** get 300 MB into the Store by building a
filesystem. The content must be constructed separately and fed to the same save
operation. This is why §4.1's Option A is the only implementable option, and why
Option C (construction inside the timer) would need a product change to
`build_filesystem`, not a harness change.

### Step 3 — the driver, concretely

`SaveHandoff` implements `FinalizedConsumer` (`cas/store.rs:788`) and `SaveOperation`
has `accept(object)` (`used by c2.rs:625`). So one operation takes both streams:

```text
untimed:  plan = namespace_content::plan(10_000, 100, seed, 300_000_000)
          content = TreeStore::new(); for each planned file with size > 0:
              construct_bytes(policy, &capacities, &fixture::noise(size, seed ^ idx),
                              &mut content, Timing::disabled(...))   // pre-timer
          create_and_save_untimed(&base, &TreeStore::new())
          de_warm = c2::prepare_sample(&base, &sample)

measured: Store::open(sample)
          operation = store.begin_save(...)
          handoff = SaveHandoff::new(&mut operation)
          counting = CountingConsumer::new(&mut handoff)
          objects = FilesystemObjects::new(&PairProvider::new(&empty, &empty), &mut counting)
          build_filesystem(&mut objects, &input, None)      // metadata
          drop(counting); drop(handoff)
          for id in content.insertion_order():              // content
              operation.accept(content.cloned_object(id)...)
          outcome = operation.finish(...)
```

`CountingConsumer` must be dropped before the content loop so `handoff` is free; the
metadata count is read from it first. `PairProvider::new(&empty, &empty)` is correct
here because the build demands no content (above).

### Step 2 — the inode binding, and its honest limit

`PreparedTree`'s inodes carry label-derived `content_root`s. Binding them to the
constructed roots means a `namespace_content`-aware variant of `Recipe::prepare` that
takes each file's constructed root instead of `content_root(profile, seed, label)`.

**It does not change the measurement** (the build never reads a file's `content_root`),
so it is a semantic improvement, not a correctness requirement. If it is skipped, the
row must say so: the Store would hold the 300 MB of content under identities that no
inode references. Recommended order: land the driver and gate first, then the binding.

### Step 4 — the gate

`g1.o5-content-bytes`: `SaveOutcome.inserted` canonical bytes `>= 300_000_000`, and
equal to `plan.total_bytes`. It reads `outcome.inserted` (already used at
`pipeline.rs` as `pipeline.inserted`) — no new plumbing. Without it, a row that saved
empty inodes passes every existing gate, which is exactly today's failure mode.

### Step 5 — the registry entry

`src/families/pipeline.rs` gains one row beside the existing four, following their
`CaseSpec` chain exactly: `Shape::Pipeline(PipelineOp::NamespaceScale)`,
`.cache(CacheState::PreparedDewarmed)`, `.store(StoreState::OpenedFromCopy)`,
`.smoke_if(false)`, and an `entry_tier(2, 10_000, "binary")` so the tier matches
`c1.fs.build-scale`'s own 10,000 tier. `PipelineOp::NamespaceScale` is added to the
enum at `src/registry.rs:360-368`, and `configuration()` in `pipeline.rs` gains its arm
with `files: 10_000, directories: 100`.

### Step 6 — the blocker, restated

`tests/golden/registry.tsv` and `tests/golden/expected.tsv` must both gain the row, and
`tests/registry_golden.rs` compares them. The registry table can be regenerated from the
binary's own `--list`. The **pins** cannot be written until §8.3 is ruled on, because
`pipeline.commits` changed meaning with `7075f338d` and the existing
`pipeline-filesystem-build` pin is already FAILing on it. A row landed with hand-written
pins would violate the rule that a pin is regenerated only from a passing run.

### Budget reality check for step 3

300 MB of constructed content held in a `TreeStore` is held as `FinalizedObject`s in a
`HashMap` — the canonical bytes plus their role envelope, so roughly the payload size
again in RSS. For calibration the existing `payload-create-500m` row peaks at **527 MB**
RSS for 500 MiB, and the host has 38 GB. The 25 s declared exception in §6 is therefore
about the *save*, not about memory, and should be re-measured rather than assumed when
the row first runs.

## 8d. Why the pin cannot be regenerated from a passing run today

`shared/pin_expected.py`'s own contract, quoted from its docstring:

> **counters** come from a named baseline run — the round-4c full lane at
> `90bbb617d`, the last tree on which all 217 admission rows passed with 0 `FAIL`.
> Pinning *that* run's numbers is what turns "every counter identical to round 4c"
> from a comparison a reader performs into a gate the row must pass.

and, in the code:

```python
if document.get("status") != "PASS":
    # A row that did not pass has no baseline number to pin: pinning a
```

So a pin has exactly two legitimate sources: **the named baseline run**, or a run
whose row **passed**. Neither is available for `pipeline.commits`:

- the named baseline is round 4c at `90bbb617d`, whose value for that counter is the
  `1` already in the table — regenerating from it reproduces `1`, not `43`;
- the current tree's `pipeline-filesystem-build` **FAILs** (`1 -> 43`, §3), and a
  FAILing row has no baseline number to pin.

Regenerating the pin from the current tree would therefore not be "regenerating from
a passing run" — it would be **re-baselining a moved counter**, which is the specific
act the Stage 6 re-check rules out until the movement is settled:

> `tests/golden/expected.tsv` is pinned to round 4c; a moved counter is a FAIL, not a
> new baseline, so the pin can only be regenerated after the delta is explained.

**The delta is explained (§3) but not settled.** Explaining it establishes *why*
`pipeline.commits` moved — `7075f338d` replaced the fixed two-save model with a
configured per-Store writer budget and made every step commit before releasing the
arbitration lock. It does **not** establish that 43 is the value the row should
henceforth pin. That is a question about whether a step-per-commit cadence is wanted
for a single-writer save, which is a product decision, not a measurement.

### The structural finding, which is not specific to this row

The three rules compose into a deadlock for **any** new admission row:

1. `pinned_expectations::every_admission_case_pins_at_least_one_counter` requires
   every admission case to pin at least one counter;
2. a pin may only come from the named baseline or from a passing row;
3. a new row cannot pass until it is registered and run, and cannot be registered
   without breaking (1).

The escape is the one §8c names — run the row unregistered, pin it from that passing
run, then register — and it is **not open here**, because the new row's own
`pipeline.commits` cannot be pinned while the *existing* pipeline row's value is
unsettled: the two rows share the counter's meaning, so pinning the new one would
freeze a definition that is under review.

This is recorded because it will recur: adding a row to this lane is not a
harness-only change while any counter's meaning is in dispute. It needs the counter
settled first, which is an owner ruling.

## 8e. The blocker the first real run found: `MAXIMUM_WALK_ENTRIES`, not pins

With §8.3's pin regenerated (§8f) the row was registered and run for the first time.
It returned `NOT_RUN` with a **product** error, not a harness one:

```text
row.not-run   product error: InvalidRecord("cycle check work limit")
```

`crates/layerfs-content/src/filesystem/limits.rs`:

```rust
pub const MAXIMUM_WALK_ENTRIES: usize = 4_096;
```

and `validate.rs:55` aliases it as `MAXIMUM_CYCLE_CHECK_ENTRIES`, so the cycle check
refuses a tree that states more than 4,096 bindings **in one operation**. A
10,000-file / 100-directory tree states 10,101.

**This is not a defect; it is the declared design.** The product's own `limits.rs`
says a tree larger than the ceiling "is reached by several operations that each stay
under the ceiling", and `c1.fs.build-scale`'s existing `namespace-10000` row already
does exactly that — its receipt reports `fs_build.operations: 3` with
`fs_build.batch_bindings_max: 4096`. `pipeline-filesystem-build` never meets the
ceiling because it builds only 1,000 files.

So the parity row **must build in batches**, and `PreparedTree::batches(budget)`
(`fs_fixture.rs:500`) is the supported route. Its own doc states the two constraints
that shape the driver: the first batch is a `build_filesystem` and states the root's
bindings plus every directory's first binding, and the remaining batches are
`update_filesystem` calls against the previous batch's root stating **new regular
files only**, so no batch restates a binding the base already has.

**What that costs the driver, and why it is the next real step.** `update_filesystem`
needs `base: Some(previous_root)` and a reader that can serve the previous batch's
objects. Inside a measured region those objects have just been handed to the
`SaveHandoff`, so the driver needs a reader that holds them. `fs.rs` solves this by
loading a **pre-computed chain** built before the timer (`run_batched_build_row` takes
`&chain` and `measure_batch`'s reader is that chain, never the live Store). The parity
row needs the same: a `TreeStore` holding every batch's objects, built untimed, used
as the reader for all batches while the batches themselves run inside the timer and
their output goes to the save.

That is a larger driver than the single-build version landed in §8b, and it is the
work item §8e defines. It is **not** blocked on any ruling.

**Consequence for the spec's own framing.** v0.1.6 performs its 10,000-file
initialization inside one `initialize_layerstack` call and splits it internally into
73 admission transactions. v0.1.7 states at most 4,096 bindings per operation and
therefore needs **3 operations** for the same tree. The two are not the same operation
count and the parity row must declare its batch structure rather than hide it — the
same way `g2.walk-ceiling` already does for `c1.fs.build-scale`.

## 8f. The `pipeline.commits` pin is regenerated and verified

Step 1 of the four: `tests/golden/expected.tsv` now pins
`pipeline-filesystem-build counter:pipeline.commits 43`, with a provenance comment
naming `7075f338d` and the mechanism, following the `pooled-lane-cold` precedent that
is already in the same header block.

Verified: `pipeline-filesystem-build` runs **PASS**, and its pin gate reports
`8 of 8 pinned counters reproduced here; 0 published by the other phase`. The other
eight pinned keys (`pipeline.bindings` 1010, `declared_files` 1000,
`declared_directories` 10, `handoff_objects` 33, `inserted` 33, `objects_emitted` 33,
`reused` 0, and the `filesystem_root` digest) all reproduced unchanged.

The 217-row lane has **not** been re-run; no other row's pins were touched.

## 9. Not claimed

- No implementation is authorized here; no harness, product or golden file was changed.
- The `pipeline-filesystem-build` FAIL in §3 is a **diagnostic observation** on one
  sample, reported with its receipt. The cause of the counter movement is attributed to
  `7075f338d` by ancestry, commit message and code (`cas/lifecycle.rs:166-175`); **the
  217 lane was not re-run**, so this is an attribution for one row, not a lane result,
  and it does not establish that any other row moved.
- The 313–320 ms `c1.fs.build-scale` figure and the 8.342 ms
  `pipeline-filesystem-build` figure are **not** throughput numbers and are not
  comparable to v0.1.6's 944.881 ms. Only §4's row, once ruled on and run, could be.

## 10. Reproduction of every number in this spec

```sh
cd core/benchmark/fs-bench-pro-storage-content
cargo +1.85.1 build --release --locked --manifest-path Cargo.toml
python3 runner.py perf --case namespace-10000              --no-build --verify full --out <fresh>
python3 runner.py perf --case pipeline-filesystem-build    --no-build --verify full --out <fresh>
```

Receipts this session: `benchmark-results/issue219/ns17-fsbuild-10000-300mb-20260921T031259Z/`,
`benchmark-results/issue219/ns17-pipeline-fsbuild-20260921T031259Z/`.
Golden pin under test: `tests/golden/expected.tsv:1311-1315`.
