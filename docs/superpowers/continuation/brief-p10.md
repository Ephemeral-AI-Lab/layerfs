# P10 — single-call result API (IMPLEMENTATION)

Issue: layerfs-connect#61. Parent: layerfs-connect#14 ("each
of Node, Python, CLI and MCP can mount, run a command, commit,
read history, and push or pull").

Role: implementer. You do NOT commit, push, open PRs, or touch
the git clone, the baseline, or the connect checkout. The
commander verifies your diff, runs the tests itself, and does
all committing, pushing, and PR work.

Workdir: the Area P product tree (re-verify the p1-p15 tip at
launch).

## Contract first (mandatory, before any code)

P10 maps already-existing results and telemetry onto a single
call. Before coding, write the precise public contract beside
the P6 cases (`api/agent/test/fixture/area-p-contracts.md`
pattern): the call shape, the result shape, the exact refusal
codes for every failure mode, and which existing results and
telemetry fields it carries vs. which it deliberately omits.
Have the contract reviewed (the commander arranges a
contract-review pass) and only then implement.

Constraints carried from the Area P design:

- Reuse public behavior only: Python/CLI/MCP must not get a
  second Store implementation, and this call must not invent
  one either — it composes existing operations.
- New production files ≤999 lines; lib.rs/mod.rs ≤200 lines.
- Disposable execution only; Durable is NOT_RUN.
- Reply budget discipline: if the result can exceed the
  control reply budget, say how it pages, truncates, or
  refuses — do not silently return a partial result.
- No fake clocks, test hooks, weakened assertions, or
  retries. No Store schema change unless the contract review
  explicitly approves one (prefer none).

## Tests + verification

- Contract tests in the P6 harness style (success shape,
  every refusal, boundary cases) plus real-Store tests where
  the surface touches the Store.
- Each new test must FAIL on old code for the intended reason
  (missing call / missing field) and PASS after.
- Matrix: `export PATH="$HOME/.cargo/bin:$PATH"`, build first
  (--no-run), explicit ≤120 s timeouts: the suites you touch
  + `area_p1_history`, `area_p14_paging`, `area_p7_attribution`,
  `sdk_method_golden` (under `umask 022`), bridge
  control/observed records; JS neighbors (area-p, calls,
  cli.test) if the JS surface moves; `cargo +1.85.1 fmt --all
  -- --check`; workspace clippy (excl. node, `-D warnings`);
  `python3 -B tools/check_product_boundary.py` (+ tool
  self-tests).
- /dev/fuse and Docker do not exist here — do not touch
  mounted/real-E2E tests; disclose as NOT RUN.

## Progress + report

Append one line per suite to PROGRESS-p10.md (append-only).
Final report, concise: the contract (shapes + refusals), what
changed (production / test / contract surface), every suite
with exact pass/fail numbers, anything not verified and why.
Do not paste whole diffs.
