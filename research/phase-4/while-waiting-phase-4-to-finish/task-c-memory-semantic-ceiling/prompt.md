/goal Define the smallest exact semantic boundary needed for a Memory lane to
provide Phase 4 parity and a shared-core performance ceiling beside SQLite,
without designing or implementing a generic engine framework.

## Scope and sole write authority

Work only in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`. WP4-M is active
and nonterminal. Do not edit, message, interrupt, steer, or wait on it. Do not
use partial WP4-M results.

You may create exactly:

`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/research/phase-4/while-waiting-phase-4-to-finish/task-c-memory-semantic-ceiling/report.md`

If the assigned `report.md` already exists, stop and report the collision; do
not overwrite it.

All other paths are read-only. Do not run Cargo, rustc, tests, SQLite,
benchmarks, compression, or any command that writes under `target/`. Inspect
dirty tracked files through
`git show d781173a08ab4092eb539c3a0870056e6c6a77ff:<path>` or sealed custody
sources.

If WP4-M has begun measured release rows, do not run local shell commands or
write the report until those rows are quiet or the active task is terminal;
web research and reasoning may continue.

## Purpose

Prepare WP5-WP9 by defining what Memory and SQLite must share so that Memory
measures the shared CAS+CDC+COW/canonical ceiling without pretending to satisfy
durability. The result must be the minimum real boundary required by two
implementations, not a factory, registry, provider framework, connection pool,
or speculative third-backend API.

## Read first

1. `research/phase-4/decision-map.md`
2. `research/phase-4/core/pipeline/full-create-pipeline.md`
3. `research/phase-4/core/cas/authenticated-reuse.md`
4. `research/phase-4/core/canonical/v2-single-identity.md`
5. `research/phase-4/assurance/verification-security-resources.md`
6. `research/phase-4/foundations/invariant-matrix.md`
7. `implementation-detail/phase-4/rollback/spec.md`
8. `implementation-detail/phase-4/rollback/implementation-plan.md`
9. `implementation-detail/phase-4/algorithm/spec.md`
10. `implementation-detail/phase-4/algorithm/tests-and-benchmarks.md`
11. `implementation-detail/phase-4/mapping/logical-persistence.md`
12. committed `crates/layerfs-core` and `crates/layerfs-engine/src/lib.rs`;
13. the sealed accepted F2 source for the advanced private mapping semantics.

## Required analysis

First distinguish these existing or planned layers explicitly; do not call
them all an “engine” or infer parity from shared names:

1. production core semantics in `layerfs-core`;
2. the current `InMemoryCas` and `LogicalFile` utilities;
3. the production SQLite `Engine` and its durable publication path;
4. the benchmark-private WP4-M `Store`, which is evidence machinery rather
   than a production backend;
5. the responsibilities deferred to WP5, WP6, WP7, and later work packages.

Trace the operations actually needed for semantic parity:

- immutable put and fully authenticated equal reuse;
- complete canonical get and bounded range access;
- transaction/open/mutation-scoped evidence where required;
- root, delta, receipt, generation, and visible-head semantics;
- rollback and failure provenance;
- ambiguous publication classification versus Memory `NotApplicable` cases;
- full create, same-count edit, +1 edit, directory operations, scrub,
  reconstruction, and ranges;
- exact typed errors and precedence;
- bounded memory/Q and cleanup;
- counters needed to distinguish shared-core from backend cost.

For every operation or observation, classify:

- `SharedSemantic` — identical logical outcome and error required;
- `SQLiteOnly` — durable/profile/VFS/SQL/pager behavior;
- `MemoryOnly` — if any genuinely necessary behavior exists;
- `NotApplicable` — explain why rather than emitting zero;
- `Unavailable` — name the missing API/evidence.

Specify how a fair Memory/SQLite comparison preserves identical source,
identities, canonical bytes, roots, deltas, closure, reconstruction, ranges,
and changed-work counters while reporting durability correctly as
`NotApplicable` for Memory.

## Anti-abstraction rules

Do not propose:

- a public generic engine framework;
- a provider registry or factory;
- async, workers, pools, retries, or another backend;
- capability negotiation for one hypothetical future implementation;
- serialization changes;
- implementation code.

Prefer the smallest internal semantic port extracted only where both Memory
and SQLite demonstrably need it. It is valid to conclude that no trait should
be frozen until the two concrete callers exist.

## Evidence rules

Label claims `Observed`, `Derived`, `Hypothesis`, or `Unavailable`. Do not use a
Memory time as durable throughput. Do not convert absent SQL/VFS/COMMIT work to
zero-cost success; use `NotApplicable` and keep shared work equal.

## Required report

Write only the assigned `report.md`, containing:

1. executive disposition;
2. exact current semantic call graph;
3. current-layer responsibility table distinguishing the five layers above;
4. minimal operation boundary, expressed as responsibilities and data flow
   rather than production Rust unless a tiny signature clarifies it;
5. Memory/SQLite parity matrix;
6. counter and evidence-classification matrix;
7. exact error/rollback/publication expectations;
8. resource/Q ownership and terminal cleanup requirements;
9. fair shared-core ceiling methodology, without running it;
10. dependencies on WP4-P and canonical-v2 decisions;
11. explicit rejected abstractions;
12. next implementation handoff limited to WP5/WP6 preparation;
13. linked local sources.

End with exactly one disposition:

- `READY_FOR_TWO_CONCRETE_LANES`
- `WAIT_FOR_PROFILE_SELECTION`
- `WAIT_FOR_CANONICAL_V2`
- `SEMANTIC_BOUNDARY_INCOMPLETE`

## Completion

Complete only when every required operation and observation has one owner and
classification, no speculative framework is introduced, local links resolve,
no other file changed, and the final response gives the report path,
disposition, minimum boundary, blockers, and SHA-256.
