# fs call round trips (#818): implementation record

> **Status:** Working record of the rollout on branch `claude/rt818-rollout-c3dbae`.
> Base `layerfs_v2_0_0` `1317ebf7ca`. Measured values replace SPEC estimates only
> where a row below says so.

## Base and citations

- The spec pinned `056e920166`. `git diff --stat 056e920166 1317ebf7ca` touches
  only `AGENTS.md`, `docs/` and `docs/lesson/` (11 files): **no production or
  test source changed**, so every file:line citation in SPEC and the track specs
  stands unchanged at `1317ebf7ca`.
- Target seeded by APFS clone from the idle baseline worktree
  `layerfs-818-rt` (`056e920166`, same `Cargo.lock`).

## Rules taken for this run (from the session that started it)

- Implement first, verify once. Per item: prediction first, `--no-run` build,
  the item's pinned tests once, drop/kill sweep for each new or changed message,
  byte identity / answers / refusal order through existing tests.
- Per milestone: three-arm benchmark once on the merged tip (SQLite, PG local,
  PG +10 ms; +2 ms where a row is over budget) and one independent review.
- **15% rule:** measured within 15% of the SPEC expectation → record and
  proceed. Beyond 15% → diagnose once from per-role counts, then fix or record
  the cause and continue. No retuned caps, windows or timeouts.
  (This replaces SPEC §6.4's "10% or 3 trips" explain-before-next-item rule
  for this run.)
- The per-item latency arm, memory soak and held-out sweeps of verification.md
  run at milestone level only, not per item (no repeated verification).

## Decisions (SPEC §9 recommendations, taken without asking)

| # | Taken | Note |
|---|---|---|
| D1 | Accept new SQLite statement pins for dedupe fixes | each listed in its item |
| D2 | Deferred: **R7 not done** | owner decision needed |
| D3 | R6 taken after M3, measured on a large-file tree | |
| D4 | No (R5 does not return the root locator) | |
| D5 | Accept windows 16 → 128 and the 128-object roots demand | |
| D6 | **H7 not done** | owner decision needed (large-file attribution scope) |
| D7 | W3 uses a Store slot | no contract change |
| D8 | W4 prepares create statements at Store open (+1 trip per open) | |
| D9 | Write floor 3 + d not attempted | |
| D10 | `PLAN_PATHS` = 1,024 for W6 | |
| D11 | `READ_WINDOW_ROWS` lever deferred | |
| D12 | I3c: Init Saves use `publish_flight` via their own flag, full sweeps | |
| D13 | I3d only if one-shot SDK processes run Init → **not taken** (recommendation is conditional; no such use selected) | |
| D14 | Over-budget +10 ms rows run at +2 ms / smaller shapes until M3 | |
| D15 | Release builds for Init timing; Init baseline re-taken in release at P0 | |
| D16 | **Keep today's refusal order:** keep the eager marker read when the kept markers are absent | no order change |
| D17 | I2a/I2b add a `first_dependent`-style check | no order change |

## Items

Per item: prediction vs measured (trips and time per arm), within 15%,
decisions, deleted tests, Production LOC. Receipts under `receipts/<item>/`.

### Phase 0

_pending_
