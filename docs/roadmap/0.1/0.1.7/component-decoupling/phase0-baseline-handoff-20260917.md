# Phase 0 handoff: the baseline campaign (#178, items P0-1 – P0-3)

> **Status:** Tasking prompt for the Phase 0 baseline of
> [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178) (phased
> optimization execution). Phase 0 is [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)'s
> opening act. It produces **measurements only** — no product, test or harness
> source changes, no optimizations, no tuning. Work until the three
> deliverables below exist as append-only receipts, or are honestly recorded as
> `NOT_RUN`/`UNDETERMINED` with their reasons.

---

## 1. What you are doing and why

Nothing in the #176/#177 registers has been measured: every figure to date is a
declared constant or a source-derived structure. Phase 0 produces the baseline
every later optimization is measured against — the matched-workload receipt
(P0-1), the spilling determination (P0-2) and the per-phase counter baseline
(P0-3) — on one frozen tree, under a contract frozen **before** any collection.

If the baseline comes back saying the gap is small or concentrated somewhere
unexpected, the right outcome is a re-prioritized #178 checklist backed by the
receipt — not quiet abandonment of items. Measured-and-declined is a legitimate
close for any box; silence is not.

## 2. Read first, in this order

1. [`AGENTS.md`](../../../../../../AGENTS.md) — §1 (cache rules), §2 (reuse),
   §3 (running a measurement) are **absolute**; §8 (one construction worker).
2. [`docs/general/benchmark_rules.md`](../../../../general/benchmark_rules.md).
3. [`core/AGENTS.md`](../../../../../../core/AGENTS.md).
4. [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178) — the phase
   plan, the checklists and the rules that apply to every box.
5. [`parallelism-and-batching-study-20260918.md`](parallelism-and-batching-study-20260918.md)
   (what P0-2 decides and why O1 hangs on it) and
   [`complexity-and-roundtrip-research-20260917.md`](complexity-and-roundtrip-research-20260917.md)
   (which counters the Phase 1 items verify against — §2's register).
6. The frozen-contract precedent:
   [`stage-5-verification-addendum-20260917.md`](stage-5-verification-addendum-20260917.md)
   and
   [`../evidence/stage-5-component-comparison-20260917T143008Z/README.md`](../evidence/stage-5-component-comparison-20260917T143008Z/README.md)
   — the identity gate, the budget gate, one sample per case per arm, and the
   matched-pair discipline (each arm built from its own manifest, neither
   calling the other).

## 3. The measurement contract (freeze before collecting)

Write this contract into the evidence directory **first**, then collect once.
No collection happens before the contract file exists.

- **Tree:** one identified commit, clean tracked tree; record `git rev-parse HEAD`
  and `git status` in the evidence directory. Toolchain `+1.85.1`, `--locked`,
  release profile for anything timed.
- **One sample per case per arm.** No best-of, no re-runs until a number looks
  good. Determinism checks (re-running a case to confirm a counter is stable)
  are **diagnostics** and must be labelled as such in the receipt, beside the
  gate sample.
- **Fresh `--output` per run**; receipts are append-only; nothing is
  overwritten; failures and discarded attempts stay on disk.
- **Cache state declared and enforced equally in both arms.** The precedent's
  form — "warm in-process fixture, the base tree is built in the arm process
  before the timer" — is acceptable if declared; no warm/cold pooling; a
  measured phase pays for its own work.
- **Measurement lock:** nothing else runs; record wall time per complete
  command; the complete command (product timer + lifecycle + cleanup) fits the
  ordinary **≤ 15 s** budget, with a declared exception list ≤ 25 s.
- **Identities pinned per receipt:** source commit, clean-tree flag, product,
  compilation seal (profile + toolchain + lock), harness identity, workload
  definition (hash or verbatim spec). A rebuilt artifact needs a rebuilt
  matched arm.
- **Both arms from their own workspaces:** the reference arm from the root
  manifest, the core arm from `core/Cargo.toml`; no private implementation is
  transplanted; neither arm calls the other.
- **No source changes of any kind** — product, test, harness or example. If a
  needed counter or knob does not exist, that is a finding to report with a
  proposed separate change, not something to hack in. Setting a SQLite pragma
  for a *diagnostic* run is allowed only if the receipt labels the run
  diagnostic and the gate sample stays on the unmodified configuration.
- **Evidence home:** a fresh dated directory
  `docs/roadmap/0.1/0.1.7/evidence/phase0-baseline-<stamp>/` with a `README.md`
  manifest (every command, every exit code, every wall time, every NOT_RUN).

## 4. The three deliverables

### P0-1 — the matched-workload receipt, honestly scoped

Enumerate the candidate matched families, then collect the ones that are
honestly matchable through **public entry points on both sides**, and record
the rest as `NOT_RUN` with the reason:

1. **Complete filesystem operations (core `build_filesystem`/`update_filesystem`
   vs the reference Workspace route): expected `NOT_RUN`.** The reference has
   no public entry point that consumes final sorted bindings and returns a
   root; its update is private to `layerfs-workspace`. Re-verify that this
   still holds at the current tree (cite the call sites), then record the
   `NOT_RUN` with the reason — this is the same reason VF-6 was deferred to
   Stage 6, and the baseline must state it rather than inherit it.
2. **Component primitives: matched — collect.** The precedent's three cases
   (small 200/20, wide 2,000/200, large-few-changes 20,000/20) through the two
   example executables, re-collected at the current tree under the same
   contract: identity gate (all six pinned identities must MATCH or the case
   fails, no ratio), budget gate, one sample per case per arm, release profile.
   The earlier collections at `eb42c1347` and `3b4941f1e` are retained
   unedited; any comparison against them is **diagnostic** and labelled as
   such (cross-tree ratios are not stable absolutes).
3. **C2-only save path: investigate, then collect or record `NOT_RUN`.**
   Determine whether a matched "save N supplied canonical objects, one save,
   one finish" benchmark exists through public surfaces on **both** sides (the
   core `Store::begin_save/accept/finish`; the reference's admission entry
   points). If a matched pair exists, collect it at the transaction ceiling
   (8,191 rows / 4 MiB − 1 worth of objects) and at one smaller size. If the
   surfaces do not match honestly, record `NOT_RUN` with the specific reason.

Deliverable: receipts + a comparison table (per case: reference ns, candidate
ns, ratio, identity verdict, wall time), with cache state, worker count
(one), and every declared limit stated per arm.

### P0-2 — the spilling determination

The question, precisely: **does SQLite page-cache spilling occur at core's
transaction sizes (up to 8,191 rows / 4 MiB − 1 canonical bytes) under the
default 2 MiB page cache with `journal_mode = MEMORY`?** This decides whether
#178's P2-1/O1 is real.

- Determine it **empirically**, and record the method and its limits. Candidate
  mechanisms, in order of preference: an SQLite status counter reachable
  through the pinned rusqlite (e.g. a cache-spill status), the `PRAGMA
  cache_spill` read-back (what it does and does not tell you), or a labelled
  diagnostic A/B — the same save at the default cache versus a raised
  `cache_size`, comparing the observables that exist (commits, elapsed as
  diagnostic, journal behaviour).
- If no observable exists without a code change, the honest answer is
  `UNDETERMINED — no reachable observable without instrumentation`, plus the
  proposed instrumentation as a finding for a separate change. Do not guess
  from the constant sizes alone.

### P0-3 — the per-phase counter baseline (core only)

Capture, on the frozen tree, the counters the Phase 1 items verify against —
this is the before/after anchor for #178:

- **Workloads** (define them in the frozen contract, then keep them fixed):
  the existing harness cases are the vehicles — `measure_filesystem`'s cases
  per mode, `measure_edits`' cases per mode, `filesystem_timing_c1`'s cases,
  one C2 save at the transaction ceiling, and one ordering-heavy update in the
  s5term grid shape **at the default pending ceiling** (4,096) plus the same
  shape **at the forced 64** (the forced shape is P1-5/P1-13's anchor; the
  default shape shows whether spills happen at all in normal operation).
- **Counters to record per workload:** `ObjectWork` (objects_read,
  objects_emitted, bytes_read, **read_waves**), `SortedWork` for directories
  and inodes (pages_read — noting in the receipt that this counter currently
  undercounts batched decodes, #178 P1-11 — plus pages_created/reused,
  peak_scratch_bytes), `ReferenceWork` (rows_spilled, peak_pending,
  `runs.rows_read/rows_written/runs_created/merges/peak_run_bytes/peak_live_runs`),
  the edit route's `nodes_read`, `ValidationWork` (entries_examined,
  objects_read), storage counters (packs created, commits, pooled values), and
  the timer's per-phase elapsed as **diagnostic-grade** single samples.
- For each counter, state whether it is deterministic for the fixed input
  (one labelled determinism re-run is allowed as a diagnostic).

## 5. Acceptance

- [ ] The contract file exists in the evidence directory before any receipt.
- [ ] P0-1: every matched family collected with identities/limits/cache
      state/workers declared, or recorded `NOT_RUN` with a current-tree reason.
- [ ] P0-2: spilling determined, or `UNDETERMINED` with the method's limits and
      the proposed instrumentation.
- [ ] P0-3: the counter baseline exists for the frozen workload set, with the
      Phase 1 anchor counters present and determinism labelled.
- [ ] Every command's exit code and wall time recorded; every failure retained.
- [ ] One summary comment on #178 ticking only P0-1/P0-2/P0-3 with the receipt
      links; the Phase 1/2 checkboxes are untouched.
- [ ] No commit touches `core/crates`, `crates/`, tests, examples or tools.
      Evidence-directory commits are documentation-only and report production
      LOC (expected delta 0).

## 6. What you must not do

- No optimizations, no constant changes, no pragma changes on gate samples, no
  harness edits — not even "small" ones to make a counter reachable.
- No re-running a case to get a better number; no pooling warm and cold rows;
  no quoting a diagnostic as a gate.
- No closing of #178 items by argument; the boxes move only on receipts.
- Do not start Phase 1, however tempting the baseline looks — post the
  summary and stop. The re-prioritization decision belongs to the owner.
- Do not touch the #176/#177 registers; a cross-reference comment is enough if
  a baseline fact affects them.

## 7. If you cannot finish

Post on #178 with: the item, the exact failing artifact, the command and its
output, the constraint that blocks it, and the dispositions you need from the
owner (extend scope, waive in writing, or change the contract). An honest
`NOT_RUN`/`UNDETERMINED` with its reason is an acceptable outcome; a
green-looking baseline with a silent gap is not.
