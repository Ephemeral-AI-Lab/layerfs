# Phase 2 execution rounds

Append-only receipts for Phase 2 of [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178),
under [`CONTRACT.md`](CONTRACT.md). The frozen workload set, the probe client and
the eight checks are inherited from
[`../phase1-execution-20260918T090000Z/`](../phase1-execution-20260918T090000Z/)
— reused, not re-invented (handoff §1).

| Round | Item | Status |
| --- | --- | --- |
| `p2-0` | P2-0 — the `cas/owner.rs` responsibility split | **landed with a correction** (`ed5ab5d95`; one guard path) |
| `v5` | V5 — the statement counter | **landed with a correction** (`464807178`; counter scope) |
| `v6` | V6 — the group-decode counter | **landed** (`3e7b3db80`) |
| `v7` | V7 — save-connection cache observability | **blocked on the spill half**, profile half landed (`8f0fda297`) |
| `p2-8` | P2-8 — `append_fits` running total | **landed** (`6a4abb256`) |
| `p2-6` | P2-6 — hash the requested object once per wave | **landed** (`57c4cf3bd`) |
| `p2-7` | P2-7 — drop `copy_run` | **landed** (`b2abb6455`) |
| `p2-4` | P2-4 — decoded-group cache on the ordinary path | **landed with a correction** (`3ce5f409e`; cache scope) |
| `v8` | V8 — the presence-query counter (added: P2-5's gate did not exist) | **landed** (`12527f477`) |
| `p2-5` | P2-5 — batched presence + one reader per save | **landed** (`7db87bb8b`) |
| `p2-2` | P2-2 — multi-row INSERT | **landed** (`0a593084c`; 72, not 64) |
| `p2-1` | P2-1 — `cache_size` + `cache_spill` | **measured-and-declined** (candidate patch on disk) |
| `p2-3` | P2-3 — `locking_mode = EXCLUSIVE` | **measured-and-declined** (breaks the reader contract) |
| `closing-pass` | the completion audit | [`CLOSING-PASS.md`](CLOSING-PASS.md), arm [`rounds/closing-pass/final/`](rounds/closing-pass/final/) |

## Driver provenance (recorded, not silent)

1. **`collect.py` is a port of the Phase 1 driver**, taken verbatim from
   `../phase1-execution-20260918T090000Z/collect.py` at commit `502f2aae1`. The
   Phase 1 directory is append-only evidence and is never edited, so a Phase 2
   round that needs a row the Phase 1 driver does not have adds it here and says
   so below. The port was proved to be a port, not a re-invention, by
   re-collecting the frozen set on the pre-item tree and comparing every counter
   against the Phase 1 `after/` arms for the same tree (see the first round that
   uses it).
2. **`client/` is the same port.** The probe client links `layerfs-content` and
   `layerfs-storage` statically, so each arm rebuilds it from its own tree
   (`B2`) and records its sha256 in `artifacts.txt`. `target/` is ignored by the
   repository `.gitignore` and is never quoted as evidence.

## Driver corrections (recorded, not silent)

1. **`X3` added to `collect.py` (2026-09-18, with V5's round).** `CONTRACT.md` §2.4
   requires one labelled determinism re-run per round for the round's primary row.
   The write-path rows `D28`/`D29` had none in the Phase 0/1 sets (`X1` repeats
   `D26`, `X2` repeats `D5`), so `c2-repeat` runs `c2 8191 default` a second time
   as `X3`. Additive: no frozen row, case, parameter or printed field changes.
2. **`compare_arms.py` added (2026-09-18, with P2-0's round).** Strips every timing
   field and each arm's own output path, then compares all measurement steps.
   `elapsed` is diagnostic-grade (`CONTRACT.md` §2.4), so a before/after claim is a
   counter claim. It excludes the two build steps by name and is proven live by a
   perturbed-copy control in P2-0's receipt §2.

## What each round directory holds

```text
rounds/<item>/<arm>/commands.tsv   every command, exit code and wall time
rounds/<item>/<arm>/logs/          complete stdout+stderr per command
rounds/<item>/<arm>/output/        the vehicles' own reports and trees
rounds/<item>/<arm>/artifacts.txt  sha256 of every binary the arm ran
rounds/<item>/receipt.md           written once, never edited
rounds/<item>/verify-<item>.md     the author-verification record
```
