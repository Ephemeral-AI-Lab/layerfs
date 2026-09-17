# Phase 1 execution rounds

Append-only receipts for Phase 1 of [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178),
under [`CONTRACT.md`](CONTRACT.md).

| Round | Item | Status |
| --- | --- | --- |
| `c1-rebaseline` | C1 counter correctness + the pre-C1 Phase 1 baseline | collected (`before/`) |

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

## What each round directory holds

```text
rounds/<item>/<arm>/commands.tsv   every command, exit code and wall time
rounds/<item>/<arm>/logs/          complete stdout+stderr per command
rounds/<item>/<arm>/output/        the vehicles' own reports and trees
rounds/<item>/receipt.md           written once, never edited
rounds/<item>/verify-<item>.md     the verification subagent's only write
```
