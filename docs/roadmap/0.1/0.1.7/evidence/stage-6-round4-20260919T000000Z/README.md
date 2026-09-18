# Stage 6, round 4 — five work packages, one open owner question

> **Status:** Round-4 record for [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171).
> Append-only. It does not edit
> [`../stage-6-round3-20260919T000000Z/`](../stage-6-round3-20260919T000000Z/README.md)
> or [`../stage-6-round3b-20260919T000000Z/`](../stage-6-round3b-20260919T000000Z/README.md);
> those are the same round's earlier receipts at earlier trees. The harness binary
> changed, so this is a separate directory and is comparable to none of them. The
> governing assignment is
> [`../../component-decoupling/stage-6-round4-handoff-20260919.md`](../../component-decoupling/stage-6-round4-handoff-20260919.md).

## 1. What this run is

| Field | Value |
| --- | --- |
| Command | `python3 runner.py perf --lane full --out <dir>` |
| Cases | 220 registered (217 admission + 3 diagnostic), one sample per case per arm |
| Wall | **417.015 s** |
| Source commit | `80160fded3ceeb5c2bf0da438977bbc96ebe9eb2`, clean tree |
| Harness binary sha256 | `83420c11f949891a67eec203ac3393a1ad00c9a7de3eb353cc4186b6c2da91ae` |
| Product lock sha256 | `bb44c9eea06980955a3dc4b1bb45ea2365b6fda2766e0b70045c1d9f2b748991` |
| Harness lock sha256 | `f9e14b4d55dfe3b946d6706c0d48aa126c22a206f7a51bf4ef9df70f17e1447e` |
| Registry golden sha256 | `5186d2f5e8574489ae91fe538c2e039f07406e03c558981e9ccceb1a82e685da` |
| Construction workers | `1` (exported by the runner, asserted in every receipt) |
| Platform | `macOS-26.4.1-arm64-arm-64bit-Mach-O` |
| Registry self-check / golden | `PASS` / match |

| Class | Rows | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: | ---: |
| Admission (the frozen 217) | 217 | **209** | **0** | 8 |
| Diagnostic (`component.primitives`) | 3 | 0 | 0 | 3 |
| **Total registered** | **220** | **209** | **0** | **11** |

| Round | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: |
| 1 | 122 | 3 | 95 |
| 2 | 167 | 3 | 50 |
| 3 (product fixes, `1ae8dd374`) | 180 | 0 | 40 |
| 3b (plus C2-4, `3c9313e3e`) | 194 | 0 | 26 |
| **4 (`80160fded`)** | **209** | **0** | **11** |

**+15 PASS, 0 FAIL, −15 NOT_RUN.** Every one of the 23 admission rows round 3b
left open is now `PASS` except the eight that are the owner question in §7.

Verification (`runner.py verify --run <dir>`), recorded as `verify-pass.json`:

- **0 disagreements** across all 220 re-derived statuses;
- sealed call-graph **PASS** over **120** product source files, no findings;
- runtime tripwires **PASS** over **159** stores, no findings;
- verification budget `PASS` at **1.67 s** against the 60 s limit.

Calibration (`runner.py calibrate --out <dir>`), recorded as
`experiments-E1-E4-W1-W2-W4.json`: E1 `REFUTED`, E2/E3/E4 `SATISFIED`,
W1 `SATISFIED`, **W2 `SATISFIED`**, W4 `SATISFIED`. **W2 now exists** — the
declared-process-kill arm is §6.

## 2. Per-family matrix

| class | family | rows | PASS | FAIL | NOT_RUN |
| --- | --- | ---: | ---: | ---: | ---: |
| admission | `c1.construct.whole-file` | 4 | 4 | 0 | 0 |
| admission | `c1.construct.chunked` | 4 | 4 | 0 | 0 |
| admission | `c1.cdc.chunk-count` | 12 | 12 | 0 | 0 |
| admission | `c1.edit.length-preserving` | 12 | 12 | 0 | 0 |
| admission | `c1.edit.length-changing` | 32 | 32 | 0 | 0 |
| admission | `c1.transition.boundary` | 7 | 7 | 0 | 0 |
| admission | `c1.many-tiny` | 20 | 12 | 0 | 8 |
| admission | `c1.tree.construct-traverse` | 12 | 12 | 0 | 0 |
| admission | `c1.tree.namespace-mutation` | 4 | 4 | 0 | 0 |
| admission | `c1.change-locality` | 12 | 12 | 0 | 0 |
| admission | `c1.fs.build-scale` | 8 | 8 | 0 | 0 |
| admission | `c2.lifecycle` | 5 | 5 | 0 | 0 |
| admission | `c2.reuse.cross-file` | 10 | 10 | 0 | 0 |
| admission | `c2.delta.cdc-locality` | 20 | 20 | 0 | 0 |
| admission | `c2.delta.boundaries` | 21 | 21 | 0 | 0 |
| admission | `c2.reuse.workspace` | 14 | 14 | 0 | 0 |
| admission | `c2.footprint` | 6 | 6 | 0 | 0 |
| admission | `c2.delta.small-file` | 4 | 4 | 0 | 0 |
| admission | `c2.read.waves` | 4 | 4 | 0 | 0 |
| admission | `c2.pool.cold-warm` | 2 | 2 | 0 | 0 |
| admission | `pipeline.*` | 4 | 4 | 0 | 0 |
| diagnostic | `component.primitives` | 3 | 0 | 0 | 3 |
| **total** | | **220** | **209** | **0** | **11** |

## 3. WP-4 — the four walk-ceiling tiers (commit `fe3f69481`)

`namespace-{10000,100000}[-text-v1]` were `NOT_RUN` with
`InvalidRecord("cycle check work limit")`. The ceiling is `MAXIMUM_WALK_ENTRIES`
(`filesystem/limits.rs`), charged once per whole-tree walk; a `build_filesystem`
states its own bindings and the single reachability walk charges every one of them,
so 10,100 and 101,000 bindings are refused in one call. The product's own doc names
the route: a larger tree "is reached by several operations that each stay under the
ceiling".

`PreparedTree::batches(budget)` splits the **final** tree's bindings. Growing a
recipe instead does not work, and the reason is worth recording: `Recipe::files_in`
divides `entries` by the directory count, so a grown recipe rebinds serials to
different directories and every batch becomes a rebinding of an existing name —
which is exactly what makes `check_parent_aliases` walk the whole base tree. The
first version of the probe did that and failed at every size above 4,000.

The measured phase runs every batch inside one timer with a `DiscardingConsumer`.
Each batch's base is read from a fixture chain built before the timer, because an
update reads back objects it emits and a discarding consumer cannot serve them; the
fixture chain is also the oracle, so the row gates **every intermediate root**
against it rather than only the last.

| Row | complete command | operations | largest batch |
| --- | ---: | ---: | ---: |
| `namespace-10000` | 0.702 s | 3 | 4,096 bindings |
| `namespace-10000-text-v1` | 0.692 s | 3 | 4,096 |
| `namespace-100000` | 8.737 s | 26 | 4,096 |
| `namespace-100000-text-v1` | 8.759 s | 26 | 4,096 |

No tier was shrunk. `tests/walk_ceiling.rs` pins the boundary through the public
API — 4,096 bindings accepted, 4,097 refused by the walk — the batch algebra (every
binding stated exactly once, every batch at or under the ceiling), and the property
the driver depends on (an update that states new files only is served by a
non-retaining measured phase).

## 4. WP-2 — `c2.pool.cold-warm` (commit `abd7455b8`)

The two rows were `NOT_RUN` with `Unimplemented("pooled-lane")`. They are one
controlled pair: they offer the **same** measured object set, and only the Store's
pooled-index state differs. The warm row's base holds the same values under
different serials, so no measured leaf is an object-level exact hit and every one
reaches the pooled lane — a base that reused the same serials would be 512 exact
hits and would never measure the lane at all.

| reading | cold | warm |
| --- | ---: | ---: |
| leaves | 512 | 512 |
| new values | 51,200 | 0 |
| reused values | 0 | 51,200 |
| value groups written | 512 | 0 |
| commits | 19 | 2 |
| `pool_index_entries` | 51,200 | 51,200 |
| `pool_index_bytes` | 1,228,800 | 1,228,800 |
| catalogue, read from the file | 512 groups / 51,200 values | 512 groups / 51,200 values |
| object rows, read from the file | 512 | 1,024 |
| complete command | 0.293 s | 0.390 s |

The reopened index starts empty in both rows — it is re-synchronized from the
catalogue on the first pooled save after a reopen — so the separating reading is
whether the catalogue could answer the values, which is `reused_values`.

**Where this departs from the handoff, and why.** The handoff says the fixture
"needs a build large enough to produce 512 pooled leaves". `c2-families.md` §1
excludes C1 file construction from every C2 family — "C2 families accept canonical
objects from the harness directly and never run C1 file construction" — and a build
would make a pooled-metadata row measure the filesystem builder. The leaves are
built with the product's own `InodeLeaf::encode`. The frozen specification governs.

`shared/space.py` gained a `metadata_value_groups` reading, the catalogue the
specification names, so the counter is re-derived from the Store file by a reader
that did not produce it. No `COALESCE`; an absent table is `INCOMPLETE`; four new
self-checks hold that.

## 5. WP-3 — `pipeline.*` (commit `e661d9be3`)

Four rows, `NOT_RUN` with `Unimplemented("pipeline")`. They are the only rows where
the C1 and C2 halves run in one region, and
`test_setup_and_cache_discipline.md` §2.2 fixes what is inside the timer:
`update_filesystem` (or `apply_edits`) plus `Store::open` plus the save plus its
acknowledgement. C1 construction of the base is setup.

The handoff is the product's own adapter: `layerfs_storage::SaveHandoff` is
"adapter that lets C1 feed a save operation directly", so every emitted object
reaches the save on the path the product provides and the harness retains nothing
inside the heap window. `CountingConsumer` takes the count on that same path, so
`g2.handoff` compares what C1 emitted with what the save acknowledged rather than
with a number the save reports about itself. The base is read from the Store and not
from harness memory: the reader is `StoreProvider` over the sample copy.

| Row | complete command | objects handed off = inserted | O2 read-back |
| --- | ---: | ---: | --- |
| `pipeline-edits-small` | 0.030 s | 3 = 3 | spliced digest matches |
| `pipeline-edits-chunked` | 0.204 s | 7 = 7 | spliced digest matches |
| `pipeline-edits-large-to-small` | 0.022 s | 1 = 1 | 1,048,576 → 131,071 bytes across the 131,072 cutoff |
| `pipeline-filesystem-build` | 0.025 s | 33 = 33 | 11 directories read back through the Store |

Two corrections the driver forced, both recorded rather than applied silently:
`families/pipeline.rs` declared `created-in-sample` for cache and store and was
wrong — no driver existed to contradict it, and every pipeline row measures against
a base that must already be stored, so the declaration is now
`prepared-dewarmed` / `opened-from-copy` and `tests/golden/registry.tsv` carries the
four changed lines; and the `large-to-small` result lands at `cutoff - 1` bytes,
read from `ConstructionPolicy` rather than restated.

## 6. WP-1 — the phase split, and WP-5 — the kill arm (commit `10606edc1`)

### 6.1 The phase split

The five `dedup-cdc-*-500` rows were `NOT_RUN` at 28.4–29.6 s against a **15 s**
complete-command limit. Round 3's measured split: fixture construction 11.545 s,
base + de-warm 0.021 s, measured 5.021 s, oracle replay and read-back 11.985 s.
Moving only the oracle leaves ≈16.5 s; moving only the fixture leaves ≈17.0 s.
Fitting 15 s needs both, so the row runs in three invocations.

| Row | `prepare` (untimed) | `perf` (≤ 15 s) | `verify` (≤ 60 s) |
| --- | ---: | ---: | ---: |
| `dedup-cdc-overwrite-500` | 12.15 s | **5.032 s** | 12.12 s |
| `dedup-cdc-insert-500` | 12.37 s | **5.116 s** | 12.22 s |
| `dedup-cdc-delete-500` | 11.50 s | **4.919 s** | 11.71 s |
| `dedup-cdc-scattered-500` | 12.09 s | **5.265 s** | 12.66 s |
| `dedup-cdc-common-body-500` | 11.89 s | **4.894 s** | 12.06 s |

Across the whole 20-row delta family the maxima are 5.265 s performance, 12.367 s
acquisition, 12.663 s verification. No tier was shrunk and no timeout enlarged.

`--emit-input`/`--load-input` and the runner's `prepare` verb now have a producer.
`workload/artifact.rs` writes the **union** of every distinct canonical object plus
the member list, because 500 members derived from one 4 MiB base share almost every
chunk and 500 separate stores would be 500 copies of the same bytes (the artifact is
15–139 MB depending on the op). It records each object's **persisted role code**:
the older `write_to_dir`/`load_from_dir` round trip keeps only bytes and re-wraps
them as chunks, which is why a C2 replay could not use it — the same lossiness the
owner's round-5 plan for [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184)
independently records.

The verification invocation appends to the same append-only `trace.jsonl`, with
`TraceWriter::append` continuing the sequence rather than restarting it, so the
runner's `verify` re-derives each row from one record set — 0 disagreements over all
220 rows is that check. Verification has its own 60 s budget, reported separately
from the complete-command budget, as `benchmark_rules.md` §11 requires.

### 6.2 The declared-process-kill arm (W2)

The one arm of acceptance checkbox 5 that had never run. The hold is harness argv
(`--hold-save`/`--hold-at`/`--hold-ns`) on a child that has accepted its offered set
and not committed its watermark; it adds no product hook, feature flag or
fault-injection surface, and the killed child's own output is a calibration arm
rather than admission evidence.

**The first arm was REFUTED and is recorded rather than dropped.** It held at
`begin`, where the child holds write ownership but has written nothing, so the next
`begin_save` was correctly accepted. The product was right and the arm measured the
wrong state: `UninspectedState` is defined as packs present with the watermark
behind them (`cas/lifecycle.rs`, `error.rs`), which needs a save that has accepted
objects. That discarded reading is the reason the corrected arm's kill point is the
one it is.

The corrected arm, measured — the Store state read straight out of the file with
`sqlite3`, not through the product:

| arm | `retained_pack_ceiling` | highest pack id | object rows | next `begin_save` |
| --- | ---: | ---: | ---: | --- |
| killed at `accept` (SIGKILL, exit −9) | 0 | 39 | 477 | **refused** `UninspectedState { ceiling: 0, highest_pack_id: 39 }` |
| control, same save run to completion | 44 | 44 | 553 | accepted |

W1 and W4 stay `SATISFIED`.

## 7. WP-6 — the one open question

**A filesystem build runs to completion against a non-retaining consumer and an
empty provider — measured. A filesystem update does not: it demands an object it
emitted earlier in the same operation, so a `DiscardingConsumer` cannot serve it.**

The overlay is already written and proven in `src/workload/providers.rs`
(`SharedStore`/`SharedReader`) — it is what makes the unmeasured replay of those same
inputs succeed — but using it inside `measure_update` puts a retaining consumer
inside a timed phase, which `AGENTS.md` §1 and the driver contract forbid.

**The question, for the owner:** *is a retaining measured phase admissible for an
update-shaped row?* The eight `tiny-unlink` / `tiny-bulk-delete` rows close on the
answer and on nothing else. They are retained as `NOT_RUN` with their measured state
and this reason, which is what the definition of done requires and what round 3 did.

Three candidate answers and their consequences, so the ruling is a choice rather
than a discovery. The third only became visible this round, and it is the reason
the question is worth re-asking rather than inheriting:

1. **A retaining measured phase, with a declared bound.** The retaining window is
   one operation's emitted objects. For `tiny-*-500` that is bounded by the
   500-binding batch, so the heap reading would be reported as "product work plus a
   declared one-operation harness window" and the row's O(1)-memory claim would move
   to a family that can still make it. Eight rows close. This is the option the
   handoff names, and `AGENTS.md` §1's "a timed phase never retains" is absolute
   against it as written.
2. **Not admissible.** The rows stay `NOT_RUN` and acceptance checkbox 1 stays
   partial for `c1.many-tiny`, or the family is re-scoped by the owner with a new
   contract stamp.
3. **A discarding measured phase whose reader is a prepared fixture chain** — the
   shape this round already shipped for the four walk-ceiling tiers (§3), where an
   update reads back objects it emitted and a `DiscardingConsumer` cannot serve
   them. There, each batch's base is a **declared input** (`FilesystemInput::base`)
   supplied from a chain built before the timer by a byte-identical unmeasured
   replay, and the row gates **every intermediate root** against that chain. For
   `tiny-unlink` / `tiny-bulk-delete` the same construction would supply the object
   the removal reads back from a chain built by the identical unmeasured removal,
   with a `DiscardingConsumer` and the same per-step root gate.

Option 3 is the one this round cannot decide alone, because the object it supplies
is one the measured call emitted **within the same call**, not a base handed to it.
Whether a fixture may serve an operation's own in-flight read is the owner's call,
not the driver's, and the round-4 prompt reserves exactly this question. It is
recorded as an available route rather than taken.

This round does not decide it. It is recorded here as asked, and it is the only
thing standing between this round and a closed [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171).

## 8. The declared-exception list was wrong about itself again (commit `80160fded`)

`runner.py:DECLARED_EXCEPTIONS` carried `payload-random-read-500`. The registered
case is `payload-random-read-500m`. The entry matched **no case at all**, so the
500 MiB `c2.read.waves` tier that owner decision D4 explicitly puts on the ≤ 25 s
exception list has been classified against the 15 s limit since the runner was
written.

That is why round 3b's `run-full.json` records **seven** declared exceptions where
the constant holds **eight**: the list is intersected with the selection, and this
entry never intersected anything. Round 3's handoff read that seven as the list
being seven long and concluded the `dedup-cdc-*-500` rows were not declared; both
readings came from this one typo. The round-4 prompt's instruction to check the list
against the receipts rather than against a handoff's description of it is what found
it.

The status is unchanged — `payload-random-read-500m` measures 9.312 s and passes
either limit — but the declaration was wrong and the arithmetic that depended on it
was wrong with it. The durable fix is the check, not the string: `self-check` now
asserts that every declared exception names a case the binary's own registry holds,
and fails closed on any entry that names nothing.

## 9. Budgets across the whole run

The largest complete command in the run is **9.781 s**
(`store-footprint-metadata-cardinality-100000`), inside the 15 s limit. The largest
declared exception is **9.312 s** (`payload-random-read-500m`), inside 25 s. The
largest verification invocation is **12.663 s**
(`dedup-cdc-scattered-500`), inside 60 s. No row was made to fit by shrinking a
tier, relaxing a limit, inflating a timeout or adding a worker.

## 10. Acceptance checkboxes — the round-4 state

| # | Checkbox | State |
| --- | --- | --- |
| 1 | Canonical identity/profile and Store compatibility across complete inputs, edits/transitions, **trees, pooling**, supported formats | **PARTIAL.** Identity, profile, complete inputs, edits, transitions, trees and `c2.reuse.workspace` were already green; **pooling is now green** (`c2.pool.cold-warm` 2 of 2, §4). The only gap left is the eight `c1.many-tiny` rows that are the owner question in §7. |
| 2 | C1-only, C2-only save/read, bounded diagnostics, **integrated timing** on real production paths | **SATISFIED.** **`pipeline.*` is now green** (4 of 4, §5), on the product's own `SaveHandoff`, with the measured region fixed by `test_setup_and_cache_discipline.md` §2.2. |
| 3 | Existing-or-better performance where a matched baseline exists; unmatched features get correctness/resource checks, not invented ratios | **SATISFIED as withdrawn.** Owner decision D1 withdraws the comparative claim; the only matched pair (`component.primitives`, 3 cases) is registered, receipted and `NOT_RUN`. No ratio is invented anywhere in this round. |
| 4 | Total retained storage/index/pack footprint, occupancy, query/read/write/copy/hash/assembly work, simultaneous memory/disk reported | **SATISFIED.** `c2.footprint` 6 of 6, and §4 adds the pooled catalogue and index readings re-derived from the file. |
| 5 | Failure/cleanup/unknown-outcome, concurrency/visibility, zero retry/fallback/fsync/WAL, without fault injection or dependency patches | **SATISFIED.** W1 `SATISFIED`; W3 `PASS` in `lifecycle-begin-save`; W4 `SATISFIED` over 120 product source files with the runtime tripwires agreeing over **159** stores; **W2 `SATISFIED`** — the declared-process-kill arm, the one part that had never run, with its first REFUTED attempt recorded in §6.2. |
| 6 | Distinct-reuse receipt revision explicit and consumers updated together | **NOT APPLICABLE.** No receipt revision was made; historical receipts are unchanged. The round-4 additions are new fields (`phase`, `acquisition`, `verification`) on a schema that already had them as declared concepts. |
| 7 | One sample per case/arm, append-only outputs, equal declared cache state, no warm credit, no extra workers, no resource-sensitive overlap | **SATISFIED.** One sample per case per arm; every output path fresh and refused if it exists; `prepared-dewarmed` rows gate `resident_pages == 0` and `disk_read_bytes`; the measurement lock is held for every `perf` and `verify`; `LAYERFS_CONSTRUCTION_WORKERS=1` is exported by the runner and asserted in every receipt. |
| 8 | Every non-`PASS` preserved with identities and repro commands; no dropped cases, relaxed limits or best-of | **SATISFIED.** 11 non-`PASS` rows retained with their measured state and reason; the five budget overruns are closed by the phase split rather than by shrinking them; the REFUTED first kill arm and the discarded interference reading from round 3 are both on the record. |
| 9 | Affected-workspace core tests/examples, formatting, Clippy, boundary/tool checks pass individually; per-commit LOC reports complete | **SATISFIED.** §12. No aggregate gate was run and none was created. |

**Checkbox 2 and 5 moved from PARTIAL to SATISFIED this round.** Checkbox 1 remains
PARTIAL on the owner question in §7, which is why
[#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) **stays open**: the
round-4 prompt's definition of done is that every checkbox holds, and this one does
not until the owner rules.

## 11. Production LOC

**Zero** for the round. Every commit is `core/benchmark/**`, `docs/**` or
`shared/experiments.py`; `AGENTS.md` excludes benchmark harnesses and development
tools from the production count, and `core/tools/check_product_boundary.py` scans
only `core/crates/*/src` and `core/crates/*/sql`.

`python3 tools/production_loc.py`: core **19,517** / reference **65,417** /
combined **84,934** — unchanged across all five commits.

No product source was changed this round. The product seal is therefore unchanged
and the only reason this is a new evidence directory is that the **harness**
changed, which invalidates the harness identity.

## 12. Checks run in this round

| Check | Result |
| --- | --- |
| `python3 core/tools/check_product_boundary.py` | PASS — 120 production Rust/SQL files scanned |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | OK, 6 tests |
| `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | OK, 17 tests |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` | clean |
| `cargo +1.85.1 test --locked --manifest-path core/Cargo.toml` | 472 passed / 0 failed |
| `cargo +1.85.1 clippy --locked --manifest-path core/Cargo.toml --all-targets -- -D warnings` | clean |
| `cargo +1.85.1 test --locked --manifest-path $H/Cargo.toml` | 84 passed / 0 failed |
| `python3 -m unittest discover -s $H/shared -p 'test_*.py'` | OK, 106 tests |
| `python3 $H/runner.py self-check` | PASS — lock parity 46 entries / 0 mismatches, registry, declared exceptions, golden |
| `python3 tools/production_loc.py` | core 19517 / reference 65417 / combined **84934** |

`tools/preflight.sh` was not run and was not restored. No CI workflow and no
aggregate gate was created.

## 13. Files in this directory

| File | What |
| --- | --- |
| `run-full.json` | the run's tally, identity and declarations |
| `report-full.txt` | the rendered report, including every non-`PASS` row |
| `verify-pass.json` | the verification pass: 0 disagreements, both tripwire verdicts |
| `experiments-E1-E4-W1-W2-W4.json` | the calibration run, W2 included |

The raw per-case receipts are **not** in the repository: `benchmark-results/*` is
gitignored, so the run directory exists only on the machine that produced it. These
four files are the derived artifacts. The prepared artifacts under
`benchmark-results/fs-bench-pro-storage-content/prepared/` are likewise local, keyed
by case and sealed with the harness binary's sha256; an artifact whose seal names
another harness is not reused.
