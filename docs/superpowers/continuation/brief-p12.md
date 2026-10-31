# P12 — storage summary (IMPLEMENTATION)

Issue: layerfs-connect#63. Parent: layerfs-connect#14.

Role: implementer. You do NOT commit, push, open PRs, or touch
the git clone, the baseline, or the connect checkout. The
commander verifies your diff, runs the tests itself, and does
all committing, pushing, and PR work.

Workdir: the Area P product tree (re-verify the p1-p15 tip at
launch).

## Contract first (mandatory, before any code)

P12 is the CHEAP summary companion to P11's statistics: a
small, bounded summary a client can fetch without paying for
a full scan. Before coding, write the precise public contract
beside the P6 cases: the summary fields, the bound on its
size and cost, what it deliberately omits vs. P11, and the
refusal codes. The contract must state the cost bound
explicitly (e.g. O(1) lookups, or a named small page) and the
implementation must prove it. Have the contract reviewed
before implementing.

Constraints carried from the Area P design:

- Cheap must be proven, not asserted: EXPLAIN + runtime
  profile evidence at two store sizes, showing the cost does
  not grow with store size (or grows only within the stated
  bound).
- The summary must be computed from the real Store, never
  from a stale snapshot or an estimate.
- New production files ≤999 lines; lib.rs/mod.rs ≤200 lines.
- Disposable execution only; Durable is NOT_RUN.
- No fake clocks, test hooks, weakened assertions, or
  retries. No Store schema change unless the contract review
  explicitly approves one (prefer none).

## Tests + verification

- Real-Store tests at two store sizes proving the cost bound
  (Fixture::work counters: statements/rows only, never
  vm_steps), plus shape/refusal tests.
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

Append one line per suite to PROGRESS-p12.md (append-only).
Final report, concise: the contract (fields + cost bound +
proof), what changed, every suite with exact pass/fail
numbers, anything not verified and why. Do not paste whole
diffs.
