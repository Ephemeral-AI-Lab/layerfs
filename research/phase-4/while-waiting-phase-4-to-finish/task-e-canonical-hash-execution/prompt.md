/goal Determine, without changing or executing LayerFS, whether the exact-output
canonical ObjectId/BLAKE3 path contains an identity-preserving optimization
large enough to justify one later Phase 4 experiment, and write one research
report with a prospective handoff.

## Scope and sole write authority

Work only in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`. The WP4-M task is
active and incomplete. Do not message, interrupt, steer, or use partial results
from it.

You may create exactly:

`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/research/phase-4/while-waiting-phase-4-to-finish/task-e-canonical-hash-execution/report.md`

If the assigned `report.md` already exists, stop and report the collision; do
not overwrite it.

Everything else is read-only. Do not edit this prompt or another research
document. Do not run Cargo, rustc, tests, executables, SQLite, compression,
profilers, disassemblers, filesystem experiments, or commands that write
`target/`. Do not download a local source tree. Use official upstream web
sources for BLAKE3 behavior and committed/sealed local source for LayerFS.

If the active task has begun measured release rows, do not run local shell
commands or write the report until the measured campaign is quiet or complete;
web research and reasoning may continue. A read-only status check is allowed,
but do not wait on or communicate with the task.

Use `git show d781173a08ab4092eb539c3a0870056e6c6a77ff:<path>` for dirty
tracked files. For accepted benchmark behavior use the sealed F2 source at:

`target/wp4m-f4a-residual-attribution-k64-20260820-v1/custody/sources/phase4_create_edit_benchmark-f2-accepted.rs`

Recorded SHA-256:
`c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158`.

## Objective and non-authority

Identify one current-format mechanism, or conclude none exists, that reduces
canonical-identity execution while preserving exact canonical bytes,
ObjectIds, authentication, error precedence, bounded Q, caller-thread
execution, and one durable publication. Do not implement, benchmark, change
identity, authorize workers, or replace BLAKE3.

Existing research is hypothesis/routing context, not evidence.

## Read first

1. `research/phase-4/index.md`
2. `research/phase-4/decision-map.md`
3. `research/phase-4/foundations/benchmark-and-evidence.md`
4. `research/phase-4/foundations/invariant-matrix.md`
5. `research/phase-4/foundations/hypothesis-ledger.md`
6. `research/phase-4/core/pipeline/full-create-pipeline.md`
7. `research/phase-4/core/canonical/identity-and-hashing.md`
8. `research/phase-4/core/canonical/v2-single-identity.md`
9. `research/phase-4/assurance/verification-security-resources.md`
10. `implementation-detail/phase-4/algorithm/spec.md`
11. `implementation-detail/phase-4/algorithm/complexity-analysis.md`
12. `implementation-detail/phase-4/algorithm/tests-and-benchmarks.md`
13. `implementation-detail/phase-4/mapping/logical-persistence.md`
14. `implementation-detail/phase-4/wp4m/f-series/f2/report.md`
15. `implementation-detail/phase-4/wp4m/f-series/f4/report.md`
16. committed/sealed versions of:
    - `crates/layerfs-core/src/identity/`
    - `crates/layerfs-core/src/object/codec.rs`
    - `crates/layerfs-core/src/content/persistence.rs`
    - `crates/layerfs-engine/src/lib.rs`
    - accepted F2 benchmark source;
17. pinned dependency declarations in `Cargo.toml` and `Cargo.lock`.

## External research

Use only:

- the official BLAKE3 specification;
- official BLAKE3 repository/source/API documentation for the pinned `1.8.5`
  behavior;
- official Rust documentation when an API or language guarantee matters;
- architecture-vendor specifications for an ISA claim.

Do not use blogs, Stack Overflow, benchmark aggregators, marketing summaries,
or another application's speed as LayerFS evidence. Cite primary URLs near the
claim. Do not quote at length.

## Required analysis

### 1. Exact hash-pass inventory

Inventory every accepted full-create hash purpose materially involving:

- raw `ChunkId`;
- canonical `ObjectId` construction;
- canonical incumbent/post-COMMIT authentication;
- construction whole-source witness;
- ordered CDC sequence witness;
- closure, receipt, transition, or authority hashes where relevant.

For each report caller chain, domain/framing, input class, observed/derived
bytes and calls, purpose, current-profile necessity, F4 timer family, and
whether removal changes identity, authority, error precedence, or format.

### 2. Canonical ObjectId execution graph

Trace generated canonical Bytes from framing through materialization, hashing,
CAS admission, SQLite binding/write, evidence handoff, incumbent reuse, scrub,
reconstruction, and ranges. Determine whether an already-authoritative
canonical ID is recomputed or can be carried safely through existing evidence.

Separate construction hashing, incumbent authentication, fresh verification,
raw-payload rehashing, and benchmark-only witness work.

### 3. Supported BLAKE3 facts

From pinned configuration and official upstream source determine:

- enabled/disabled features and public API shape;
- documented AArch64 single-thread/SIMD behavior;
- whether retained evidence already establishes NEON;
- per-object initialization/finalization versus byte-compression work;
- safe reusable domain-prefix state;
- exact-output streaming encode+hash support;
- whether streaming removes a pass or merely overlaps operations;
- any parallel/multi-input interface, explicitly labeled contract-breaking if
  it violates caller-thread/no-worker rules.

Do not recommend private BLAKE3 internals or custom cryptography.

### 4. Mechanism assessment

Evaluate at least:

- reusable domain-prefix state;
- encode-and-hash during one canonical production traversal;
- carrying a computed canonical ID through put evidence;
- reducing per-object hasher setup/finalization;
- larger chunks as a separate versioned CDC/profile question;
- multi-message/parallel hashing as a separately authorized execution profile;
- canonical-v2 as format-coupled and outside an identity-preserving candidate.

For each state code touchpoint, exact removable/overlappable work, mandatory
replacement, compatibility, security/error/Q effects, gross F4 ceiling,
missing fact, future direct counter, and kill rule. Fewer calls are not speed
evidence.

### 5. Target contribution

Use accepted F2-v3 `659.593 ms` as performance authority and F4-A only for
attribution. Bound contribution toward 500, 400, 333.333, and conditional
250-ms levels using same-row or explicit gross-ceiling equations. Never add
independent medians. The `96.068 ms` canonical lane is mostly mandatory and
must not be called removable wholesale.

## Evidence rules

Label claims `Observed(source/API)`, `Derived(equation)`, `Hypothesis`, or
`Unavailable(reason/source)`. Never infer instructions, SIMD utilization,
cache behavior, or physical I/O from wall time. Never treat live WP4-M code,
partial output, or research proposals as accepted evidence.

## Required report

Write only the assigned `report.md`, containing:

1. terminal disposition first;
2. custody and evidence hierarchy;
3. accepted versus live/in-progress source distinction;
4. exact hash-pass inventory;
5. canonical ObjectId call/authority graph;
6. pinned BLAKE3 configuration and supported execution facts;
7. ranked identity-preserving mechanisms;
8. separately listed format/contract-changing mechanisms;
9. target contribution bounds;
10. security, error, Q, and durability implications;
11. unavailable facts and anti-recommendations;
12. prospective experiment handoff or exact reason not to run one;
13. primary/local citations.

The handoff section, when justified, must specify one variable, exact retained
control, equality gates, timer/counter equation, mandatory replacement,
focused tests, full durable/lifecycle protection, later serial release A/B,
100/512-MiB obligations, target contribution, and retain/revise/revert rule.
Do not fabricate an experiment for a NO-GO.

End with exactly one disposition:

- `ADVANCE_IDENTITY_PRESERVING_EXPERIMENT`
- `DEFER_FOR_MISSING_STATIC_EVIDENCE`
- `NO_GO_IDENTITY_PRESERVING`
- `FORMAT_COUPLED_ONLY`

## Completion

Complete only when every pass has purpose/bytes/calls/authority classification,
current-format and format changes are separated, no unsupported performance
claim remains, one disposition and actionable handoff/stop rule are present,
local links resolve, no other file changed, and the final response gives the
report path, disposition, mechanism or blocker, and SHA-256.
