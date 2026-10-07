# Core benchmark routing and evidence

> **Status:** Current general guide.
> Updated 2026-10-07 for owner-directed serverless Store placement.
> Retained receipts and frozen family limits are unchanged.

Read [root instructions](../../../AGENTS.md), [core instructions](../../AGENTS.md),
[measurement workflow](../../../docs/general/agent-measurement-policy.md),
[benchmark rules](../../../docs/general/benchmark_rules.md) and
[report template](../../../benchmark_agent_report.md) before changing a harness
or collecting a sample. Product implementation belongs in core. Root crates are
the v0.1.6 reference until cluster two completes.

## Select the actual operation and profile

`runner.py` routes several distinct families; their presence does not make them
equivalent or authorize rerunning closed experiments. Inspect the selected case
registry, driver, frozen contract, source/profile and closure before invoking it.

- Current cluster-one SQLite comparisons use
  [phase7_sqlite.py](families/phase7_sqlite.py) and
  [phase7_history.py](families/phase7_history.py), with the declared
  [SQLite contract](shared/sqlite_contract.py). They exercise public core Init/
  content/storage/history operations, not a recreated server.
- Use the [cluster-one handbook](../../../cluster_one_handbook.md) for exact
  APIs, persistence/layout profiles and retained results. The current global
  provider is SQLite: host Init uses the system library; direct Linux daemon
  Store access uses the pinned bundled library.
- [Owner-closure results](../../docs/issues/302/SQLITE-OWNER-CLOSURE-RESULTS-20261005.md)
  and the handbook retain the SQLite campaign's failed/withdrawn/unrun outcomes.
  Reuse unaffected qualified evidence by exact identity. A new campaign needs
  its own prospective selection; no automatic old-family sweep or resampling.
- Cluster-two qualification must exercise the actual full-root Workspace/FUSE/
  daemon path described by the [design index](../../docs/issues/303/README.md).
  Current source integration and prospective registrations are prerequisites;
  component Init or a passthrough prototype is not an integrated measurement.
- Legacy SDK/reference cases require their compatible pinned source and explicit
  selection. Do not re-enable excluded API/server packages merely to make the
  current product resemble an old driver. Baseline evidence keeps its own scope.

Host Init seals one Store for one installation into a named VM volume; afterwards
all Store/Storage/History calls run directly in the Linux daemons and the host is
control-only. Each daemon's overlay remains a separate local database. A Store
never lives under the repository bind mount. Follow the selected authentic
topology and [hosting scope](../../../docs/general/benchmark_rules.md#hosting-scope-for-cluster-one-and-cluster-two);
old frozen families retain their topology and cannot qualify the new path.
The removed host-mediated transport is withdrawn from new selections, with
explicit NOT_RUN dispositions and every historical receipt preserved.

## Common execution and reporting requirements

Use locked release binaries for new Init measurements. Keep preparation,
performance, independent proof and complete command wall separate. Declare
source/binary/profile/layout/workload/cold-residency identities and actual owners;
report absent legacy Server/daemon scopes as N/A rather than inventing them.

Worktree-local outputs/targets/locks and sealed prepared masters are required.
Reuse setup and qualifying proofs only through declared mechanisms. No unchanged
sample repeated, best-of result, hidden warm credit, workload reduction or relaxed
timeout/cache/worker policy. Retain every failed, ineligible and unrun row.

General defaults and exact family-specific exceptions are in the measurement
workflow and frozen contracts. Init may use supported parallel construction;
Commit/capture/snapshot remain single-producer. FUSE dispatch concurrency is
separate. Product command lifetimes are not benchmark watchdogs.

After harness edits run scoped owning tests; at final implementation identity
run covering core checks. There is no CI or aggregate pre-push gate.
Report exactly what ran, failed or was reused; do not claim a newer source is
qualified by an older receipt.

## Retained SDK Init selection

The following scoped procedure applies only when the current task explicitly
selects the original SDK family at its compatible pinned identity. It is not
default routing for direct cluster-one Init or new cluster-two operations.

Read repository `AGENTS.md`, `core/AGENTS.md`, the general benchmark rules,
and [the current release-only lite-verifier contract](../../docs/benchmark/fs-bench-pro/issue-231/SDK-VERIFIER-LITE-20260924.md)
before changing this tree or sampling. The older #231 `daemon-host` receipts
and the #236 debug SDK receipts remain historical evidence; do not rewrite
or relabel them.

`runner.py` is the sole `init_namespace` runner. `families/init_namespace.py`
owns the case registry, sealed source preparation, and invocation of the
compiled SDK driver. The driver makes one public
`layerfs_sdk::ProjectApi::init` call; it does not construct C1/C2/C5 data
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
The complete performance command has a 15 s budget; the independent
lite verifier has a prospectively fixed 9.5 s budget, strictly below 10 s.
Historical 5 s receipts keep that limit. The two-case cycle has a recommended
30 s budget. Hold the
nonblocking worktree-local run lock while fixtures and result files are mutable;
never block another owner's worktree. No build overlaps a timed operation in
this worktree.

The source cache is uncontrolled, so even a correct, under-budget row is
`admission_eligible=false` and has no numeric latency PASS. Report the single
raw SDK call time, complete command wall, verifier wall, exact route/fixture
identity, external lifecycle CPU, Store/history size, cleanup and any
interference. Do not pool SDK and historical daemon-host rows. The current
verifier reopens Store/history, inventories every path and inode kind, checks
directory metadata, then checks full metadata and every byte of a declared
deterministic file sample. Report sampled files/bytes separately from the
manifest totals; never call this a full-content oracle. Earlier full-oracle
receipts keep their recorded scope and status.

`runner.py verify` and `runner.py report` read retained evidence only. The
manifest hashes every retained result file. Keep existing receipts append-only,
including `FAIL`, `INELIGIBLE` and `NOT_RUN`. Check the focused Python tests
after a harness edit and the owning Core checks once at final source identity;
there is no CI or aggregate pre-push gate.

## Retired backend comparison routing

The former #302 PostgreSQL/MinIO comparison instructions are superseded by the
implemented SQLite profiles and their closure. Historical plans/receipts remain
at their original identity and in Git; do not execute them as a current backend
assignment or relabel them as SQLite/product qualification.
