# Stage 6, round 3b — the C2-4 workspace-reuse driver

> **Status:** Round-3 closure receipt for the tree this round hands off. Append-only.
> It does not edit
> [`../stage-6-round3-20260919T000000Z/`](../stage-6-round3-20260919T000000Z/README.md),
> which is the same round's earlier receipt: that one records the four product
> defects and the fixture defect at commit `1ae8dd374`, and this one records the
> tree as it stands after the C2-4 driver landed. The harness binary changed, so
> the two are separate directories with separate `run.json` files and neither is
> comparable to the other.

## 1. What this run is

| Field | Value |
| --- | --- |
| Command | `python3 runner.py perf --lane full --out <dir>` |
| Cases | 220 registered (217 admission + 3 diagnostic), one sample per case per arm |
| Wall | **394.565 s** |
| Source commit | `3c9313e3e9373dc11e57bee9515302e266fa2727`, clean tree |
| Harness binary sha256 | `fffbf8c35e46b3d1c1c84de062afc2d4820a871c5faa2d91c15034444451c129` |
| Product lock sha256 | `bb44c9eea06980955a3dc4b1bb45ea2365b6fda2766e0b70045c1d9f2b748991` |
| Construction workers | `1` (exported by the runner, asserted in every receipt) |
| Platform | `macOS-26.4.1-arm64-arm-64bit-Mach-O` |

| Class | Rows | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: | ---: |
| Admission (the frozen 217) | 217 | **194** | **0** | 23 |
| Diagnostic (`component.primitives`) | 3 | 0 | 0 | 3 |
| **Total registered** | **220** | **194** | **0** | **26** |

| Round | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: |
| 1 | 122 | 3 | 95 |
| 2 | 167 | 3 | 50 |
| 3 (product fixes, `1ae8dd374`) | 180 | 0 | 40 |
| **3b (plus C2-4, `3c9313e3e`)** | **194** | **0** | **26** |

Verification, recorded as `verify-pass.json`: **0 disagreements** across all 220
re-derived statuses; sealed call-graph **PASS** over **120** product source files;
runtime tripwires **PASS** over **167** stores; verification budget `PASS` at
**2.136 s** against the 60 s limit.

Calibration, recorded as `experiments-E1-E4-W1-W4.json`: E1 `REFUTED`,
E2/E3/E4 `SATISFIED`, W1 `SATISFIED`, W4 `SATISFIED`. **No W2** — the
declared-process-kill arm is still not implemented.

## 2. The C2-4 driver (commit `3c9313e3e`, harness only)

The fourteen `dedup-workspace-*` rows were `NOT_RUN` with
`Unimplemented("workspace-reuse")`. All fourteen now `PASS`, inside the 15 s
complete-command limit (max 10.17 s at the 500 tier).

C2-2 and C2-4 are two halves of one declared equation. C2-2 measures reuse *inside*
one offered set against an empty Store. C2-4 measures reuse against the content the
workspace **already holds**: the Store is pre-populated with `base_files` files and
each addition is derived from the base file its ordinal maps to, so an `exact`
addition must reuse *every* accepted object rather than every duplicate after the
first. `base_files` is re-declared from the reference family rather than guessed: a
`compact-v2` row at the 1 and 10 tiers holds as many base files as the tier
declares, every other row holds 128, including the two `base128-v3` controls whose
point is that a larger base does not manufacture reuse.

| profile | tier | base files | accepted | reused | inserted | packs created |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| exact | 1 | 1 | 59 | 59 | 0 | 0 |
| exact | 10 | 10 | 565 | 565 | 0 | 0 |
| exact | 100 | 128 | 5674 | 5674 | 0 | 0 |
| exact | 500 | 128 | 28209 | 28209 | 0 | 0 |
| local | 1 | 1 | 60 | 57 | 3 | 2 |
| local | 10 | 10 | 568 | 533 | 35 | 3 |
| local | 100 | 128 | 5674 | 5307 | 367 | 15 |
| local | 500 | 128 | 28214 | 26492 | 1722 | 64 |
| unique | 1 | 1 | 59 | 0 | 59 | 6 |
| unique | 10 | 10 | 554 | 0 | 554 | 45 |
| unique | 100 | 128 | 5660 | 0 | 5660 | 436 |
| unique | 500 | 128 | 28189 | 0 | 28189 | 2176 |
| base128 | 1 | 128 | 56 | 0 | 56 | 6 |
| base128 | 10 | 128 | 571 | 0 | 571 | 45 |

The `exact` profile carries a second mechanism gate,
`g2.exact-hit-writes-nothing`: `packs_created == 0 && pack_appends == 0`. That is
the counter reading behind the specification's "exact-hit lookup, no pack read" —
an exact-hit save resolves by lookup and writes no pack material at all. **No pack
*read* is claimed**: every reuse compares the stored canonical bytes, which is a
read by construction, and a gate asserting otherwise would be false.

## 3. The 23 admission rows that are still not `PASS`

| Rows | Cause | Work package |
| ---: | --- | --- |
| 8 | `tiny-unlink` / `tiny-bulk-delete` — an update reads back what it emits, so a non-retaining measured consumer cannot serve it | owner ruling |
| 5 | `dedup-cdc-{overwrite,insert,delete,scattered,common-body}-500` — complete command 28.4–29.6 s against a **15 s** limit | WP-5 |
| 4 | `namespace-{10000,100000}[-text-v1]` — `InvalidRecord("cycle check work limit")`: one build walks past `MAXIMUM_WALK_ENTRIES = 4096` | WP-7 |
| 4 | `pipeline.*` — no driver | WP-6 |
| 2 | `c2.pool.cold-warm` — no driver | WP-6 |

## 4. Acceptance checkboxes — the round-3 state

| # | Checkbox | State |
| --- | --- | --- |
| 1 | Canonical identity/profile and Store compatibility across complete inputs, edits/transitions, **trees, pooling**, supported formats | **PARTIAL.** Identity, profile, complete inputs, edits, transitions, **trees** (`c1.tree.*` 16, `c1.change-locality` 12) and **`c2.reuse.workspace`** (14) are gated and green. **Pooling** is `NOT_RUN` (`c2.pool.cold-warm`, no driver). |
| 2 | C1-only, C2-only save/read, bounded diagnostics, **integrated timing** on real production paths | **PARTIAL.** C1-only and C2-only run real production paths and are green. `pipeline.*` (integrated) is `NOT_RUN`. |
| 3 | Existing-or-better performance where a matched baseline exists; unmatched features get correctness/resource checks, not invented ratios | **SATISFIED as withdrawn.** Owner decision D1 withdraws the comparative claim; the only matched pair (`component.primitives`, 3 cases) is registered, receipted and `NOT_RUN`. No ratio is invented anywhere in this round. |
| 4 | Total retained storage/index/pack footprint, occupancy, query/read/write/copy/hash/assembly work, simultaneous memory/disk reported | **SATISFIED.** `c2.footprint` is 6 of 6, including the 100k/500 MB control round 2 recorded `NOT_RUN` on the complete-command budget. |
| 5 | Failure/cleanup/unknown-outcome, concurrency/visibility, zero retry/fallback/fsync/WAL, without fault injection or dependency patches | **PARTIAL, and the two designed arms pass.** W1 `SATISFIED`; W3 `PASS` in `lifecycle-begin-save`; W4 `SATISFIED` over 120 product source files with the runtime tripwires agreeing over **167** stores. **Not done: the declared-process-kill arm** (WP-8). |
| 6 | Distinct-reuse receipt revision explicit and consumers updated together | **NOT APPLICABLE.** No receipt revision was made; historical receipts are unchanged. |
| 7 | One sample per case/arm, append-only outputs, equal declared cache state, no warm credit, no extra workers, no resource-sensitive overlap | **SATISFIED.** |
| 8 | Every non-`PASS` preserved with identities and repro commands; no dropped cases, relaxed limits or best-of | **SATISFIED.** 26 non-`PASS` rows retained with their measured state and reason; the five budget overruns recorded with their wall times rather than shrunk. |
| 9 | Affected-workspace core tests/examples, formatting, Clippy, boundary/tool checks pass individually; per-commit LOC reports complete | **SATISFIED.** |

**#171 is not closed.** Checkboxes 1, 2 and 5 carry `NOT_RUN` work that is stated
above rather than waived.

## 5. What the earlier round-3 directory carries

[`../stage-6-round3-20260919T000000Z/README.md`](../stage-6-round3-20260919T000000Z/README.md)
is the record for the four product defects and the C1-3 fixture defect, and it is
the place to read:

- the diagnosis and the regression test of each of the four fixes;
- the measured phase split for the five budget rows (fixture construction 11.5 s,
  measured 5.0 s, oracle 12.0 s) and the correction that those rows are **not** on
  the 25 s declared-exception list and never were;
- the controlled A/B against the round-2 commit showing the fixes are not
  performance regressions;
- WP-7's real cause (`MAXIMUM_WALK_ENTRIES = 4096`, not the 4,096-binding build
  ceiling).

## 6. Checks run in this round

| Check | Result |
| --- | --- |
| `python3 core/tools/check_product_boundary.py` | PASS — 120 production Rust/SQL files scanned |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | OK, 6 tests |
| `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | OK, 17 tests |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` | clean |
| `cargo +1.85.1 test --locked --manifest-path core/Cargo.toml` | 472 passed / 0 failed |
| `cargo +1.85.1 clippy --locked --manifest-path core/Cargo.toml --all-targets -- -D warnings` | clean |
| `cargo +1.85.1 test --locked --manifest-path $H/Cargo.toml` | 80 passed / 0 failed |
| `python3 -m unittest discover -s $H/shared -p 'test_*.py'` | OK, 102 tests |
| `python3 $H/runner.py self-check` | PASS — lock parity 46 entries, 0 mismatches, registry, golden |
| `python3 tools/production_loc.py` | core 19517 / reference 65417 / combined **84934** |

## 7. Files in this directory

| File | What |
| --- | --- |
| `run-full.json` | the run's tally, identity and declarations |
| `report-full.txt` | the rendered report, including every non-`PASS` row |
| `verify-pass.json` | the verification pass: 0 disagreements, both tripwire verdicts |
| `experiments-E1-E4-W1-W4.json` | the calibration run: E1–E4, W1, W4, with W2 absent |
