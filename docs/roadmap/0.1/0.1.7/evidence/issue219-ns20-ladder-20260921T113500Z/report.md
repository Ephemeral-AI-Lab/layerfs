# Report — #219 round 20, step 2: the `c1.fs.build-scale` ladder, 100 → 100,000

Pre-registration: [`pre-registration.md`](pre-registration.md), written before the first rung ran. Raw
receipts under [`raw/`](raw/), including the two ns17 receipts this report reads the 10k rung against.
**Instrument, not treatment:** no product line changed, one sample per rung, one run per rung,
`--verify full`, fresh `--out` per run.

## The ladder

All four rungs `PASS`. Complete commands 0.005 s to 0.918 s, every one inside the 15 s limit, and
`namespace-100000` — which is **not** in `runner.py`'s `DECLARED_EXCEPTIONS` — is the largest at
0.918 s.

| rung | case | status | gates | bindings | `fs_build.operations` | `phases.operation_ns` | complete command | preparation | ns per binding |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 100 | `namespace-100-compact-v3` | `PASS` | 9/9 | 101 | — (single build) | 127,292 | 4,680,958 | 262,542 | **1,260** |
| 1,000 | `namespace-1000-compact-v3` | `PASS` | 9/9 | 1,010 | — (single build) | 947,250 | 8,224,709 | 1,272,459 | **938** |
| 10,000 | `namespace-10000` | `PASS` | 10/10 | 10,100 | **3** | 68,514,625 | 94,736,583 | 13,852,625 | **6,784** |
| 100,000 | `namespace-100000` | `PASS` | 10/10 | 101,000 | **25** | 665,695,208 | 918,281,125 | 142,232,958 | **6,591** |

Receipts: `ns20-L1-100-20260921T113500Z`, `ns20-L1-1000-…`, `ns20-L1-10000-…`,
`ns20-L1-100000-20260921T113500Z`, all at source `88accb7d0`, `source_dirty` false, one binary
(`97b8a5a7e71f…`) for the whole round.

**The row with no `fs_build.operations` is not missing a counter.** The 100 and 1,000 rungs take
`run_build_row` because their trees fit `WALK_CEILING`; the counter is published only by the batched
row (`src/ops/fs.rs:868`), and their `fs_build.base_records_read: 0` and `fs_build.read_waves: 0` are
the positive evidence that no chain was read. The registered counts — 1, 1, 3, 25 — hold; the first two
are simply not published as a number.

## The registered prediction, scored

| registered | outcome |
| --- | --- |
| `fs_build.operations` 1, 1, 3, 25 | **fired** — 3 and 25 read; 1 and 1 by the absence of the batched counters |
| per-binding cost at 100k within −24 % / +26 % of the 10k rung | **fired at −2.8 %** (6,591 against 6,784 ns) |
| 100k point 3.2 s, band 2.4–4.0 s | **missed low**: 0.666 s |
| 100k complete command inside 15 s | **fired**, 0.918 s |
| refuted above 4.0 s | did not fire |
| refuted if `fs_build.operations` ≠ 25 | did not fire |

The point prediction missed for a reason that is itself the finding, below: it was anchored on the 10k
rung's *old* receipts, and the 10k rung is 4.68× cheaper today than those receipts record.

## The answer the commission asked for

**The C1 build's cost per binding is flat from 10,000 to 100,000 — 6,784 → 6,591 ns, −2.8 %.**
Ten times the entries and 1.67× the declared bytes cost 9.72× the operation (68.51 ms → 665.70 ms) with
25 operations against 3. The uncharted C1 block in the pipeline row is therefore **not a
superlinearity problem**: the work the 10k rung measures transfers, and a `pipeline-namespace-100000`
row would be buying a scaling question that this ladder has already answered.

**The one bend in the ladder is not at 10k, and it is structural.** Per binding: 1,260 → 938 → **6,784**
→ 6,591. The 7.2× step is between the 1,000 and 10,000 rungs, and it is exactly where the tree stops
fitting one operation: 1,010 bindings fit `WALK_CEILING` (4,096), 10,100 do not, so the row becomes
batched — a chain to read (`fs_build.base_records_read: 6,066`), 68 read waves and per-batch ordering
backings on disk. That cost is the price of the ceiling, not of the entry count, and it is paid once
between 4,096 and 10,000 bindings rather than per entry.

## A correction to the handoff, measured

The handoff prices this ladder as "registered, pinned and cheap — **verified, not assumed**: the 10k
rungs are PASS at **0.36 s and 0.35 s**". Both receipts exist and both are real, and both were taken at
**`dbab10ae2`**, before the product changed on the exact path being measured:

| 10k rung | source | `phases.operation_ns` | ns per binding |
| --- | --- | ---: | ---: |
| `ns17-fsbuild-10000-300mb-20260921T031259Z` | `dbab10ae2` | 320,582,291 | 31,741 |
| `ns17-namespace-10000-full-20260921T031259Z` | `dbab10ae2` | 313,133,084 | 31,003 |
| `ns20-L1-10000-20260921T113500Z` | `88accb7d0` | **68,514,625** | **6,784** |

**4.68× cheaper than the first receipt and 4.57× cheaper than the second**, with every `fs_build.*`
counter byte-identical across all three — same operations (3), same bindings (10,100), same pages, same
waves, same gates. The work is the same; only the price moved.

**The attribution is by reading, and it is narrow.** The harness's own `c1.fs.build-scale` driver is
behaviourally unchanged across those five hours: `git diff dbab10ae2..HEAD -- …/src/ops/fs.rs` is three
`fn` → `pub(super) fn` visibility changes and nothing else, and the other harness changes in the window
are the `pipeline.*` row's own driver, a registry case count, and a timing field added to
`CountingConsumer`. On the product side, `build_filesystem`
(`core/crates/layerfs-content/src/filesystem/update.rs:69`) and its validation live in
`layerfs-content`, and **exactly one file in that crate changed** between `dbab10ae2` and `HEAD`:
`filesystem/validate.rs`, by `fff509f3d` and `35aaf6f0b` — round 13's memoised absence, whose own
commit message records that this row's batches "bind serials they are allocating, so those serials are
absent by construction, and every one of them cost a two-page descent in the binding loop and the
identical descent again in the walk: 27,436 of the 27,662 pages, half of them bought twice."

So the 4.68× is a product change five hours *after* the receipts the handoff quotes, on the path the
handoff quotes them for. **The handoff's "0.36 s" ceiling estimate is stale by 4.7×** and belongs with
correction 2's family: a receipt is evidence about the tree that produced it.

## What this does not say

No comparison is made against the reference harness's 47 `namespace-100000` rows (279 ms to 105.9 s,
`TARGET_MISS` in both arms, `cache_contract: null`, `verification_status: NOT_RUN`, a different harness
and different builds). The 100 and 1,000 rungs are first measurements at any commit and are reported
without a registered point prediction. The ladder says nothing directly about the pipeline row's
`span_build_ns`; the two are different rows even where they build the same shape, and the pipeline row's
own build span is reported where it belongs.

## Environment honesty

One sample per rung, no retuning, no receipt overwritten, `--verify full` on all four, every rung
`PASS`. `namespace-100000`'s input-tree master was acquired once, untimed, before its timed child
(`acquisition_wall_ns: 0` on the row because the acquisition is the runner's and is published in the
lane phases). All seven rungs and receipts ran under the per-worktree measurement lock. **Not run:** the
`-text-v1` rungs of the same family, any rung between these four, and any second sample at any rung.

Production LOC: **31683 → 31683 (delta 0)**. Method `tools/production_loc.py --root <tree>`; this step
adds documentation only.
