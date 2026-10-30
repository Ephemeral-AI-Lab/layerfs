# P4 — Python SDK (IMPLEMENTATION, per D10)

Issue: layerfs-connect#55. Parent: layerfs-connect#14.
Design decision D10 (approved 2026-10-28).

Role: implementer. You do NOT commit, push, open PRs, or touch
the git clone, the baseline, or the connect checkout. The
commander verifies your diff, runs the tests itself, and does
all committing, pushing, and PR work.

Workdir: the Area P product tree (re-verify the p1-p15 tip at
launch).

## Contract first (mandatory, before any code)

P4 gives Python the same surface Node has (mount, run a
command, commit, read history, push/pull — with push/pull
still blocked by R7 #42, see `brief-p3-status.md`). Before
coding, write the precise public contract beside the P6
cases: the Python API shape, its error/refusal mapping onto
the existing refusal codes, and the parity matrix vs. the
Node SDK (every Node operation: present, equivalent, or
explicitly deferred with a reason). Have the contract
reviewed before implementing.

## Implementation shape (D10, fixed)

- Linux: native Python binding over the PUBLIC Rust API
  (PyO3; maturin is the build tool; PyO3 is the authorized
  new locked dependency). No second Store implementation.
- macOS/Windows: the existing authenticated fixed-length JSON
  worker protocol. No new transport.
- First wheel target: manylinux x86_64, Python 3.10–3.13,
  as much of the ABI3 subset as possible.
- The Python SDK must reuse public behavior only — it
  delegates to the same operations the Node SDK uses.

## Tests + verification

- Parity tests: the P6 contract cases exercised through
  Python, plus real-Store tests on Linux (Disposable
  fixtures).
- Each new test must FAIL on old code for the intended reason
  and PASS after.
- Matrix: `export PATH="$HOME/.cargo/bin:$PATH"`, build first
  (--no-run), explicit ≤120 s timeouts: the suites you touch
  + the P6 contract suites + `sdk_method_golden` (under
  `umask 022`); `cargo +1.85.1 fmt --all -- --check`;
  workspace clippy (excl. node, `-D warnings`); `python3 -B
  tools/check_product_boundary.py` (+ tool self-tests);
  Python packaging checks for the wheel (maturin build).
- /dev/fuse and Docker do not exist here — do not touch
  mounted/real-E2E tests; disclose as NOT RUN. macOS/Windows
  worker behavior is NOT verified on this Linux host;
  disclose that explicitly.

## Progress + report

Append one line per suite to PROGRESS-p4.md (append-only).
Final report, concise: the contract (API + parity matrix +
refusals), what changed, every suite with exact pass/fail
numbers, anything not verified and why (especially the
macOS/Windows path). Do not paste whole diffs.
