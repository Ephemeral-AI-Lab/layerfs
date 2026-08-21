/goal Build a row-wise, evidence-backed Phase 4 cost model for reaching the
100-MiB durable-create stretch target of `<=333.333 ms` / `>=300 MiB/s`, and
write one research report without implementing or timing a candidate.

## Scope and sole write authority

Work only in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`. WP4-M is active,
incomplete, and must not be messaged, interrupted, steered, waited on, or used
as partial evidence.

You may create exactly:

`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/research/phase-4/while-waiting-phase-4-to-finish/task-d-300-mibs-cost-model/report.md`

If the assigned `report.md` already exists, stop and report the collision; do
not overwrite it.

Everything else is read-only. Do not run Cargo, rustc, tests, SQLite,
compression, filesystem experiments, profilers, performance counters, or any
command that writes `target/`. Do not modify sealed evidence. Inline read-only
Ruby/Python/jq arithmetic that reads existing JSON and prints to stdout is
permitted; create no script or temporary file in the repository.

Use committed source through
`git show d781173a08ab4092eb539c3a0870056e6c6a77ff:<path>` when the live file
is dirty.

If WP4-M has begun measured release rows, do not run local shell commands or
write the report until those rows are quiet or the active task is terminal;
web research and reasoning may continue.

## Evidence authority

- Accepted performance authority: F2-v3 candidate, `659.593 ms` median /
  `151.609 MiB/s` for the retained 100-MiB row.
- F4-A: observer-heavy attribution diagnostic only, never a new accepted
  baseline.
- F4-A2: terminal NO-GO for scanner-owned chunk-materialization removal.
- Current canonical-v2, CDC, SQLite, and overlap documents are hypotheses and
  routing context, not candidate evidence.
- WP4-M partial output is unavailable until its terminal manifest/audit.

## Read first

1. `research/phase-4/decision-map.md`
2. `research/phase-4/foundations/benchmark-and-evidence.md`
3. `research/phase-4/foundations/invariant-matrix.md`
4. `research/phase-4/foundations/hypothesis-ledger.md`
5. `research/phase-4/core/pipeline/full-create-pipeline.md`
6. `research/phase-4/core/canonical/identity-and-hashing.md`
7. `research/phase-4/core/canonical/v2-single-identity.md`
8. `research/phase-4/core/cdc/locality-and-algorithms.md`
9. `research/phase-4/storage/sqlite/durability-and-layout.md`
10. `implementation-detail/phase-4/rollback/spec.md`
11. `implementation-detail/phase-4/rollback/implementation-plan.md`
12. `implementation-detail/phase-4/wp4m/f-series/f2/report.md`
13. `implementation-detail/phase-4/wp4m/f-series/f4/report.md`
14. `implementation-detail/phase-4/wp4m/f-series/f4/a2-cdc-materialization.md`
15. `implementation-detail/phase-4/wp4m/f-series/planning/full-create-plan.md`
16. sealed F2/F4/F4-A2 reports, raw JSONL, summaries, and audits under:
    - `target/wp4m-f2-construction-proof-k64-20260819-v3/`
    - `target/wp4m-f4a-residual-attribution-k64-20260820-v1/`
    - `target/wp4m-f4a2-cdc-materialization-k64-20260820-v1/`

Reverify the named terminal hash files and raw-row counts read-only before
using them.

## Required target ladder

Report accepted-F2 reductions required for:

```text
500.000 ms = 200 MiB/s minimum gate
400.000 ms = 250 MiB/s intermediate
333.333 ms = 300 MiB/s formal stretch
250.000 ms = 400 MiB/s conditional research horizon
```

The 250-ms value is not a Phase 4 acceptance promise.

## Mandatory row-wise arithmetic

Use integer nanoseconds where raw fields permit.

For each accepted F2 candidate row:

```text
saving_needed_i(T) = max(0, durable_wall_i - target_wall_T)
```

For each measured F4-A row and one modeled mechanism:

```text
modeled_wall_i
  = durable_wall_i
  - union(nonoverlapping eligible removed intervals_i)
  + mandatory replacement work_i
  + added work_i
```

Rules:

1. Never calculate `median(parent) - median(component)`.
2. Never add independently selected component medians.
3. Never pair an F2 row with an F4 row by row number or order.
4. Never add a parent timer to a child or subtract overlap twice.
5. Never call required work removable because it is expensive.
6. Never substitute VFS logical calls/bytes for physical I/O.
7. Never substitute unavailable mandatory work with zero in an expected case.
8. A zero-replacement gross ceiling is allowed only when visibly labeled as
   optimistic over-crediting, not a forecast.
9. Publish every modeled row before medians/min/max/spread/target counts.
10. Preserve timer equations and name residual/observer work.

## Required scenarios

Model at least:

1. accepted F2 authority and target gaps;
2. current F4-A diagnostic work;
3. v2 raw-ID-only removal;
4. v2 raw-ID plus full combined construction-lane subtraction as a deliberately
   over-credited gross ceiling;
5. v2 plus a parameterized exact-boundary CDC improvement;
6. v2 plus a parameterized canonical-hash improvement;
7. v2 plus a parameterized SQLite mapping/COMMIT improvement;
8. combined shared-core plus SQLite residual;
9. bounded CPU/I/O overlap as a critical-path model only.

For parameterized cases, solve the additional required saving rather than
inventing a likely percentage:

```text
required_additional_i
  = max(0, modeled_wall_after_prior_changes_i - 333_333_000)
```

For overlap, never subtract both serial lanes. The best-case overlapped stage
is at least `max(required_cpu_lane, required_io_lane)` plus ordering, handoff,
queue, cancellation, and serial publication work. If current evidence cannot
identify independent intervals, mark the overlap ceiling `Unavailable`.

## Mandatory classifications

Distinguish:

- mandatory source inspection;
- exact CDC work;
- mandatory canonical ObjectId hashing;
- potentially redundant raw identity hashing;
- unresolved source-witness versus ordered-commitment work;
- encode/framing;
- CAS/SQLite API work;
- VDBE+pager composite;
- direct VFS work;
- required main-database writes and FULL sync;
- movable but not removable work;
- observer/residual;
- unavailable physical or overlap facts.

Use `Observed`, `Derived`, `Hypothesis`, and `Unavailable` literally.

## Required report

Write only the assigned `report.md`, containing:

1. terminal disposition first;
2. custody snapshot, evidence hashes, and raw-row counts;
3. evidence hierarchy and exclusions;
4. accepted target/gap table;
5. all accepted F2 candidate-row gaps;
6. all F4-A row-wise component equations used;
7. raw-ID-only and gross combined canonical-v2 row models;
8. CDC, canonical-hash, SQLite, and overlap scenario matrices;
9. every row followed by median/min/max/spread and target counts;
10. Amdahl/critical-path interpretation;
11. mandatory work and unavailable observations;
12. ranked future measurements, each tied to the uncertainty it resolves;
13. exact stop rules and contract changes required by each route;
14. linked local evidence.

End with exactly one disposition:

- `ROUTE_IDENTIFIED`
- `NO_MEASURED_ROUTE`
- `INSUFFICIENT_EVIDENCE`

`ROUTE_IDENTIFIED` means only that a named combination has enough measured
gross ceiling. It is not an implementation authorization, candidate PASS, or
performance forecast.

## Completion

Complete only after raw hashes/counts and every row equation are independently
checked, no cross-row/cross-campaign subtraction occurred, no partial WP4-M
result was used, all local links resolve, no other file changed, and the final
response gives the report path, disposition, most important uncertainty, and
SHA-256.
