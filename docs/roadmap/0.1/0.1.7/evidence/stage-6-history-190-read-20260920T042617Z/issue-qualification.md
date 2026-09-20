## #190 why admission is INELIGIBLE and O3 is INCOMPLETE — diagnosis, and what resolution requires

Recorded because the two labels read like failures and are not. Neither is a defect in the product, neither is caused by the retained read-path change, and neither can be moved by any product change. Both were already present in every earlier round (L40, L41).

### `g1.o3-pinned-counters = INCOMPLETE` is a harness verdict, by design

All four retained performance traces carry exactly:

```
g1.o3-pinned-counters = INCOMPLETE|no pinned counters for this case|at least one O3 constant pinned for every admission case
```

Trace-derived row status is therefore `INCOMPLETE` for all four (`g1.o1-chain-complete` PASS, `g4.swaps` PASS). The driver's stdout line `history-stride10 PASS gates=2` counts only the driver's own two gates and is not the row status.

- `src/main.rs:563` defines the pinned-constant gates: **O3** against `tests/golden/expected.tsv`, **O1** against pinned identity digests. O3 is pinned counters.
- `src/workload/expected.rs:157-166` returns `Gate::incomplete(...)` when a case has no pinned counters.
- `tests/golden/expected.tsv` pins **217** case ids; the history rows are not among them.
- `src/families/history.rs:1-9` states why: the `history.*` group is **outside the 217** — not in `families::ALL`, `FROZEN_CARDINALITY`, `registry::cases()`, `--smoke` or `--lane full`; it carries its own cardinality, lanes, golden table and verification default, and "amends no 217-row verdict".

So the gate reports that the harness has **no frozen oracle for these rows at all** — not drift, not a miss.

**Two hazards before pinning anything.** (1) A pinned counter must be invariant under every *legitimate* future optimization; the table's own header records the worked example — the re-baseline at `795fb1a2f` moved `counter:pool.commits` 19 → 18 because a codec change crossed one fewer 4 MiB charge boundary. (2) The runner is **stricter than the driver**: `runner.py:1464-1491` returns **FAIL**, not INCOMPLETE, for a row that publishes counters but has no pinned constant. `INCOMPLETE` is the lenient face of the same gap.

### `admission INELIGIBLE` is a declared stance, not a gate

No `INELIGIBLE` record appears in any of the four traces. It is the lane's qualification declaration: `src/gates.rs:24-26` defines INELIGIBLE as "a precondition failed: residency, coverage, cache arm mismatch", and the rows declare `CacheState::CreatedInSample` / `StoreState::CreatedInSample` (`src/registry.rs:53-60,108-115`). The only enforced cache contract in the tree is `shared/cold.py`, and `cold.py::applies` fires **only** for `init_namespace`/`namespace-100000`. No residency contract exists for `history.*`, so AGENTS.md §1 and `benchmark/AGENTS.md:95-97` make the row INELIGIBLE and not admission evidence.

The primitives to build one already exist: `shared/residency.py` (`residency`, `de_warm`, mmap + `mincore` + `msync(MS_INVALIDATE)`, "never touch-every-page"), mirrored in `src/support/instruments.rs`, plus `disk_read_bytes` as the device attestation `mincore` cannot replace.

### Three structural blockers, in order of severity

1. **There is no admission path for this lane.** `runner.py:136-147`: `default_verification_mode` returns `ADMISSION_MODE` only when `lane == "full"`, and `HISTORY_LANES` are explicitly not in `full` (`runner.py:60-65`). A history invocation is always `sample` mode, and a sampled row is `INCOMPLETE`. Even a perfect cache contract and pin set would still produce a sampled row until the owner changes what admission means for these lanes.
2. **The wall budget does not fit.** `benchmark/AGENTS.md:78-84`: complete command **≤ 15 s**, declared exceptions up to **25 s**. Measured history complete commands: **38.1 s** (stride10) and **73.1 s** (stride3). `benchmark_rules.md:716-718` contemplates large-history and endurance cases taking longer and requires them to be "explicitly selectable", and says "an ordinary-lane pass is not complete admission" — so a longer frozen budget is contemplated but **not frozen**. Owner decision; do not shrink the selection to fit or enlarge a timeout.
3. **The cache stance is a modelling question.** The history workload is a same-process write-then-read replay: state *N* reads what states *1..N−1* wrote, inside one timed operation, Store created in-sample. Invalidating the Store's pages between states would invalidate the operation's own work microseconds after writing it, and AGENTS.md §1's forbidden list covers setup/preparation/another sample/another arm/another phase — none of which this is. The rule's own test ("would the work still have to happen inside the timed phase?") is satisfied. The question is whether `CreatedInSample` is the **correct** declaration for this shape and whether it can be **verified** (residency plus device reads at state boundaries) rather than assumed — not whether a cold claim can be manufactured.

### What resolution requires

**Owner decisions (an agent cannot make these):** (a) whether `history.*` carries a frozen oracle at all; (b) whether admission mode and a longer frozen budget extend to these lanes, or whether a reduced selection that fits ≤25 s becomes the admission row while stride10/stride3 stay diagnostic.

**Engineering, once decided:** declare and verify the cache stance; select and pin O3 counters with an invariance argument each, regenerated through `shared/pin_expected.py` ("never hand-edit a value"); produce the row through the runner.

**Explicitly not to be done:** add history rows to `registry::cases()`/`FROZEN_CARDINALITY` to satisfy the 217-row coverage assertion; change `HISTORY_LANES` membership in `full` without the owner; hand-edit a pin; invent a cold claim; move reads into setup; shrink a selection to fit; promote the 120/240 s diagnostic caps to admission rows.

Handoff prompt for the next agent: [`issue190-qualification-handoff.md`](https://github.com/Ephemeral-AI-Lab/layerfs/blob/codex/history-data-access/docs/roadmap/0.1/0.1.7/issue190-qualification-handoff.md). #190 stays open; the historical v0.1.6 comparison is a separate unresolved item and is not a qualification gate for these rows.
