# Stage 6, round 2 — the WP-A test suite, five filesystem drivers, and one product fix

> **Status:** Round-2 record. Supersedes nothing; round 1
> ([`../stage-6-qualification-20260918T175400Z/`](../stage-6-qualification-20260918T175400Z/README.md))
> is the historical record and is not edited. The governing assignment for round 3
> is [`../../component-decoupling/stage-6-round3-handoff-20260919.md`](../../component-decoupling/stage-6-round3-handoff-20260919.md).

## 1. What this round ran

| Field | Value |
| --- | --- |
| Command | `python3 runner.py perf --lane full --out <dir>` |
| Cases | 220 registered (217 admission + 3 diagnostic), one sample per case per arm |
| Wall | **326.787 s** |
| Source commit | `55dda8ba406214fec05591a7e0068e495b7118ad`, clean tree |
| Harness binary sha256 | `77196b682cd5581e15e81ed593ac242b5f5c2f729ad3a5111b7738ab50e1e097` |
| Product lock sha256 | `bb44c9eea06980955a3dc4b1bb45ea2365b6fda2766e0b70045c1d9f2b748991` |
| Construction workers | `1` (exported by the runner, asserted in every receipt) |
| Platform | `macOS-26.4.1-arm64-arm-64bit-Mach-O` |
| Started (UTC) | `2026-09-18T19:32:36Z` |

| Class | Rows | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: | ---: |
| Admission (the frozen 217) | 217 | **167** | 3 | 47 |
| Diagnostic (`component.primitives`) | 3 | 0 | 0 | 3 |
| **Total registered** | **220** | **167** | **3** | **50** |

Round 1 was 122 PASS / 3 FAIL / 95 NOT_RUN. **This round is +45 PASS, −45 NOT_RUN**,
with the same three `FAIL` rows.

Verification (`runner.py verify --run <dir>`), recorded as `verify-pass.json`:

- **0 disagreements** across all 220 re-derived statuses;
- sealed call-graph **PASS** over **120** product source files, no findings;
- runtime tripwires **PASS** over **138** stores, no findings;
- verification budget `PASS` at 0.982 s against the 60 s limit.

## 2. The 47 admission rows that are not `PASS`

Four independent causes, not one. This decomposition is the round's most useful
output: round 1 reported "12 product errors" as a single bucket.

| Rows | Cause | Owner |
| ---: | --- | --- |
| 20 | `c2.reuse.workspace` 14, `c2.pool.cold-warm` 2, `pipeline.*` 4 — no driver | implementable |
| 8 | `UNIQUE constraint failed: objects.object_id` | diagnosed, §4 |
| 8 | `tiny-unlink` / `tiny-bulk-delete` — an update reads back what it emits | owner ruling, §5 |
| 4 | `dedup-cdc-{overwrite,delete,scattered,common-body}-500` — 28.1–29.6 s vs the 25 s exception | WP-D |
| 4 | `namespace-{10000,100000}[-text-v1]` — walk ceiling at the declared size | WP-B batched growth |
| 2 | `CapacityExceeded pack.assembled_length` — 262147 and 262151 vs 262144 | §4 |
| 1 | `InvalidRecord("mapping coverage")` — `overwrite-tail-4k-500m` | §4 |

The three `FAIL` rows are unchanged from round 1 and are **not** product defects:
`overwrite-fixed-64k-chunk-count-decrease-{10m,100m,500m}` do not move the extent
count (536→536, 5417→5417, 26972→26972). That is a harness fixture defect (WP-C),
recorded and not refitted.

## 3. What round 2 added

**WP-A — the test suite (commit `91d7c5d90`).** `src/gates.rs`, which the frozen
specification calls "the highest-value test target", had **zero** tests. Now:

| Target | Tests |
| --- | ---: |
| `tests/digest_vectors.rs` | 9 |
| `tests/gates_bands.rs` | 23 |
| `tests/instruments_selfcheck.rs` | 8 |
| `tests/oracle_independence.rs` | 16 |
| `tests/registry_negative.rs` | 9 |
| `tests/window_containment.rs` | 12 |
| `shared/test_{space,trace,receipt,copyladder,residency,invariants}.py` + lock parity | 102 |

`python3 -m unittest discover -s shared -p 'test_*.py'` previously collected **0**
tests; it now collects 102. Rust total across all targets is 79.

**WP-B first half (commit `6a1e95a02`).** `src/ops/fs_fixture.rs` (the recipe-to-input
builder and the only place the ordering rules are applied) and `src/ops/fs.rs` (the
O4 oracle and five drivers). Five of eight groups now run: `c1.many-tiny`,
`c1.tree.construct-traverse`, `c1.tree.namespace-mutation`, `c1.change-locality`,
`c1.fs.build-scale`. `--emit-input DIR` / `--load-input DIR` cache the prepared
input as `prepared-tree.tsv`; the derived views are recomputed on load so a loaded
artifact cannot disagree with a built one.

**One product fix (commit `55dda8ba4`).** See §4.

## 4. Product defects found, and the one fixed

The owner ruled during this round that **product bug fixes are permitted; new
features are not**. That supersedes handoff §6's "no product source change". One
fix landed.

**Fixed — stale delta pack cache.** `MutationOwner::pack_cache` maps
`pack_id → whole pack bytes` and is populated on read
(`encoding/delta/read.rs:301`). `write_pack` (`cas/placement.rs`) released the
pooled reader's cache but never dropped the delta reader's entry, so appending a
group left a stale directory. Reading an ordinal the append had added asked the
*old* directory for it, which has fewer groups, and `pack::layout::group_view`
refused with `Integrity("group ordinal")` — a correct row read through bytes that
no longer described the pack. A pack is append-only, so the cached copy is valid
only until the next append; the entry is now removed on every write.
`dedup-cross-file-mixed-10` moves to **PASS**; the three
`dedup-cross-file-identical-{10,100,500}` rows move off `group ordinal`.

**Diagnosed, not fixed — the `UNIQUE constraint failed: objects.object_id` group.**
`owner.offer` has exactly one call site (`cas/save.rs:84`) and `objects` exactly one
writer (`cas/placement.rs:156`), so a duplicate row requires `offer` to be reached
for an identity that already has one. `flush_batch` takes `by_id` **once**, at wave
start. A later object in the same wave hits the `pending_member` branch, which calls
`seal_pending`; that seals the lane's *whole* group, writing rows for every member
and clearing them. For every other identity from that group, later in the same wave,
`by_id` is now stale and `pending_member` is false, so it falls through to `offer`
and inserts a second row. The wave-local `prepared` map cannot help: it dedupes
repeated *identities*, and these are different ones. Evidence: the failure is
exactly the multi-member rows, and `dedup-cdc-insert-1` — one member, no repeats —
fails for the unrelated capacity reason instead.

**Not investigated.** `CapacityExceeded { pack.assembled_length, limit: 262144 }` at
262147 (`store-footprint-large-object-500m`) and 262151 (`dedup-cdc-insert-1`); and
`InvalidRecord("mapping coverage")` (`overwrite-tail-4k-500m`).

## 5. An open ruling

A filesystem **build** runs to completion against a non-retaining consumer and an
empty provider — measured. A filesystem **update** does not: it demands an object it
emitted earlier in the same operation, so a `DiscardingConsumer` cannot serve it.
The eight `tiny-unlink` / `tiny-bulk-delete` rows therefore close `NOT_RUN` with the
reason attributed to the driver, not the product. Making them pass is a small
harness change (use the `SharedStore`/`SharedReader` overlay already in
`workload/providers.rs` inside `measure_update`); it is not made because that puts a
retaining consumer inside a timed phase, which the binding rules forbid. **Whether a
retaining measured phase is admissible for an update-shaped row is an owner
question.**

## 6. Production LOC

`84920 → 84921` across the round (**delta +1**, the one-line cache invalidation).
Counting method and scope are recorded in each commit message and reproducible with
`python3 tools/production_loc.py`. `core/benchmark/**` is outside the production
guard, so the harness work in §3 does not contribute.

## 7. Files in this directory

| File | What |
| --- | --- |
| `run-full.json` | the run's tally, identity and declarations |
| `report-full.txt` | the rendered report, including every non-`PASS` row |
| `verify-pass.json` | the verification pass: 0 disagreements, both tripwire verdicts |

The raw per-case receipts are **not** in the repository: `benchmark-results/*` is
gitignored, so the run directory exists only on the machine that produced it. These
three files are the derived artifacts.
