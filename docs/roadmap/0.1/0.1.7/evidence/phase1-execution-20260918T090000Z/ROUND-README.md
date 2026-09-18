# Phase 1 execution rounds

Append-only receipts for Phase 1 of [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178),
under [`CONTRACT.md`](CONTRACT.md).

| Round | Item | Status |
| --- | --- | --- |
| `c1-rebaseline` | C1 counter correctness + the pre-C1 Phase 1 baseline | collected (`before/`, `after/`) |
| `v1` | V1 — `filesystem_timing_c1` prints `counters.validation` | collected (`after/`) |
| `v2` | V2 — `edit_memory_probe` + `edit_timing_c1 --case delete/shrink` | collected (`after/`) |
| `v3` | V3 — `StoreReadCounters.opens` + the vehicle print | collected (`after/`) |
| `p1-2` | P1-2 — pooled per-operation read session | collected (`after/`) |
| `p1-1` | P1-1 — branch children in one wave (width 256) | collected (`after/`) |

## Driver corrections (recorded, not silent)

1. **`--entries 200` removed from the `measure_filesystem` invocations.** The
   first collection of this directory (`rounds/c1-rebaseline/before/`) ran the
   Phase 0 `RUNPLAN.md` §P0-3 command verbatim, including `--entries 200`, which
   that vehicle does not accept (`unexpected argument --entries`,
   `examples/measure_filesystem.rs:91`); D7–D9 exited **101**. Phase 0 recorded
   the same failure and re-ran the three commands **without** the flag
   (`../phase0-baseline-20260917T221759Z/commands.tsv` lines 20–22 failed, 40–42
   passed). `collect.py` was corrected to the successful command shape and D7–D9
   were re-collected; the failed attempts and their logs are retained beside the
   successful ones, exactly as Phase 0 retained its own.

2. **The probe client was a stale binary (found and fixed 2026-09-18, before any
   Phase 1 box was ticked).** `collect.py`'s `CLIENT` constant pointed at
   `/tmp/layerfs-phase1-target/release/phase0client`, which **no step rebuilt**;
   the build step `B2` writes `client/target/release/phase0client` instead. The
   probe client links `layerfs-content`/`layerfs-storage` statically, so the
   `/tmp` copy was frozen at the tree it was first built from. The C1 round's
   `D25`/`D26` (`order`) rows and its `D28`/`D29` (`c2`) rows were therefore
   collected through a binary that did not contain C1's own product change, and
   C1's receipt §3.2 claim that the `order` rows are unchanged is **refuted** by
   the corrected artifact: rebuilt from the same tree, `order.default` reads
   `dir_pages_read 17`/`ino_pages_read 81`/`read_waves 7` where the stale binary
   read `2`/`1`/`11`. The correction is appended to
   `rounds/c1-rebaseline/receipt.md` (§9) with both artifacts' hashes; the rows
   the fix touches are re-measured there on the parent tree with a client built
   from that tree. From this entry on, `CLIENT` is the artifact `B2` builds, each
   round records its `artifacts.txt` (sha256 per binary it ran), and a round whose
   client hash does not match the tree under test is invalid.

3. **The `v2` set added to `collect.py` (2026-09-18, with V2's commit).** V2's
   three rows are vehicle rows, not frozen-set rows: `M1` `edit_memory_probe`,
   `M2` `edit_timing_c1 --case delete`, `M3` `edit_timing_c1 --case shrink`. They
   are collected by `collect.py <round> <arm> v2` and included in `all`;
   `edit_memory_probe` also joins `artifacts.txt`. Nothing about the frozen set,
   its vehicles, parameters or printed fields changes: D27 keeps every field it
   printed (checked by diff) and gains one additive `case: default` line.

## What each round directory holds

```text
rounds/<item>/<arm>/commands.tsv   every command, exit code and wall time
rounds/<item>/<arm>/logs/          complete stdout+stderr per command
rounds/<item>/<arm>/output/        the vehicles' own reports and trees
rounds/<item>/receipt.md           written once, never edited
rounds/<item>/verify-<item>.md     the verification subagent's only write
```
