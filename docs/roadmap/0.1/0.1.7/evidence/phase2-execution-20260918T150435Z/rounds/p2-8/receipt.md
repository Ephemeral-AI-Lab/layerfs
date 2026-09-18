# P2-8 receipt — the open lane keeps its assembled length

> **Status:** Item receipt. Written once, from [`after/`](after/) (product commit
> `8dc5b582e`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../v7/after/`](../v7/after/), collected on `8f0fda297` (the V7 round commit
> `0ebdef2ec` differs from it only in documentation), this item's parent.
> **Terminal state: landed** — the work is removed, the partitions are
> bit-identical, and the boundary case pins the first-fit decision.

## 1. The item

`append_fits` decided whether a group joins the lane's open pack by calling
`assembled_length(lane, &open.groups)`, which loops over every group of the tail
summing `body_size`. Placing `g` groups into one pack was therefore O(g²) body
measurements, `g` bounded only by the lane's group count (256 for the metadata
lanes, 65,536 for Ordinary/Native). #176 Tier 0-15 (C2 half) filed it as a
running total; the plan's shape put the total "in the open-lane placement state".

* `OpenPack` gains `assembled` — header, one directory entry per group and every
  body — maintained when a group lands and reset with the tail when a closing
  assembly consumes it;
* `append_fits(lane, assembled, group_count, group)` takes that total instead of
  the slice: one `body_size` per candidate, no pass over the tail;
* `LanePlacement::retained_bytes()` reads the same total, so the owner's
  `retained_tail_bytes` is O(lanes) rather than O(groups);
* `assembled_length` stays the canonical predicate, unchanged.

## 2. The gate — partitions and emitted bytes identical

```sh
python3 compare_arms.py rounds/v7/after rounds/p2-8/after
# steps compared: 37, differing: 0

python3 pack_bytes_census.py rounds/v7/after rounds/p2-8/after
# 12 stores compared across 2 arms, identical: True
```

Every step of the frozen set - D1–D29, M1–M4, X1–X3, Y1 - is **bit-identical**
between the arms, and the stores the arms wrote agree on pack rows, total pack
bytes, object rows and the sha256 of their concatenated pack bodies:

| store | packs | pack bytes | objects |
| --- | ---: | ---: | ---: |
| `D8/fs-c2` | 2 | 10,462 | 12 |
| `D9/fs-pipeline` | 4 | 11,085 | 12 |
| … 12 stores in total, all identical | | | |

`packs_created` / `pack_appends` (printed by the `c2` rows and the `edits` save
lines) are therefore not merely equal as counters - the bytes on disk are the
same bytes.

**The removed work has no counter, and this receipt does not pretend otherwise.**
What P2-8 removes is re-measurement inside one process, visible today only as
elapsed time, which is diagnostic-grade (`CONTRACT.md` §2.4: +17.6% spread on one
binary and input). The gate the plan set for this item is identity plus the
boundary case, and that is what is measured; no wall-time claim is made.

## 3. The boundary test

`tests/pack_locator.rs`'s
`the_placement_fit_probe_agrees_with_the_canonical_assembled_length` is extended,
not re-pinned: it now recomputes the running total from `assembled_length` at
**every** probe and feeds that to the probe, so the assertion is unchanged (the
probe's answer equals the canonical predicate's) and it additionally fails if the
maintained total drifts from the canonical length. It walks three lanes -
`Ordinary`, `WholeFile` (whose directory entry width differs) and `Native` - past
each lane's group-count limit, and the boundary is probed at every step.

It cannot compile on the parent tree, which is the honest form of "the test fails
there" for a signature that did not exist:

```text
error[E0061]: this function takes 3 arguments but 4 arguments were supplied
   --> crates/layerfs-storage/tests/pack_locator.rs:358:17
note: expected `&[EncodedGroup]`, found `usize`
```

## 4. The eight checks (exit codes)

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 | PASS, 120 files |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 | OK |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 | OK |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 | clean |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 | **462 passed, 0 failed** |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 | clean |
| 7 | `python3 tools/production_loc.py --files` | 0 | 19,368 core |
| 8 | `git diff --check` | 0 | clean |

Logs: [`after/checks/`](after/checks/). Parity 35/35 green and unchanged.

## 5. Production LOC

**19,361 → 19,368 (delta +7).** `layerfs-storage` 6,278 → 6,285. The plan's
estimate was +10..25; the delta is the field, its maintenance, the fit-probe
signature and the retained-tail read, minus nothing deleted.

## 6. Architecture document (same commit)

`core/docs/architecture/13-physical-writing.md` §18.4: the placement diagram and
the complexity table now show the running total (`append_fits` **O(1)**,
`retained_bytes` O(1), `select_many` O(G)), and the paragraph that read
"**`append_fits` is not O(1)`.**" now states what changed, what stayed (the
canonical `assembled_length`), and what checks the two against each other. The
pin note records the change as #178 **P2-8**. This document was updated in the
item's own commit - the first version of that commit omitted it, and the receipt
records that the omission was corrected by amending before the round was written.

## 7. Clean-tree reproduction

```sh
git archive 8dc5b582e | tar -x -C /tmp/verify-p2-8
cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples
…/measure_edits --mode c2 --case chunked --threshold-bytes 131072 --output /tmp/p2-8
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test pack_locator
```

Falsification answers: [`verify-p2-8.md`](verify-p2-8.md).
