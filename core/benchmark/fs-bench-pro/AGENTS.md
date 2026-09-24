# v0.1.7 SDK Init benchmark workflow

Read repository `AGENTS.md`, `core/AGENTS.md`, the general benchmark rules,
and [the current release-only SDK contract](../../docs/benchmark/fs-bench-pro/issue-231/SDK-RELEASE-FOUR-TIER-20260924.md)
before changing this tree or sampling. The older #231 `daemon-host` receipts
and the #236 debug SDK receipts remain historical evidence; do not rewrite
or relabel them.

`runner.py` is the sole `init_namespace` runner. `families/init_namespace.py`
owns the case registry, sealed source preparation, and invocation of the
compiled SDK driver. The driver makes one public
`layerfs_sdk::Client::init_project` call; it does not construct C1/C2/C5 data
itself. No daemon, FUSE, pathless Init, second benchmark runner, or alternate
route may supply a new Init number. MCP and CLI remain outside this benchmark.

The default family selection is exactly the 100- and 1,000-file cases, seed 1,
one sample each, in that order. The 10,000- and 100,000-file cases remain
visible as `NOT_RUN` in that default selection and may be run explicitly
under the release-only four-tier contract. Verification is mandatory and
separate from the timer.
Never resample a case at the same identity, retry a miss, select a best result,
or change a deadline, worker count, fixture, or cache contract to get a pass.
Retain every failed or ineligible attempt in a fresh output directory.

Use a worktree-local Cargo target, prepared masters, Store, scratch and result
root. Build only needed binaries with `--locked` and record a 30 s build budget.
**Use locked Cargo release binaries only** for every new SDK Init measurement:
`runner.py` must build with `--release`, and the SDK driver and independent
verifier must come from `target/release/examples/`. No debug option, debug
fallback or reuse of an old unmarked/debug build cache is allowed. Keep all
older debug v2 and release research receipts under their original identities;
never promote them into the new release selection. A source/cache/operation
change requires its own frozen identity and fresh receipts.
The complete performance command has a 15 s budget; the independent verifier
has a 5 s budget. The two-case cycle has a recommended 30 s budget. Hold the
nonblocking worktree-local run lock while fixtures and result files are mutable;
never block another owner's worktree. No build overlaps a timed operation in
this worktree.

The source cache is uncontrolled, so even a correct, under-budget row is
`admission_eligible=false` and has no numeric latency PASS. Report the single
raw SDK call time, complete command wall, verifier wall, exact route/fixture
identity, external lifecycle CPU, Store/history size, cleanup and any
interference. Do not pool SDK and historical daemon-host rows. The full oracle
reopens Store/history and verifies every path, portable metadata value, byte
count and SHA-256 against the sealed manifest through public readers.

`runner.py verify` and `runner.py report` read retained evidence only. The
manifest hashes every retained result file. Keep existing receipts append-only,
including `FAIL`, `INELIGIBLE` and `NOT_RUN`. Check the focused Python tests
after a harness edit and the owning Core checks once at final source identity;
there is no CI or aggregate pre-push gate.
