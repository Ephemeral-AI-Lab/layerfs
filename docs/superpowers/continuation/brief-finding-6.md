# Retro finding 6 — fs.read early-stop predicate (IMPLEMENTATION)

Role: implementer. You do NOT commit, push, open PRs, or touch
the git clone, the baseline, or the connect checkout. The
commander verifies your diff, runs the tests itself, and does
all committing, pushing, and PR work.

Workdir: the Area P product tree (the commander tells you the
path at launch; it is the tree whose p1-p15 tip you must
re-verify first). Report: retro finding 6 (P8, medium).

## Problem

The read scanner invokes its visitor only after LF or EOF
completes a line. `fs.read` checks `endLine` and remaining
reply bytes only inside that visitor. Once earlier output
fills the 48 KiB reply budget, the scanner still reads the
whole following line before it can stop. The existing fixture
already has the adversarial shape: a six-byte first line
followed by a 300,000-byte second line — the first-window
assertion proves the short answer, but not that the second
line was read; with no LF, the scanner would go to EOF. A tiny
reply can therefore cause unbounded content I/O.

Locations (re-verify at launch; line numbers from the
2026-10-29 retro report):
- `engine/crates/layerfs-store-service/src/read/window.rs:312-358`
  (visitor waits for LF at :337-350; EOF path at :355-357)
- `engine/crates/layerfs-store-service/src/read/reads.rs:43-65`
  (budget refusal at :47-60)
- Fixture: `area_p8_reads.rs:128-135,224-229`

## Change — local, NO public contract or storage change

Give the read scanner an early-stop predicate, evaluated:

1. At a new line, and
2. Once earlier output already fills the reply budget, as the
   now-unreturned line's prefix crosses the remaining bytes.

Do NOT shorten the scan of a line that will itself be
returned truncated, so text-mode Binary classification stays
unchanged. Preserve `nextLine` and truncation semantics
without requiring the unreturned line's terminator.

## Tests (must cover)

- Huge TERMINATED next line: byte/read counters prove the
  scan stopped at the budget boundary instead of reading the
  whole line.
- Huge UNTERMINATED next line (to EOF): same proof; the scan
  must not run to EOF.
- A line that WILL be returned truncated is still scanned
  fully enough that Binary classification is unchanged
  (existing classification tests must keep passing).
- `nextLine` and truncation semantics unchanged (existing
  P8 read tests keep passing).
- Each new test must FAIL on old code for the intended reason
  (unbounded scan) and PASS after. If a test cannot fail on
  old code, say so explicitly instead of claiming coverage it
  does not have.

## Verification matrix

- `export PATH="$HOME/.cargo/bin:$PATH"`, build first
  (--no-run), explicit ≤120 s timeouts: the suites you touch
  + `area_p8_reads`, `area_p1_history`, `area_p14_paging`,
  `area_p7_attribution`, `sdk_method_golden` (under
  `umask 022`), bridge control/observed records.
- `python3 -B tools/check_product_boundary.py` (+ tool
  self-tests).
- JS neighbors (area-p, calls, cli.test) only if touched;
  otherwise state they were not re-run and why.
- `cargo +1.85.1 fmt --all -- --check` and workspace clippy
  (excl. node, `-D warnings`; components ARE installed).
- /dev/fuse and Docker do not exist here — do not touch
  mounted/real-E2E tests; disclose as NOT RUN.

## Progress + report

Append one line per suite to PROGRESS-retro-f6.md
(append-only). Final report, concise: what changed
(production / test), the byte/read-counter evidence for both
the terminated and unterminated huge-next-line cases, every
suite with exact pass/fail numbers, anything not verified and
why. Do not paste whole diffs.
