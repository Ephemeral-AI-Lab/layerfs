# Stage 6, round 3 — four product defects fixed, one fixture defect corrected

> **Status:** Round-3 record. Supersedes nothing; rounds 1 and 2 are the historical
> record and are not edited. The governing assignment was
> [`../../component-decoupling/stage-6-round3-handoff-20260919.md`](../../component-decoupling/stage-6-round3-handoff-20260919.md).
>
> **This round does not close [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171).**
> §7 states which acceptance checkboxes it satisfies and which it does not.

## 1. What this round ran

| Field | Value |
| --- | --- |
| Command | `python3 runner.py perf --lane full --out <dir>` |
| Cases | 220 registered (217 admission + 3 diagnostic), one sample per case per arm |
| Wall | **358.825 s** |
| Source commit | `1ae8dd374e8adfe19270e73dae5950ce3b7bcb2e`, clean tree |
| Harness binary sha256 | `32888e705b8cbccd2156ae812604d563245b890efb0a803b8564a24b4728dbe7` |
| Product lock sha256 | `bb44c9eea06980955a3dc4b1bb45ea2365b6fda2766e0b70045c1d9f2b748991` |
| Harness lock sha256 | `f9e14b4d55dfe3b946d6706c0d48aa126c22a206f7a51bf4ef9df70f17e1447e` |
| Registry golden sha256 | `3f33a2eb6e3db60fc8d619346fe70cddec4b3865ba49f6bc665d0510d60a17d0` |
| Construction workers | `1` (exported by the runner, asserted in every receipt) |
| Platform | `macOS-26.4.1-arm64-arm-64bit-Mach-O` |
| Started (UTC) | `2026-09-18T20:46:13Z` |

| Class | Rows | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: | ---: |
| Admission (the frozen 217) | 217 | **180** | **0** | 37 |
| Diagnostic (`component.primitives`) | 3 | 0 | 0 | 3 |
| **Total registered** | **220** | **180** | **0** | **40** |

| Round | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: |
| 1 | 122 | 3 | 95 |
| 2 | 167 | 3 | 50 |
| **3** | **180** | **0** | **40** |

**This round is +13 PASS, −3 FAIL, −10 NOT_RUN**, and the three `FAIL` rows round 1
and round 2 both carried are gone.

Verification (`runner.py verify --run <dir>`), recorded as `verify-pass.json`:

- **0 disagreements** across all 220 re-derived statuses;
- sealed call-graph **PASS** over **120** product source files, no findings;
- runtime tripwires **PASS** over **139** stores, no findings;
- verification budget `PASS` at **1.253 s** against the 60 s limit.

Calibration (`runner.py calibrate --out <dir>`), recorded as
`experiments-E1-E4-W1-W4.json`: E1 `REFUTED`, E2/E3/E4 `SATISFIED`, W1 `SATISFIED`,
W4 `SATISFIED`. **No W2**: the declared-process-kill arm is still not implemented
(§6), which is the one arm of acceptance checkbox 5 that has never run.

## 2. The four product defects this round fixed

Every one is a case where the product's **own source already states the behaviour
that was failing**, which is the round-2 owner ruling's test for a bug rather than
a feature. Each landed as its own commit with its own regression test, and each
regression test was confirmed to reproduce the original failure against the
previous source before the fix was kept.

| # | Commit | Defect | Rows moved |
| --- | --- | --- | ---: |
| 1 | `7d5f67cf4` | A preparation wave took its membership snapshot once, before its own writes. A seal publishes rows for a whole group, so every other member of that group, later in the same wave, missed the stale snapshot and was offered a second time — `UNIQUE constraint failed: objects.object_id`. The file's own comment states the invariant that was being broken. | 7 |
| 2 | `258a3e7ad` | `OpenPack::assembled` was documented as including the pack header and was initialised to zero, so a group that took the assembled pack up to 16 bytes past `PACK_LIMIT` was admitted and then refused by the assembly it fed — `CapacityExceeded { pack.assembled_length, limit: 262144, actual: 262147 }`. | 2 |
| 3 | `0d70fc884` | `ChildDescriptor::cumulative_logical_end` is cumulative within its own page, and `traverse` compared it against absolute range bounds. Every child of a non-root branch whose subtree began past the range was pruned, so a range read of a file with more than one branch level emitted short and refused itself — `InvalidRecord("mapping coverage")`. | 1 |
| 4 | `1ae8dd374` | Fixture defect, harness only: the C1-3 `decrease` base's window was plain noise, which under this profile holds two or three extents — the same count the zero run replacing it chunks to. The row's declared direction was a coin flip at this length. | 3 |

The three `FAIL` rows were `overwrite-fixed-64k-chunk-count-decrease-{10m,100m,500m}`.
They were not product defects: the extent count genuinely did not move
(536→536, 5417→5417, 26972→26972). Fixing the fixture moved all three to `PASS`
with a structural decrease (541→536, 5422→5417, 26976→26971), and the sibling
`preserve` and `increase` rows are unchanged.

## 3. The 37 admission rows that are not `PASS`

| Rows | Cause | Owner |
| ---: | --- | --- |
| 14 | `c2.reuse.workspace` — no driver (`Shape::Workspace` is `Unimplemented`) | WP-6 |
| 8 | `tiny-unlink` / `tiny-bulk-delete` — an update reads back what it emits, so a non-retaining measured consumer cannot serve it | owner ruling, §6 |
| 5 | `dedup-cdc-{overwrite,insert,delete,scattered,common-body}-500` — complete command 28.2–29.6 s against a **15 s** limit | WP-5, §5 |
| 4 | `namespace-{10000,100000}[-text-v1]` — `InvalidRecord("cycle check work limit")`: one build walks past `MAXIMUM_WALK_ENTRIES = 4096` | WP-7 |
| 4 | `pipeline.*` — no driver | WP-6 |
| 2 | `c2.pool.cold-warm` — no driver | WP-6 |

Round 2's list of four independent causes is now six, because the two product
causes it recorded (`CapacityExceeded`, `InvalidRecord("mapping coverage")`) and
the eight-row `UNIQUE constraint` cause are all closed.

## 4. Correction to the round-3 handoff's budget premise

The handoff states that the five `dedup-cdc-*-500` rows "measure 27.98–29.63 s
against the 25 s declared exception". **They are not on the declared-exception
list, and never were.** `runner.py:DECLARED_EXCEPTIONS` holds seven IDs
(`payload-create-{100,500}m`, `payload-create-chunked-{100,500}m`,
`overwrite-fixed-64k-chunk-count-{preserve,increase,decrease}-500m`), and round 2's
`run-full.json` records exactly those seven. Round 2's own report classifies these
rows against the 15 s limit (`budget.complete-command=NOT_RUN (30.988 s)`), and so
does this round.

That matters because it changes what "fits" means. Measured phase split for
`dedup-cdc-overwrite-500`, instrumented once for the measurement and then removed:

```text
members (fixture construction)   11.545 s
base + de-warm                    0.021 s
measured phase (c2.delta)         5.021 s
oracle replay + read-back        11.985 s
```

Moving the replay and read-back into a second unmeasured `verify`-mode invocation —
the handoff's prescription, and what `benchmark_rules.md` §6 requires — leaves
`11.5 + 5.0 ≈ 16.5 s`, still above 15 s and below 25 s. Fitting the 15 s limit also
requires moving fixture construction out of the performance invocation, which is
what §6 and owner decision D2 already ask for ("acquisition … is reported as its
own field … outside the row's admission decision"). **No work was done on this
here**: it is an architectural change to the child's phase boundary that touches
every driver, and it was not attempted rather than attempted badly. The handoff's
own note that "a longer declared exception is an owner decision" applies.

## 5. Re-verified, not assumed

The three product fixes change the product seal, so this round's run is a **new
directory** and is not comparable to round 2's. Nothing was re-run into an existing
path, no case was dropped, no limit was relaxed and no tier was shrunk.

A controlled A/B against the round-2 commit (`55dda8ba4`, built in a worktree)
confirms the fixes are not performance regressions on the rows they touch:

| Row | round-2 source | this source |
| --- | ---: | ---: |
| `dedup-cdc-overwrite-500` | 29.321 s | 29.034 s |

with identical counters (`inserted` 3665, `reused` 106357, `full_records` 3665,
`no_candidate` 2162). An earlier five-row batch that reported 30–72 s for these
rows was measured while other work was in flight; the same row re-measured alone
lands at 29.0 s. That reading is discarded and is recorded here rather than
silently dropped.

## 6. What this round did not do

- **WP-5** — the setup/performance/verification split of §4. Not attempted.
- **WP-6** — 20 admission rows with no driver (`c2.reuse.workspace` 14,
  `pipeline.*` 4, `c2.pool.cold-warm` 2). Not attempted.
- **WP-7** — the four walk-ceiling tiers. The failure is the product's own
  `MAXIMUM_WALK_ENTRIES = 4096` reached by one `build_filesystem` over 10,100 and
  101,000 bindings, not the 4,096-binding build ceiling the handoff names; the
  handoff's batched-growth route is the right shape and was not implemented.
- **WP-8** — the declared-process-kill arm. Not implemented; no child mode pauses
  inside a save, so no kill point exists.
- **§5's owner question** — whether a retaining measured phase is admissible for an
  update-shaped row — is still open and still the reason the eight
  `tiny-unlink`/`tiny-bulk-delete` rows are `NOT_RUN`.

## 7. Acceptance checkboxes — what this round does and does not satisfy

| # | Checkbox | State |
| --- | --- | --- |
| 1 | Canonical identity/profile and Store compatibility across complete inputs, edits/transitions, **trees, pooling**, supported formats | **PARTIAL.** Identity, profile, complete inputs, edits, transitions, **trees** (`c1.tree.*` 16, `c1.change-locality` 12) are gated and green. **Pooling** is `NOT_RUN` (`c2.pool.cold-warm`, no driver). |
| 2 | C1-only, C2-only save/read, bounded diagnostics, **integrated timing** on real production paths | **PARTIAL.** C1-only and C2-only run real production paths and are green. `pipeline.*` (integrated) is `NOT_RUN`. |
| 3 | Existing-or-better performance where a matched baseline exists; unmatched features get correctness/resource checks, not invented ratios | **SATISFIED as withdrawn.** Owner decision D1 withdraws the comparative claim; the only matched pair (`component.primitives`, 3 cases) is registered, receipted and `NOT_RUN`. No ratio is invented anywhere in this round. |
| 4 | Total retained storage/index/pack footprint, occupancy, query/read/write/copy/hash/assembly work, simultaneous memory/disk reported | **SATISFIED.** `c2.footprint` is **6 of 6 PASS**, including the 100k/500 MB control that round 2 recorded `NOT_RUN` on the complete-command budget. Pack accounting is gated, `COALESCE` is not used, and four axes are reported per row (time, heap, RSS, space). |
| 5 | Failure/cleanup/unknown-outcome, concurrency/visibility, zero retry/fallback/fsync/WAL, without fault injection or dependency patches | **PARTIAL, and the two designed arms pass.** W1 (`SATISFIED`): a hand-edited watermark makes `Store::open` refuse while the unperturbed control still opens. W3 (`PASS` in `lifecycle-begin-save`): a second `begin_save` on one Store fails `OwnershipUnavailable`. W4 (`SATISFIED`): a sealed call-graph scan over 120 product source files finds no `fsync`/`fdatasync`/`sync_all`/`sync_data`, no WAL mode, no retry/resend/backoff call site and no non-zero busy timeout, and the runtime tripwires agree over **139** Stores. **Not done: the declared-process-kill arm** (WP-8). |
| 6 | Distinct-reuse receipt revision explicit and consumers updated together | **NOT APPLICABLE.** No receipt revision was made; historical receipts are unchanged. |
| 7 | One sample per case/arm, append-only outputs, equal declared cache state, no warm credit, no extra workers, no resource-sensitive overlap | **SATISFIED.** One sample per case per arm; every output path fresh and refused if it exists; `prepared-dewarmed` rows gate `resident_pages == 0` and `disk_read_bytes`; the measurement lock is held for every `perf` and `verify`; `LAYERFS_CONSTRUCTION_WORKERS=1` is exported by the runner and asserted in every receipt. |
| 8 | Every non-`PASS` preserved with identities and repro commands; no dropped cases, relaxed limits or best-of | **SATISFIED.** 40 non-`PASS` rows retained with their measured state and reason; the five budget overruns are recorded with their wall times rather than shrunk. |
| 9 | Affected-workspace core tests/examples, formatting, Clippy, boundary/tool checks pass individually; per-commit LOC reports complete | **SATISFIED.** §8. No aggregate gate was run and none was created. |

**Therefore #171 is not closed by this round.** Checkboxes 1, 2 and 5 carry
`NOT_RUN` work that is stated above rather than waived. Checkbox 4 moved from
`PARTIAL` to `SATISFIED` this round.

## 8. Checks run in this round

| Check | Result |
| --- | --- |
| `python3 core/tools/check_product_boundary.py` | PASS — 120 production Rust/SQL files scanned |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | OK, 6 tests |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` | clean |
| `cargo +1.85.1 test --locked --manifest-path core/Cargo.toml` | 472 passed / 0 failed |
| `cargo +1.85.1 clippy --locked --manifest-path core/Cargo.toml --all-targets -- -D warnings` | clean |
| `cargo +1.85.1 test --locked --manifest-path $H/Cargo.toml` | 80 passed / 0 failed |
| `python3 -m unittest discover -s $H/shared -p 'test_*.py'` | OK, 102 tests |
| `python3 $H/runner.py self-check` | PASS — lock parity 46 entries, 0 mismatches, registry, golden |
| `python3 tools/production_loc.py` | core 19517 / reference 65417 / combined **84934** |

## 9. Production LOC

`84921 → 84934` across the round (**delta +13**): `+10` for the mid-wave seal fix,
`0` for the pack-header fix, `+3` for the mapping-origin fix. The C1-3 fixture
correction is `core/benchmark/**` and is outside the production guard.

Counting method and scope are recorded in each commit message and reproducible
with `python3 tools/production_loc.py`. Scope: first-party product implementation
(`core/crates/*/src` + runtime SQL, reference `crates/*/src` + runtime SQL),
comments, blanks and tests excluded. core `19504 → 19517`, reference
`65417 → 65417`.

## 10. Files in this directory

| File | What |
| --- | --- |
| `run-full.json` | the run's tally, identity and declarations |
| `report-full.txt` | the rendered report, including every non-`PASS` row |
| `verify-pass.json` | the verification pass: 0 disagreements, both tripwire verdicts |
| `experiments-E1-E4-W1-W4.json` | the calibration run: E1–E4, W1, W4, with W2 absent |

The raw per-case receipts are **not** in the repository: `benchmark-results/*` is
gitignored, so the run directory exists only on the machine that produced it.
These four files are the derived artifacts.
