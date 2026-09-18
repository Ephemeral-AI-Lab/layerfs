# Phase 2 execution rounds

Append-only receipts for Phase 2 of [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178),
under [`CONTRACT.md`](CONTRACT.md). The frozen workload set, the probe client and
the eight checks are inherited from
[`../phase1-execution-20260918T090000Z/`](../phase1-execution-20260918T090000Z/)
— reused, not re-invented (handoff §1).

| Round | Item | Status |
| --- | --- | --- |
| `p2-0` | P2-0 — the `cas/owner.rs` responsibility split | pending |
| `v5` | V5 — the statement counter | pending |
| `v6` | V6 — the group-decode counter | pending |
| `v7` | V7 — save-connection cache observability | pending |
| `p2-8` | P2-8 — `append_fits` running total | pending |
| `p2-6` | P2-6 — hash the requested object once per wave | pending |
| `p2-7` | P2-7 — drop `copy_run` | pending |
| `p2-4` | P2-4 — decoded-group cache on the ordinary path | pending |
| `p2-5` | P2-5 — batched presence + one reader per save | pending |
| `p2-2` | P2-2 — multi-row INSERT | pending |
| `p2-1` | P2-1 — `cache_size` + `cache_spill` | pending |
| `p2-3` | P2-3 — `locking_mode = EXCLUSIVE` | pending |

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

## What each round directory holds

```text
rounds/<item>/<arm>/commands.tsv   every command, exit code and wall time
rounds/<item>/<arm>/logs/          complete stdout+stderr per command
rounds/<item>/<arm>/output/        the vehicles' own reports and trees
rounds/<item>/<arm>/artifacts.txt  sha256 of every binary the arm ran
rounds/<item>/receipt.md           written once, never edited
rounds/<item>/verify-<item>.md     the author-verification record
```
