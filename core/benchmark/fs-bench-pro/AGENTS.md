# v0.1.7 SDK Init benchmark workflow

Read repository `AGENTS.md`, `core/AGENTS.md`, the general benchmark rules,
and [the #236 SDK route contract](../../docs/benchmark/fs-bench-pro/issue-236-sdk-init/SPEC.md)
before changing this tree or sampling. The older #231 `daemon-host` receipts
and specification remain historical evidence; do not rewrite or relabel them.

For the prospective #232 Workspace Exec/FUSE edit route, also read
[`exec2edit.md`](../../docs/benchmark/fs-bench-pro/exec2edit.md). Every
benchmark performance driver must use the public `layerfs-sdk` package for
product operations. `runner.py` checks registered `benchmark_*` driver source
before building or reusing a binary. The check complements runtime route
counters and independent verification; it does not register an edit case.
Until the SDK supplies Branch setup and full sandbox assembly, an SDK-only
exec-to-edit family remains `NOT_RUN`. The current runner selects Init only.

`runner.py` is the sole `init_namespace` runner. `families/init_namespace.py`
owns the case registry, sealed source preparation, and invocation of the
compiled SDK driver. The driver makes one public
`layerfs_sdk::Client::init_project` call; it does not construct C1/C2/C5 data
itself. No daemon, FUSE, pathless Init, second benchmark runner, or alternate
route may supply a new Init number. MCP and CLI remain outside this benchmark.

The default family selection is exactly the 100- and 1,000-file cases, seed 1,
one sample each, in that order. The 10,000- and 100,000-file cases remain
visible as `NOT_RUN`. Verification is mandatory and separate from the timer.
Never resample a case at the same identity, retry a miss, select a best result,
or change a deadline, worker count, fixture, or cache contract to get a pass.
Retain every failed or ineligible attempt in a fresh output directory.

Use a worktree-local Cargo target, prepared masters, Store, scratch and result
root. Build only needed binaries with `--locked` and record a 30 s build budget.
**Use the default Cargo debug profile only** for this SDK Init selection:
`runner.py` must build without `--release`, and the SDK driver and independent
verifier must come from `target/debug/examples/`. Do not substitute a release
binary, an optimization flag, or a release diagnostic receipt to improve a row.
Keep the historical 10k release diagnostic separate from the registered debug
cases and do not compare debug SDK timings with release daemon-host timings as
a regression claim. A different profile requires its own frozen selection and
new receipts; it never relabels these v2 observations.
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
