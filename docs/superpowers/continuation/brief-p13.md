# P13 — folder-scoped client (IMPLEMENTATION)

Issue: layerfs-connect#64. Parent: layerfs-connect#14.

Role: implementer. You do NOT commit, push, open PRs, or touch
the git clone, the baseline, or the connect checkout. The
commander verifies your diff, runs the tests itself, and does
all committing, pushing, and PR work.

Workdir: the Area P product tree (re-verify the p1-p15 tip at
launch).

## Contract first (mandatory, before any code)

P13 is an options/wrapper slice: a client scoped to a folder.
Before coding, write the precise public contract beside the
P6 cases: how the scope is expressed (options shape), what
"scoped" confines (reads? writes? history? exec? — decide
each operation explicitly, do not leave one ambiguous), what
happens on an out-of-scope path (exact refusal code and
message class), how the scope composes with workspaces and
branches, and the JS/CLI shapes. Have the contract reviewed
before implementing.

Constraints carried from the Area P design:

- The scope must be enforced at the operation boundary, not
  by convention in the client wrapper alone: a scoped client
  must not be able to escape its folder through any operation
  it exposes (check every operation, including history reads
  and exec, against the scope).
- No second Store implementation; the wrapper delegates to
  the existing public behavior.
- New production files ≤999 lines; lib.rs/mod.rs ≤200 lines.
- Disposable execution only; Durable is NOT_RUN.
- No fake clocks, test hooks, weakened assertions, or
  retries. No Store schema change unless the contract review
  explicitly approves one (prefer none).

## Tests + verification

- Confinement tests: every exposed operation attempted
  out-of-scope refuses with the contracted code; in-scope
  operations behave identically to the unscoped client.
- Composition tests: scope + workspace, scope + branch,
  scope + history reads.
- Each new test must FAIL on old code for the intended reason
  and PASS after.
- Matrix: `export PATH="$HOME/.cargo/bin:$PATH"`, build first
  (--no-run), explicit ≤120 s timeouts: the suites you touch
  + `area_p1_history`, `area_p14_paging`, `area_p7_attribution`,
  `sdk_method_golden` (under `umask 022`), bridge
  control/observed records; JS neighbors if the JS surface
  moves; `cargo +1.85.1 fmt --all -- --check`; workspace
  clippy (excl. node, `-D warnings`); `python3 -B
  tools/check_product_boundary.py` (+ tool self-tests).
- /dev/fuse and Docker do not exist here — do not touch
  mounted/real-E2E tests; disclose as NOT RUN.

## Progress + report

Append one line per suite to PROGRESS-p13.md (append-only).
Final report, concise: the contract (scope semantics +
refusals + composition), what changed, every suite with exact
pass/fail numbers, anything not verified and why. Do not
paste whole diffs.
