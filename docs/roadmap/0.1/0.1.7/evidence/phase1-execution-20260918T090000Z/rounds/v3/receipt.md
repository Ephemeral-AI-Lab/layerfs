# V3 receipt — the connection-opens counter

> **Status:** Prerequisite receipt (V3 of the Phase 1 plan §1). This is the only
> prerequisite with a product change: telemetry only, no algorithm, query, pack or
> byte. Written once, from [`after/`](after/) (commit `aefcd95a5`) under
> [`../../CONTRACT.md`](../../CONTRACT.md). V3 has no #178 checkbox of its own; it
> is the before-value P1-2's box is measured against.

## 1. The gap it closes

P1-2's whole claim is that a read operation's connection lifetime drops from
`O(waves)` to `O(1)`. Until V3 nothing reported it: `Store::read_batch` opened a
connection per wave (`cas/store.rs:211`), `contains` opened one too (`:247`), and
`StoreReadCounters` carried objects, packs, pages, ceiling, edges, depth and
bytes — no open count. Phase 0's `NOT_EXPOSED` list did not name it; P1-2's own
plan section did ("connection opens per operation (add a counter)").

## 2. The change

| # | Site | Statement |
| --- | --- | --- |
| 1 | `cas/store.rs` `StoreReadCounters` | new field `opens: u64`, doc'd as *this read's* connections; `read_batch` reports `opens: 1` |
| 2 | `cas/provider.rs` `StoreProvider` | `Cell<u64>` accumulator; `read_wave` adds each wave's `opens`; new `pub fn connection_opens(&self) -> u64` |
| 3 | `examples/measure_edits.rs` | `--mode c2` and `--mode pipeline` print `readback connection opens: N` (additive line) |
| 4 | `core/docs/architecture/10-counters.md` | inventory row + the §15.6 misreading + the pin line (same commit) |

The cell makes `StoreProvider` `!Sync`. Nothing in the workspace required it to be
`Sync` (checked: no `Sync`/`Send` assertion over the provider anywhere under
`core/crates`), and the shape is the honest one — one operation's adapter. P1-2
takes the same ownership when it puts a session behind it.

## 3. The before-values on the frozen set (arm `after/`)

Identities: clean tree, release, `+1.85.1`, `--locked`, one worker, one sample;
`after/artifacts.txt` records the binaries. The counter is printed by
`measure_edits`' readback rows:

| Row | workload | `readback connection opens` | waves |
| --- | --- | ---: | ---: |
| D15 | `edits.c2.small` | **1** | 1 |
| D16 | `edits.c2.chunked` | **1** | 1 |
| D17–D19 | `edits.c2.*` | **1** | 1 |
| D20 | `edits.pipeline.small` | **1** | 1 |
| **D21** | `edits.pipeline.chunked` | **3** | 3 |
| **D22** | `edits.pipeline.small-to-large` | **3** | 3 |
| **D23** | `edits.pipeline.large-to-small` | **3** | 3 |
| **D24** | `edits.pipeline.batch` | **3** | 3 |

The four pipeline rows with a chunked base are P1-2's gate: **3 opens for one
readback**, one per wave. The c2 rows read a single object and cannot discriminate
(1 either way); the `small` pipeline row likewise. That is stated plainly rather
than presented as four independent confirmations of the same shape.

## 4. Nothing else moved

Between the `v2` arm (before V3) and this arm (after V3), **0 counter-bearing
lines differ** across D1–D29 and M1–M3 (diff after removing timing figures, the
paths, the additive `case:` line and the new `readback connection opens:` line).
The product change is telemetry: no root, object count, page, row or byte moved.

## 5. The counter, pinned

`cas_roundtrip::a_read_wave_reports_the_connection_it_opened` (new):

* one `read_batch` call reports `opens == 1`, and a **grouped demand of 8 ids is
  still one call and one connection** — the equality the Store's own API keeps;
* through the provider: `0` before any wave, then `1..=2` after two waves.

The second assertion is deliberately a **bound, not an equality**: today two waves
are two connections, and P1-2 is authorized to pool them into one, so an equality
would have to be re-pinned by that item — which the handoff forbids outside the
two pre-authorized pins. The bound is what must hold on both sides of P1-2. The
counter did not exist on the parent tree, so the pre-item failure is the absence
of the observable, not a failing assertion.

## 6. The eight checks (exit codes)

| # | Command | Exit |
| --- | --- | ---: |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 (116 files) |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 (6 OK) |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 (17 OK) |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 (**437 passed, 0 failed**) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 |
| 7 | `python3 tools/production_loc.py --files` | 0 |
| 8 | `git diff --check` | 0 |

Sealed-oracle parity set: green and unchanged — V3 touches no test in that set
(`git diff --stat 426bafec5..aefcd95a5 -- '*tests*'` is `cas_roundtrip.rs` only,
which is not one of the seven targets).

## 7. Production LOC

`V3 | +10..+20` was the plan's estimate; the actual is **+12**
(`cas/provider.rs` 63 → 73, `cas/store.rs` 385 → 387).
`core` **18,793 → 18,805 (delta +12)**; `crates/` reference 65,417 → 65,417;
combined 84,210 → 84,222. Method: `tools/production_loc.py` over the first parent
(`426bafec5`, via `git archive`) and the staged tree.

## 8. Acceptance

- [x] The observable P1-2 moves now exists and is printed by a frozen vehicle
- [x] Before-values on the frozen set, with the discriminating rows named (D21–D24: 3 opens)
- [x] No other counter moved (0-line diff across D1–D29 and M1–M3)
- [x] The counter is pinned per wave; the pooled figure is a bound so P1-2 does not re-pin a test
- [x] Architecture doc updated in the same commit; eight checks green; LOC disclosed
