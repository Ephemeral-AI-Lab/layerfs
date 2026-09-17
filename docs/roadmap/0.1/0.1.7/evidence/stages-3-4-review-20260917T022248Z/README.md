# Stages 3–4 review — retained evidence

Review round `stages-3-4-review-20260917T022248Z`. Host UTC clock read
`2026-09-17T02:22:48Z` at creation. Reviewed snapshot: branch `main`, HEAD
`b29f8e4a3708a3ece9b67fb719af990bbb0b1cb5`, `git status --porcelain` empty
(clean apart from this round's own two additions).

Report: `../component-decoupling/stages-3-4-review-20260917T022248Z.md`.
> **Snapshot caveat (added 2026-09-17):** this round was taken on a clean tree at
> `b29f8e4a3`. After it was written, a delegated audit worker made **additive** changes to
> three tracked docs and added one new prompt file, and posted two GitHub comments against
> this review's no-comment brief. HEAD, product source and every `evidence/` file are
> unchanged; see report §0.1 for the exact divergence. Nothing was reverted or committed.

Earlier evidence rounds and earlier reviews are untouched.

| File | What it is |
| --- | --- |
| `loc-totals.txt` | Audited counter output for the reviewed tree, the pre-Stage-3 base snapshot `c38961f2f`, and combined totals |
| `base-files.txt`, `head-files.txt` | Per-file production LOC and physical lines for each snapshot |
| `loc-per-file.md` | Per-file before/after/delta against the file plan's recommended range with a below/within/above verdict |
| `loc-tree.md` | Actual annotated C1/C2 production tree at HEAD |
| `loc-per-commit.txt` | Every `c38961f2f..HEAD` commit: `Production LOC:` line count, the line, and whether a `Scope:`/`Method:` note is present |
| `loc-recount.txt` | Independent recount of seven first-parent pairs with the current counter |
| `line-caps.txt` | 999-line production ceiling and 200-line lib.rs/mod.rs ceiling checks |
| `findings.md` | Report §2 — findings F-1…F-22 and positive findings P-1…P-4 |
| `criteria-stage3.md`, `criteria-stage4.md` | Report §4.1 and §4.2 — criterion tables with source, evidence, status and gaps |
| `limits.md` | Report §7 — the complete supported-envelope table and the direct answers |
| `statistics-and-memory.md` | Report §6 — time/work/selection/memory statistics and the two memory conclusions |
| `checks/` | Raw stdout of the checks this review ran (fmt 1.85.1 and 1.96.0, boundary guard, both tool suites, clippy, handoff test targets, `git diff --check`) |
| `examples/` | Raw stdout of the five `measure_edits` example modes the handoff names |
| `evidence-contradiction-checks.txt` | Independent re-verification of the contradicting/non-matching evidence this review reports |
| `counter-used.py` | Byte copy of `tools/production_loc.py` as used at this HEAD |

## What was executed here, and with what result

All commands ran on the pinned clean tree; nothing else was running.

| Check | Result |
| --- | --- |
| `cargo +1.85.1 test --workspace --locked` | exit 0 — 43 targets / 273 tests, 0 failed |
| Handoff C1 target set (8 targets) | exit 0 — 56 tests passed |
| Handoff C2 target set (7 targets) | exit 0 — 64 tests passed |
| `fmt --all --check` (1.85.1 and 1.96.0) | exit 0 both |
| `clippy --workspace --all-targets -- -D warnings` | exit 0 (cached artifacts; 0.25 s) |
| `check_product_boundary.py` | PASS — 75 production Rust/SQL files scanned |
| `unittest discover -s core/tools` | 5 tests OK |
| `unittest discover -s tools -p 'test_production_loc.py'` | 17 tests OK |
| `git diff --check` | exit 0 |
| Five `measure_edits` modes | exit 0 each |

Not run, with reasons, in report §4.3: no v0.1.6 matched arm, no repeated-sample study,
no release-profile rebuild, no miri/ASan/fuzz, no 32-bit target, no host-SQLite limit probe.
