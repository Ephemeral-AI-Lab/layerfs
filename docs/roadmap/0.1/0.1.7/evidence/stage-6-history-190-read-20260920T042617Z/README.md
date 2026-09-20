# #190 read-path attribution and the pooled-leaf catalogue treatment

> Status: Research; diagnostic evidence, not release admission.

**Disposition: retain ordinal-ordered pooled-leaf resolution.** One ordering change
inside one function removes the repeated catalogue lookup a pooled leaf performed
for information it already had. Both matched cases improve by more than one
second, the saved Stores are **byte-identical** between the arms, and no format,
schema, cache, limit, worker or public API changes.

## What was measured first

A bounded aggregate instrument decomposed the filesystem provider's own elapsed
time into catalogue lookup, pack BLOB acquisition, control-area validation, group
decoding, value authentication and reconstruction. The instrument is a **separate
recorded patch** (`source/instrument-read-path-*.patch`) present in **both** arms,
not part of the retained product change.

Instrumented stride10 baseline (`baseline2`), provider elapsed 9,019,067,220 ns:

| Provider interval | ns | Share |
|---|---:|---:|
| Pack BLOB acquisition, both lanes (142,686 fetches, 12,080,963,142 B copied) | 3,928,606,618 | 43.6% |
| Metadata value-group catalogue SQL (278,927 statements) | 2,180,465,764 | 24.2% |
| Unattributed remainder | 1,305,940,532 | 14.5% |
| Object/locator SQL | 666,844,674 | 7.4% |
| Pooled value materialisation, authentication, decode, conversion | 644,700,604 | 7.1% |
| Ordinary-lane group decompression | 177,929,427 | 2.0% |
| Control-area validation | 47,134,250 | 0.5% |
| Ordinary record decode call | 21,186,780 | 0.2% |
| Canonical leaf re-encode | 17,180,051 | 0.2% |
| Physical pooled body rebuild (delta chain) | 14,238,663 | 0.2% |
| Record framing | 13,122,420 | 0.1% |
| Physical pooled body decode into rows | 1,717,437 | 0.0% |

The spans nest; the table is disjoint as stated and closes against the provider
elapsed with a 1,305,940,532 ns remainder.

**Convention.** Every operation and region figure here is summed over **all**
selected states, including the first one, because that is what
`phases-perf.operation_ns` measures and what the retained L40 table reports. An
earlier revision of this campaign's derived JSON excluded state 1; see
[SUPERSEDED-ANALYSIS.md](SUPERSEDED-ANALYSIS.md). The exclusion moved the
headline reductions by at most 4,543,959 ns and is corrected, not hidden. **Named unmeasured remainders:** the ordinary lane's internal split, the
per-wave `retained_pack_ceiling` read, the per-leaf identity hash, the `group_value`
cache lookup per row, and whatever the 1.339-second residual is. Pack bytes are
application-buffer acquisition, not physical disk traffic; exclusive codec/SQL/copy
CPU is **NOT_MEASURED**.

## The mechanism

`PoolReader::leaf_canonical_with_groups` resolved each row's covering value group
through `sqlite::pool::group_for`, memoising only the previous group. The comment
above that loop stated why it was believed cheap: *"The rows of one leaf are in
ordinal order, so the group covering a row is almost always the group that covered
the row before it."*

The measurement contradicts the premise. A pooled leaf's rows are ordered by
**serial** (`decode_pooled_body` rejects a body whose serials are not strictly
increasing); the ordinal is a separate field. So the covering group changes about
as often as the row does:

| Quantity | stride10 | stride3 |
|---|---:|---:|
| Pooled leaf resolutions | 11,268 | 41,958 |
| Leaf rows resolved | 677,234 | 2,792,755 |
| Distinct covering groups per leaf | 5.80 | 8.77 |
| Catalogue statements per leaf | 24.8 | 31.8 |
| Catalogue statements | 278,927 | 1,334,277 |
| Nanoseconds per statement | 7,817 | 7,827 |
| Catalogue interval ns | 2,180,465,764 | 10,443,893,588 |

Each distinct covering group was asked for about four times per leaf. The
treatment visits the rows in ascending **ordinal** order, so the same memo answers
one statement per distinct group, and writes each value back at its own row's
index.

## Matched results

All durations are integer nanoseconds. One sample per case per arm, baseline2
(instrument only) versus candidate2 (instrument + treatment), identical
instrumentation, fresh outputs, both under the shared global flocks.

| Selection | Baseline operation | Candidate operation | Reduction | % |
|---|---:|---:|---:|---:|
| history-stride10 | 21,905,900,168 | 19,888,424,711 | **2,017,475,457** | 9.209735 |
| history-stride3 | 56,597,343,506 | 49,501,013,748 | **7,096,329,758** | 12.538274 |

| Selection | Provider reduction | Catalogue statements | Catalogue interval reduction | Complete command reduction | Whole-invocation CPU reduction |
|---|---:|---|---:|---:|---:|
| stride10 | 1,764,710,765 | 278,927 → **65,337** | 1,595,725,442 | 2,119,574,000 | 2,089,371,000 |
| stride3 | 7,285,698,495 | 1,334,277 → **368,074** | 7,128,182,097 | 7,356,017,875 | 7,151,548,000 |

CPU is whole-invocation user + system, **not** codec or SQL CPU. The stride10
statement count is exactly the arithmetic prediction (one per distinct covering
group); the stride3 count likewise.

| Selection | Filesystem reduction | Store begin+accept+finish reduction | Content reduction |
|---|---:|---:|---:|
| stride10 | 1,779,927,793 | 210,898,622 | 593,748 |
| stride3 | 7,283,864,997 | −178,573,535 | −11,765,376 |

The Store interval also improves, because the save's own pooled dependency
resolution goes through the same function. Stride3's Store interval and content
interval are very slightly slower; the filesystem interval dominates both cases.
Partitions are reported per region; no region is inferred.

## Rejected candidate: one catalogue statement per leaf ordinal span

The first candidate replaced the memo with one statement per leaf over the leaf's
whole ordinal span. It was measured and is retained as a **rejected** treatment:

| Selection | Operation | Delta vs baseline2 | Statements | ns per statement |
|---|---:|---:|---:|---:|
| stride10 | 19,607,302,749 | **−2,298,597,419** | 11,268 | 101,010 |
| stride3 | 58,481,684,581 | **+1,884,341,075** | 41,958 | 259,906 |

A span statement's cost grows with the *width* of the leaf's ordinal span, not
with the number of groups the leaf uses. The ordinal-ordered memo has no such
term. The stride10 win was larger for the span shape; the stride3 regression makes
it unusable, and it was replaced rather than combined.

## Semantics, custody and verification

| Selection | State roots | Canonical inventory | Value-group inventory | Store bytes |
|---|---|---|---|---|
| history-stride10 | 17/17 match | match | match | **byte-identical** |
| history-stride3 | 53/53 match | match | match | **byte-identical** |

`store_sha256` is equal between the arms in both cases, so apparent and allocated
bytes are equal too: stride10 49,324,032 apparent / 50,249,728 allocated; stride3
62,152,704 / 62,152,704. The historical allocated targets are 49,344,512 and
64,024,576; stride10 misses by 905,216 B, exactly as in the retained L40 candidate,
because this treatment does not change one stored byte. **The miss is inherited,
not caused here, and is not relabelled.** Both hashes also equal the retained L40
level-1 candidate Stores exactly, so this campaign reproduces the retained #199
Store byte for byte in both selections.

Separate identity-matched verification, one run per performance identity:

| Selection | Baseline ns | Candidate ns | Target ns | Gates |
|---|---:|---:|---:|---|
| history-stride10 | 5,593,199,541 | 4,606,757,792 | 10,000,000,000 | PASS / PASS |
| history-stride3 | 19,864,253,708 | 16,126,274,334 | 20,000,000,000 | PASS / PASS |

Each arm reads back 1,083 / 3,377 sampled paths across every selected state with
zero mismatch, zero missing and zero unexpected, and the same 2,415 / 8,739 group
decodes in both arms. Coverage is **sampled, not exhaustive**. All four complete
verification commands fit the 60-second hard budget.

## Validation

- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked`: **PASS** (494 tests).
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --examples`: **PASS** (12 targets).
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings`: **PASS**.
- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check`: **PASS**.
- `python3 core/tools/check_product_boundary.py`: **PASS** (122 production files).
- `python3 -m unittest discover -s core/tools -p 'test_*.py'`: **PASS** (6 tests).
- `cargo +1.85.1 test --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked`: **PASS** (117 tests).

The inherited harness Clippy/format failures are **not** rerun and are not called
passes; they are unchanged by this commit and remain an open gap. No CI and no
retired `tools/preflight.sh`.

## Protocol, limits and what this does not establish

One sample per case/arm; no repeats, no best-of. The complete-command diagnostic
ceilings of 120 s (stride10) and 240 s (stride3) and the 60 s verification hard
budget are unchanged; measured walls were 38.1 s / 36.3 s / 73.1 s / 80.5 s. These
rows do **not** qualify under ordinary 15/25-second admission limits. Cache state
is OS/intra-chain uncontrolled: admission **INELIGIBLE**, no cold claim. Every O3
pinned-counter row remains **INCOMPLETE**. Single samples do not establish
repeatability. The unmatched historical v0.1.6 comparison remains unresolved.

## Identities and reproduction

Base `9fe8eb290072d09594cde3ba67c3bbd1963d5e56` (the #190 continuation handoff on
top of #199). Measured baseline2 SHA256
`371ee8c28edee240f3627e16935b1f259cf045f30ba63890cf1ee6caaadc4fdb`; candidate2
SHA256 `2e1fd754961ea50bcdc85ac07c9a1131c1c9745c61665638d69015414b567089`;
rejected span candidate SHA256 `9958e0d1a09e2e1b7148b1f76b4be2f5cd84c620c9022c5028ded3df38d7abe0`.
All three are dirty-source diagnostics with recorded patches, not clean-seal
admission claims. The retained source differs from candidate2 **only** by the
recorded instrumentation patch (`source/retained-vs-measured-read.diff`).

`collect_diagnostic.py ARM CASE [--mode verify]` constructs the declared command
and refuses an existing output; `analyze.py`, `write_results.py` and
`compare_stores.py` derive summaries from the retained receipts only. Corpus
manifest `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`, tip
`b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`, stride10 `range(1,158,10) ∪ {157}`.
All eight behavioral history switches unset, `LAYERFS_CONSTRUCTION_WORKERS=1`,
`LAYERFS_HISTORY_PHASES=1`, one construction worker.

Production LOC: **85,722 → 85,725 (delta +3)**; reference 65,417 and core
20,305 → 20,308.
