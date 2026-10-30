# P11 — storage statistics (IMPLEMENTATION)

Issue: layerfs-connect#62. Parent: layerfs-connect#14.

Role: implementer. You do NOT commit, push, open PRs, or touch
the git clone, the baseline, or the connect checkout. The
commander verifies your diff, runs the tests itself, and does
all committing, pushing, and PR work.

Workdir: the Area P product tree (re-verify the p1-p15 tip at
launch).

## Contract first (mandatory, before any code)

P11 extends the existing `get_storage` surface with storage
statistics. Before coding, write the precise public contract
beside the P6 cases: every statistic field, its exact meaning
and unit, how it is computed (and its cost — statistics that
require a full scan must say so and must not be presented as
cheap), the refusal codes, and the JS/CLI shapes. Have the
contract reviewed before implementing.

Constraints carried from the Area P design:

- Statistics must be computed from the real Store, never
  estimated, sampled, or cached across calls unless the
  contract explicitly defines the cache and its invalidation.
- The engine rule applies: any claim that a statistic is
  cheap must carry EXPLAIN + runtime-profile evidence.
- New production files ≤999 lines; lib.rs/mod.rs ≤200 lines.
- Disposable execution only; Durable is NOT_RUN.
- No fake clocks, test hooks, weakened assertions, or
  retries. No Store schema change unless the contract review
  explicitly approves one (prefer none).

## Tests + verification

- Real-Store tests: statistics on empty, small, and
  multi-branch stores, verified against independent counts
  (not against the implementation's own code path).
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

Append one line per suite to PROGRESS-p11.md (append-only).
Final report, concise: the contract (fields + computation
cost + refusals), what changed, every suite with exact
pass/fail numbers, anything not verified and why. Do not
paste whole diffs.
