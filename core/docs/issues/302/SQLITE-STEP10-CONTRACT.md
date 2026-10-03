# SQLite-only Step 10 contract

> **Status:** Prospective Init contract; history deadline/driver binding pending.

This is a new treatment. It does not relabel PostgreSQL/MinIO receipts or the
old MEMORY/OFF measurements. The active goal remains all seven selections;
NOT_RUN or a small-case PASS is not terminal success.

## Registered selections and gates

| Case ID | Workload | Complete performance command | Independent proof | Allocation gate |
| --- | --- | --- | --- | --- |
| phase7-sqlite-init-100-v2 | namespace-100-compact-v3, 100 files /5,000,000 logical B | <=15s | <=9.5s | final SQLite total<=matched final Phase4.5 total |
| phase7-sqlite-init-1000-v2 | namespace-1000-compact-v3,1000 files /20,000,000B | <=15s | <=9.5s | same |
| phase7-sqlite-init-10000-v2 | namespace-10000,10000 files /300,000,000B | <=15s | <=9.5s | same |
| phase7-sqlite-init-100000-v2 | namespace-100000,100000 files /500,000,000B | <=15s | <=9.5s | same |
| phase7-sqlite-history-stride10-v1 | 17 retained states, original producer/corpus | PENDING: reconcile original60s with current15/25s rule | <=9.5s; actual bounded proof binding pending | <54,278,964B |
| phase7-sqlite-history-stride3-v1 | 53 retained states | PENDING: original170s | <=9.5s; binding pending | <70,427,034B |
| phase7-sqlite-history-stride1-v1 | 157 retained states | PENDING: original170s | <=9.5s; binding pending | <92,342,273B |

Competitive time per required case: **10*candidate_ns<=11*baseline_ns**.
This is the complete total 10% margin, not a second discretionary waiver.
Init allocation uses the conservative proposed no-growth bound, frozen before
sampling after a reasonable opportunity to clarify. Database/WAL/SHM bytes
observed before the candidate's final checkpoint are separately reported, along
with final bytes; neither observation is an unobserved lifetime peak. Current
history ceilings are preserved. Budget changes require an explicit prospective
owner direction; unrun/over-budget work cannot pass.

## Init comparison scope

The new v2 comparison is the driver's complete-product clock, including fresh
Store database creation/opening, the real namespace import, required final
checkpoint and final connection close. All candidate WAL/FULL/fullfsync work
is inside the measured child. The phase4.5 arm runs unmodified product at
7edddbdb8 with its original MEMORY/OFF profile. A small reference-only harness
calls the same public Service import body with identical explicit stack41/seed42/
name and recording disabled. This is a direct C1/C2/C5 component comparison,
not a relabeled SDK call or the old diagnostic timer. Current product does not
restore Service/grants/protocol envelopes.

The candidate uses Handles::create, Storage::new and project::init. The reference
creates both native Store/history and the Service in its child. No empty database
schema work is moved into setup for either arm. Internal bootstrap/Init/commit/
checkpoint/close spans are explanatory; they are not added to external wall.
The performance-command envelope starts before cold preconditioning and ends
after measured child/resource collection/scratch cleanup check. Fixture reuse
and release builds are outside performance; independent verification is separate.
No future timer subtraction or partial state substitutes for the declared scope.

## State, identity and reuse

One sample per case/arm/frozen candidate+reference+harness+fixture identity,
matched new arms for changed measured artifacts. Exclusive persistent claims
prevent a second arm at the same identity. Fresh outputs and single-final-write
receipts retain all failures/timeouts/ineligible rows. No n3/best-of/retry.
Both sides use closed identity-checked prepared source masters; no regenerated
source per sample, no mutated sample or Store reuse. Init is a fresh-output case;
there is no Store clone to call cold. Prepared input reuse is outside timers.

Declared source state: regular-file content pages are invalidated and whole-input
mincore is zero immediately before the child; no payload reads occur during
preconditioning. Directory/filesystem metadata residency is not directly
observed and is explicitly outside this content-page claim. Both product Stores
are fresh/nonexistent at child start. Within-child work pays creation/writes/
construction/reads in the declared complete import; no previous sample/Store is
allowed to credit it. An ineligible cache attestation never becomes a PASS.

Pin source commit/tree, active product and runtime SQL seals, compilation inputs,
root .cargo/config.toml, dependency lock, release/locked binary SHA256/archive,
reference wrapper source, harness and fixture manifest identity. Baseline checkout
is clean/unmodified before/after its temporary harness-only example build, with
its own Cargo target. Each worktree holds its own nonblocking lock. No build
shares a Cargo target or overlaps a measured phase. Init has four constructors;
environment LAYERFS_CONSTRUCTION_WORKERS=1. Retained-history constructors remain
single-producer; their actual port path is still required.

## Proof, accounting and causes

The independent verifier checks every path/kind/directory metadata and complete
metadata/content of a fixed declared file sample. Report selected files/bytes
and manifest totals separately; never call this a full-content proof. Unit-test
100/1000 full namespace oracles are complementary, not substitute receipts.
Independent fixed roots/O3/tree/content and every retained-history operation
remain required when the history driver is bound.

Record wait4 CPU/per-child peak RSS over the compared complete child. This is
not a resettable phase peak of a long-running process or a cgroup lifetime total.
Record actual database/WAL/SHM allocation observations, C2 bounded-owner counts,
namespace retained capacities, SQLite settings/runtime/default VFS selection,
statement/VM/binding/read-write transaction/body/commit/checkpoint work where
available. Direct VFS name/physical sync/write syscalls and unobserved allocator/
OS cache state are explicitly UNAVAILABLE, not guessed from write-commit counts.
No whole-importer memory bound is inferred from these counts. Larger namespace
qualification can require the conditional catalogue/paged builder work.

If candidate exceeds the 10% bound, collect SQL EXPLAIN and EXPLAIN QUERY PLAN
on the retained databases under a declared populated diagnostic state, plus
side-by-side smaller stages and actual statement/transaction/VM/body/commit/
checkpoint counts. EXPLAIN programs are not actual executed VM counts. Nested/
parallel wall spans overlap. If extra execution is required, freeze a labelled
count-driven cause diagnostic, not another unchanged-arm speed sample. Case
failures stay FAIL; required admission/proof/accounting work remains incomplete
until actually satisfied.

## Prospective v2 scope correction and bounded transaction treatment

The v1 external100 PASS masks a44% product regression behind first-execution
startup. Keep that row/status intact; do not claim competitive product speed.
Before the changed bounded treatment, v2 now compares the same complete-product
clock already captured by both drivers, from before database creation until
after final checkpoint/close. Required work stays inside; process launch/exit
and complete command remain separately reported and bounded. This is a tighter
new criterion/identity, not subtraction or relabeling of v1 data.

The treatment fixes independent canonical and sealed-physical charges at the
same4MiB-minus1 limits, shared Arc body ownership, original8191-row bound and
singleton exception. No queue/cache/worker/buffer/durability limit increases.
A bounded unit need not add two representations of the same bytes into a
canonical-only charge. Unused acknowledged pack-ID tails transfer between saves
of one exclusive Storage handle; consumed IDs never recycle on failure. Required
reservation demand uses actual preceding groups, without lowering construction
bounds. Unknown/malformed unacknowledged reservations are never inferred.
