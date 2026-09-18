# verify-v3 — the connection-opens counter

> **author-verified** (single-agent Phase 1: no independent reviewer exists; the
> evidence is the reproducibility of the commands below on the named tree).
> Round: [`receipt.md`](receipt.md). Tree: `aefcd95a5`, arm [`after/`](after/).

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive aefcd95a5 \| tar -x -C /tmp/verify-v3/tree` | 0 | clean V3 tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples` | 0 | examples built from the archive |
| R3 | `measure_edits --mode pipeline --case chunked --threshold-bytes 131072 --output <fresh>` from the archive | 0 | `readback connection opens: 3`, `readback bytes: 262144`, root `ca6c30a4355c63c3…` — identical to `after/logs/D21` |
| R4 | `measure_edits --mode c2 --case small …` from the archive | 0 | `readback connection opens: 1` — identical to `after/logs/D15` |
| R5 | `cargo +1.85.1 test --offline --locked --manifest-path core/Cargo.toml -p layerfs-storage --test cas_roundtrip a_read_wave_reports` from the archive | 0 | `1 passed` |

## 2. Falsification answers

**2a — does the new test fail on the parent tree?** The counter does not exist
there: `StoreReadCounters` has no `opens` field and `StoreProvider` has no
`connection_opens`, so the test file does not compile against `426bafec5` — the
pre-item failure is the **absence of the observable**, which is the gap V3 was
commissioned to close. Stated as such rather than dressed up as a failing
assertion.

**2b — did the counter move in the predicted direction?** V3 predicts no movement
in anything else and no movement in the new counter across the arms (it is a
baseline, not an optimization): confirmed — 0 counter-bearing lines differ
between the `v2` and `v3` arms across D1–D29 and M1–M3, and the new line reads 1
on the single-wave rows and 3 on the four chunked pipeline rows, which is exactly
the wave count those rows report (`readback` waves are visible in the timing tree
of the same logs).

**2c — parity green and unchanged?** V3's test diff is
`core/crates/layerfs-storage/tests/cas_roundtrip.rs` only — not one of the seven
sealed-oracle targets. Those 34 tests are untouched; they were re-run green on the
C1 tree in [`../c1-rebaseline/verify-c1.md`](../c1-rebaseline/verify-c1.md) §1.

**2d — single-variable?** Product: `cas/store.rs` (+2 lines, one field + its
initializer) and `cas/provider.rs` (+10 lines, the accumulator and accessor).
Plus the vehicle print, the new test and the same-commit architecture-doc update.
Nothing else.

**2e — error paths.** A demand over the read ceiling is refused **before** the
connection is opened (`check_read_demand` runs first, `cas/store.rs`), so a
refused call reports no counter at all — the counter cannot claim an open that
did not happen. That ordering is pinned by the existing
`cas_roundtrip::a_read_wave_is_bounded_by_the_declared_ceiling`, which is green in
the workspace run. A missing object still returns `ObjectMissing` after a real
open, so `opens` is 1 there (the wave did open).

**2f — elapsed as a gate?** No: the gate is `opens` (a count) and, for P1-2, the
same count. Every timing figure in the round is diagnostic.

## 3. UNVERIFIED

* **`contains` opens a connection too and is not counted by `opens`** — it returns
  no counters. V3 counts only `read_batch`'s waves; if P1-2's session must cover
  `contains`, that is P1-2's design decision, not something this counter claims.
* **The c2 rows cannot discriminate**: a single-object readback is one wave either
  way. P1-2's receipt must use D21–D24 (3 waves), not the c2 rows.
* **`opens` counts calls, not distinct file descriptors.** A pooled session that
  reopens a connection for a reason other than a wave would be invisible to it.
