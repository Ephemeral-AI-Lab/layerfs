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
| 3. `PipelineOp::NamespaceScale` driver (content → handoff → save) | not started | `src/ops/pipeline.rs` |
| 4. `g1.o5-content-bytes` gate | not started | `src/gates.rs` |
| 5. Registry entry `pipeline-namespace-10000` | not started | `src/families/pipeline.rs` |
| 6. Golden pins regenerated from a passing run | not started — blocked on §8.3 | `tests/golden/` |

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
