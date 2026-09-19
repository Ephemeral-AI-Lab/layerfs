# Ledger entry — #187 retained-history storage loss

Per `AGENTS.md` §3.6. Appended as a new file; no historical record was edited.
**Every number is `diagnostic`.** No row was run through `runner.py`, no verify phase, no golden
table. This is an investigation, not an admission campaign.

## Identities

| | |
| --- | --- |
| source commit | `66bce8378` + uncommitted working-tree changes (below) |
| corpus | `/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data` |
| corpus manifest sha256 | `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271` |
| corpus tip | `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed` |
| v0.1.6 comparison artifact | `benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite`, 49,315,840 B, sha256 `80c2b10a7ca1513228306e42063611e2b716be053023b65073d0ab67fbee50af` |
| harness identity | `core/benchmark/fs-bench-pro-storage-content`, its own Cargo workspace |
| toolchain | `cargo +1.85.1`, `zstd 1.5.7` (system CLI, diagnostics only) |
| cache contract | `CreatedInSample` for every arm; all arms byte-identical in setup |
| construction workers | `LAYERFS_CONSTRUCTION_WORKERS=1` on every run |

## Working-tree change (harness only — production LOC delta 0)

| file | delta | what |
| --- | --- | --- |
| `src/ops/history.rs` | +223 / −3 | `LAYERFS_HISTORY_ADVISORY` (declare the previous version's content root as an `OriginalBase` predecessor), `LAYERFS_HISTORY_DEPTH_LIMIT`, and publication of the `delta.*` save counters the row never read |
| `shared/space.py` | +217 | `pack_directory()` and `whole_file_records()`: the pack-directory decoder, fail-closed |
| `shared/test_space.py` | +170 | 8 new tests (30 in the module, 136 in the suite) |

`git status --porcelain` shows **no path under `core/crates/` or `crates/`**.
`python3 core/tools/check_product_boundary.py` → **PASS**, 120 files scanned.

## Reproduction

```sh
H=core/benchmark/fs-bench-pro-storage-content
cargo +1.85.1 build --release --manifest-path $H/Cargo.toml --locked
C=/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data
B=$H/target/release/fs-bench-storage-content

# arm off — the registered lane, unchanged
LAYERFS_CONSTRUCTION_WORKERS=1 $B --case history-stride10 --out /tmp/s0_off2 --corpus $C

# arm on — the faithful history model, depth-capped so the run completes
LAYERFS_CONSTRUCTION_WORKERS=1 LAYERFS_HISTORY_ADVISORY=1 LAYERFS_HISTORY_DEPTH_LIMIT=7 \
    $B --case history-stride10 --out /tmp/s0_l7 --corpus $C

# decode the pack directory (the module, not a one-off script)
python3 -c "import sys; sys.path.insert(0,'$H/shared'); import space; \
  print(space.pack_directory('/tmp/s0_l7/sample.sqlite').as_fields())"
```

## Results — every line, passing and not

| arm | gates | apparent | `delta.no_candidate` | `prefix_selected` | max chain depth |
| --- | --- | --: | --: | --: | --: |
| `off` | **PASS** | 128,864,256 | 26,847 | 18,344 | 8 (InodeLeaf only) |
| `on`, unlimited | **INCOMPLETE** — `Integrity("dependency chain depth")` | 38,912,000 (partial) | — | — | 9 (62 objects) |
| `on`, limit 4 | **PASS** | 68,669,440 | 11,972 | 33,220 | — |
| `on`, limit 7 | **PASS** | 63,737,856 | 10,878 | 34,300 | 9 (4 objects) |
| `on`, limits 8 / 10 / 12 / 16 | **ALL INCOMPLETE** | — | — | — | — |

**The `on`, unlimited arm is a FAIL and stays on disk.** The limit-7 arm PASSES but still contains
4 objects at depth 9, so its PASS is not robust — that is stated rather than smoothed over.

## The arithmetic

```
gap (apparent, both sides)   128,864,256 - 49,315,840 = 79,548,416   = 2.6130x
  cause 1  harness declared no cross-commit base          65,126,400   81.87 %
  cause 2  remaining pack-blob gap                         7,403,406    9.31 %
  cause 3  non-pack SQLite overhead                        7,018,610    8.82 %
  ------------------------------------------------------------------
  residual                                                         0
```

Bucket tables (lane bodies; framing separate), all three Stores decoded with the same module:

| bucket | v0.1.6 | `off` | `l7` |
| --- | --: | --: | --: |
| whole-file with a base | 35,904 / 285,486,222 / 16,310,397 (17.503×) | 18,344 / 82,033,173 / 17,196,162 (4.770×) | 34,300 / 271,589,818 / 16,082,951 (16.887×) |
| whole-file without | 8,237 / 62,974,073 / 22,672,881 (2.778×) | 25,804 / 266,427,536 / 93,744,892 (2.842×) | 9,848 / 76,870,891 / 28,427,582 (2.704×) |
| other lanes | 7,581 / 32,102,860 / 6,877,354 | 7,884 / 32,460,591 / 8,737,573 | 7,884 / 32,460,591 / 8,737,573 |
| framing | 196,100 | 215,664 | 212,032 |
| **total blob** | **46,056,732** | **119,894,291** | **53,460,138** |

The `off` column **reproduces the brief's §3.1 table to the byte**, including
"other lanes 8,953,237 = 8,737,573 bodies + 215,664 framing". Residual 0.

## Limits and ceilings declared

* **No timing is reported anywhere.** Eight analysis subagents shared the machine for the whole
  campaign, so every timing reading would be a warm-cache, load-contaminated number. Access cost is
  stated in **bytes per read**, never seconds.
* **No stride3 confirmation.** The guardrail says iterate on stride10 and confirm on stride3;
  stride3 was not run. **No stride3 claim is made.**
* **`history-stride1` was never run, measured or optimised.**
* **The 217-row lane is untouched**: registry, cardinality, golden table, `--lane full` (220 rows)
  and the `full` verification default (20 smoke rows) all unchanged.
* **No product source changed.** The defect below needs an owner ruling.
* **`cargo fmt --check`** is not clean on this tree and nothing was reformatted.
* **`runner.py perf`** could not be used: verify phase, golden table,
  `tests/history_declarations.rs`, lane wiring and per-lane command ceilings are not built.

## Finding that needs an owner ruling — a product defect, not patched

The faithful model is rejected with `Integrity("dependency chain depth")`.

**The bounds are consistent; this is not a bound off-by-one** (corrected by Squad E4, measured against
the product's own `delta_chains` suite: 9 passed, including a cap-2 policy storing *and reading back*
a 2-edge chain). The writer produces edges ≤ cap, the reader refuses exactly edges > cap.

**The defect is depth *measurement*:** `DepthCache::cost_of` (`select.rs:94-144`) records every level
of a walk one edge short when the walk terminates on an already-cached entry — the terminating entry is
not pushed onto `path` (exit at `:102-104`) but the depth is computed as `cost.depth + position`
at `:132-135`, with `position` starting at 0 for the element whose base the cached entry is.
**Proved by the writer's own output:** `/tmp/s0_l7`, whose policy says `whole_file_delta_max_depth = 8`,
holds **exactly 4 role-1 objects at 9 edges whose bases sit at true depth 8** — a depth-==cap base is
ineligible, so it can only have been admitted on a reported depth ≤ 7. `/tmp/base187` holds 0.

**Minimal fix (described, NOT applied): ~4 changed lines, one function, one file, no format change.**
It buys **legality, not bytes** (~2,248 B): every base at depth ≥ 8 is refused under a cap of 8, so an
unlimited declaration cannot beat the depth-7 arm. Latent until now because no producer declared
cross-commit whole-file bases; in the `off` arm the WholeFile lane never exceeds depth 1.

## Non-passing lines, reported as plainly as the passing ones

* `on` unlimited: **INCOMPLETE**.
* `on` limits 8, 10, 12, 16: **INCOMPLETE**.
* `on` limit 7: PASS **with 4 objects at depth 9** — and the PASS is **not gate-clean**: only
  `g1.o1` and `g4.swaps` ran, `g1.o3` is INCOMPLETE, **zero `g1.o2`/`g1.o4` records exist in
  either trace**, and `ops/history.rs:426-428` refuses `--phase verify` for `history.*`. **No
  read-back ever ran, so the 4 unreadable objects were never touched.** The size reading stands; the
  PASS does not cover readability.
* The **advisory list is not globally empty** (corrected by E1): `filesystem/sorted/page.rs:380-390`
  is a live producer and the sole source of the **917 cross-save InodeLeaf bases**. Empty only on the
  file-content path the driver uses.
* **An earlier draft attributed 4,315,905 B to the depth fix. That was wrong** (E4) and the figure is
  removed from the proposal, which now rests on base *selection* (B2's R4).
* The 1.2400× delta-encoder residual (2,894,080 B) is **unexplained**; its mechanism is a hypothesis.
* Squad B2's Q5 (depth-cap sweep) and B3's per-extension/LDM sweeps: **NOT RUN**, time-boxed.
* No stride3 confirmation exists.

## Closing state of the campaign

All eight squads reported (B2, B3, B4 as time-boxed partials that say so themselves).
The synthesis and the proposal are in `squad-c/SYNTHESIS.md`.

**Bottom line.** 81.9 % of the 2.65× is a harness modelling gap, 9.3 % is remaining delta coverage
and 8.8 % is SQLite row overhead — residual 0. The registered lane's number is therefore **not a
product measurement**. Two product changes — cross-path-first base selection (B2, measured against
the product's own byte-exact encoder: whole-file lane 44,510,533 → 34,839,722 B) and a lean row
grammar (−7,018,610 B) — land the Store at **47,048,435 B apparent, 0.954× v0.1.6**. Two blind
squads independently identified the schema as the deciding term (B4's sensitivity #1; B1's
"0.995×–1.051× of the gate"). Neither product change is made here: both need an owner ruling.
