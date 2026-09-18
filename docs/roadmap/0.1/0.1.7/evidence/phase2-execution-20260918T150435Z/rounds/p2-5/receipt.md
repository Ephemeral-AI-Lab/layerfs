# P2-5 receipt — one presence query per wave, one pooled reader per save

> **Status:** Item receipt. Written once, from [`after/`](after/) (product commit
> `7db87bb8b`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../v8/after/`](../v8/after/), collected on `12527f477`, this item's parent; the
> nominated reference rows have their own before arm,
> [`before-references/`](before-references/), collected on the same parent tree
> (the frozen set does not carry that shape).
> **Terminal state: landed** — measured against V8's counter on a nominated row,
> with the frozen-set effect reported as it is.

## 1. The item, and the gate that had to exist first

Two halves, both from #176 Tier 0-10:

**(a) batched presence.** `Availability::validate` ran one paged presence lookup
per offered object whose references were not already known. A wave knows every
reference its offered objects carry before it offers any of them, so `flush_batch`
now seeds availability with **one** lookup for the wave (`Availability::seed`,
charged through the same counter). Availability semantics are unchanged: the seed
records what the engine answered and nothing else, and `validate` still refuses an
object whose reference the query did not find.

**(b) one pooled reader per save.** `pool_base` built a fresh `PoolReader` per
trial, re-materialising the base packs and re-paying their decode work. It now uses
the owner's own reader, which `select_pooled` already uses — with the hazard the
plan named handled rather than assumed away (§4).

**The gate had to be built first.** P2-5's stated gate was "one presence query per
wave", and no counter could see a presence query; V8 (commit `12527f477`, the
parent of this item) added `SaveOutcome::presence_queries` for exactly that. The
plan's own row named `edges`/`pages`, which count a wave's dependency edges and
locator pages — not presence lookups.

## 2. The gate — one query per wave

The frozen set cannot show this: its pipeline rows carry one such object per save,
so the count reads 1 before and after. The nominated row
`phase0client c2-references <rows>` supplies `rows` *dependent* leaves, each naming
a stored leaf the wave does not offer:

| rows | waves (512 objects per wave) | before (`v8` tree) | after |
| ---: | ---: | ---: | ---: |
| 64 | 1 | 64 | **1** |
| 512 | 1 | 512 | **1** |
| 1,024 | 2 | 1,024 | **2** |
| 4,096 | 8 | 4,096 | **8** |

Exactly one query per preparation wave, against one per object. Before arm:
[`before-references/`](before-references/) (same parent tree, same client source,
its own `commands.tsv`, `artifacts.txt` and logs).

```sh
python3 compare_arms.py rounds/v8/after rounds/p2-5/after --skip Y2,Y3,Y4
# steps compared: 37, differing: 1 -> D22 `edits.pipeline.small-to-large`
```

**One frozen row moved, in the direction that costs**: D22's `presence queries`
reads **0 → 1**. The seed asks for every reference of the wave up front, including
references the lazy path never had to ask about on that shape, so a wave whose
per-object checks all resolved from what the wave already knew now pays one query.
That is the honest price of batching, it is bounded by one per wave, and it is
recorded here rather than smoothed over. Every other counter on D22 and all 36
other steps are bit-identical; the 12 stores are byte-identical
(`pack_bytes_census.py`).

## 3. The item's tests

* `tests/metadata_pool.rs`:
  `a_pooled_reader_releases_pack_bodies_when_the_store_writes` — three groups in
  one pack; the first read retains the pack; the next two groups' extents are then
  replaced; a reader with no cache refuses the damage from the start (control); the
  warmed reader still answers from the retained body; and after `release_packs` the
  same read is refused. It is the "no reader state leaks across trials" case: the
  state that could leak is a pack body read before a write, and the release is what
  prevents it.
* `tests/cas_reuse.rs` (V8's case) keeps pinning the counter's charge and control.

## 4. The hazard the sharing introduces, and its handling

A pack body is **not** immutable while a save runs: placing a group rewrites the
pack's blob, because the directory grows and every body moves with it. A reader
shared across a save's trials would therefore decode a pack as it was before the
write and never find the group that write added. `write_pack` now releases the
reader's pack cache (`PoolReader::release_packs`) on every pack write; the decoded
value cache survives, because an ordinal's value is written once and never moves.

## 5. The eight checks (exit codes)

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 | PASS, 120 files |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 | OK |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 | OK |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 | clean |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 | **468 passed, 0 failed** (467 + the new case) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 | clean (the first run caught an unused import in the new case; removed, re-run) |
| 7 | `python3 tools/production_loc.py --files` | 0 | 19,455 core |
| 8 | `git diff --check` | 0 | clean |

Logs: [`after/checks/`](after/checks/). Parity 35/35 green and unchanged.

## 6. Production LOC

**19,414 → 19,455 (delta +41).** `layerfs-storage` 6,353 → 6,394. The plan's
estimate was +20..45.

## 7. Architecture documents (same commit)

`core/docs/architecture/10-counters.md` (the `presence_queries` paragraph V8 added
now names what the wave batch does with it) and `05-storage.md` §6.9/§6.11: the
pooled reader is the save's, its pack cache is released on every pack write, and
the reason (a written pack's bodies move) is stated where the reader is described.

## 8. Clean-tree reproduction

```sh
git archive 7db87bb8b | tar -x -C /tmp/verify-p2-5          # + this round's client
(cd …/client && cargo +1.85.1 build --release --offline --locked)
…/phase0client c2-references 64      # dependents presence_queries 1   (parent: 64)
…/phase0client c2-references 512     # 1                             (parent: 512)
…/phase0client c2-references 4096    # 8                             (parent: 4096)
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test metadata_pool
```

Falsification answers: [`verify-p2-5.md`](verify-p2-5.md).
