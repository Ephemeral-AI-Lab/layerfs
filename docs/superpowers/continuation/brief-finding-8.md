# Retro finding 8 — blame single conversion (IMPLEMENTATION)

Role: implementer. You do NOT commit, push, open PRs, or touch
the git clone, the baseline, or the connect checkout. The
commander verifies your diff, runs the tests itself, and does
all committing, pushing, and PR work.

Workdir: the Area P product tree (re-verify the p1-p15 tip at
launch). Report: retro finding 8 (P8, low).

## Problem

`Line::text()` allocates a fresh String. The `text` helper
calls it in the budget condition, again for the byte total,
and a third time for `prefix.push`. The initial
track-selection path similarly calls it for cost and then for
`shown`. This is bounded per line and is not a dual full-file
representation, but it adds avoidable allocation and UTF-8
validation in a history-heavy loop.

Locations (re-verify at launch; line numbers from the
2026-10-29 retro report):
- `engine/crates/layerfs-store-service/src/read/blame.rs:48-63`
  (three calls adjacent at :55-58; also :144-156)
- `Line::text` copies the raw prefix into a new Vec/String at
  `engine/crates/layerfs-store-service/src/read/window.rs:29-36`

## Change — small cleanup, NO contract change

Convert once per line, reuse its length, and move that String
into the destination. Apply the same one-conversion pattern
to track selection. No public surface, storage, or behavior
change: outputs must be identical before and after.

## Tests

- Existing blame correctness tests must all keep passing
  unchanged (they are the proof of identical output).
- Add a regression test only if a behavior-neutral refactor
  can be pinned by one (e.g. an allocation-count or
  conversion-count assertion through an existing counter, if
  one exists). Do NOT add a test hook, a fake clock, or any
  production test instrumentation to obtain one — if no
  honest counter exists, say so and rely on the unchanged
  correctness tests plus review of the diff.

## Verification matrix

- `export PATH="$HOME/.cargo/bin:$PATH"`, build first
  (--no-run), explicit ≤120 s timeouts: the blame suites +
  `area_p8_reads`, `area_p1_history`, `sdk_method_golden`
  (under `umask 022`), bridge control/observed records.
- `python3 -B tools/check_product_boundary.py` (+ tool
  self-tests).
- JS neighbors only if touched; otherwise state they were
  not re-run and why.
- `cargo +1.85.1 fmt --all -- --check` and workspace clippy
  (excl. node, `-D warnings`).
- /dev/fuse and Docker do not exist here — do not touch
  mounted/real-E2E tests; disclose as NOT RUN.

## Progress + report

Append one line per suite to PROGRESS-retro-f8.md
(append-only). Final report, concise: what changed
(production / test), confirmation that outputs are identical
(which suites prove it), every suite with exact pass/fail
numbers, anything not verified and why. Do not paste whole
diffs.
