# Phase 4 G5 implementation and verification plan

Date: 2026-08-22

Status: **PROSPECTIVE / READY FOR G5-0 PREREGISTRATION**. This document audits
and repairs the post-G5-1 proposal and converts its useful mechanisms into an
ordered implementation plan. It authorizes no measured row by itself, does not
relabel the preserved G5-1 `REVISE` result, and does not permit G5-1/G5-2 to
skip their preceding gates.

## 1. Evidence authority and decision amendment

The preserved [G5-1 final synthesis](../../../research/phase-4/g5-round-0/decision/final-synthesis.md)
and [lane dispositions](../../../research/phase-4/g5-round-0/decision/lane-dispositions.md)
concluded:

```text
G5-A = RETAIN_FULL_REOPEN_AUTHENTICATION
G5-B = RETAIN_K64_F64
G5-C = H11_REVISE_EXACT_BLOCKER
G5-D = RETAIN_CURRENT_SQLITE_PROFILE
```

That remains the correct result under the prior adversarial freshness model.
The later discussion changes the product assumption rather than disproving
that result. It proposes an explicitly weaker local-development mode:

```text
TrustedLocalDev
```

This plan accepts that new mode only as a benchmark-private, opt-in candidate.
The existing verified path remains the default control and is not deleted.
The candidate may stop requiring an eager complete-closure scrub before an
edit, but it may not claim that SQLite authenticates LayerFS objects or proves
freshness against rollback, offline replacement, bit rot, arbitrary SQL, or a
malicious same-UID process.

The accurate decision is therefore:

> Trust the private local SQLite store as the freshness source in the explicit
> `TrustedLocalDev` mode; keep CAS identity checks at object-use boundaries,
> new-object hashing, transition/head validation, SQLite durability, and the
> complete verifier. Do not call this “dropping authentication” without that
> qualification.

## 2. Audit disposition

Disposition of the discussion as written: **REVISE, THEN IMPLEMENT IN
SEPARATE LANES**.

### Retain

- Make the canonical SQLite root authoritative and native files derived.
- Remove eager current/parent full-closure scrub from the trusted edit hot
  path only.
- Preserve `ObjectId = hash(canonical bytes)` and authenticate every canonical
  object actually fetched by an operation.
- Preserve validation of newly supplied canonical bytes and exact incumbent
  equality before immutable ObjectId reuse.
- Preserve expected-head comparison, one serialized writer transaction, one
  publication COMMIT, objects-before-head ordering, rollback, and fresh
  ambiguous-outcome reconciliation.
- Keep `materialize_exact(root, destination)` distinct from a latest-following
  projection.
- Use one bounded latest target, one private in-flight output, atomic publish,
  and complete fallback.
- Reuse the proven benchmark-private APFS clone/patch primitive for a
  LayerFS-owned seed.
- Keep K64/F64 in the admitted path and CD32-64/Xet-style grouping shadow-only.
- Repair H11 before using it as history, concurrency, or GC authority.

### Correct

1. **SQLite resilience is not CAS authentication.** SQLite provides the
   selected transactional and structural storage behavior. It does not prove
   `ObjectId == hash(canonical bytes)`, mapping topology, transition semantics,
   rollback freshness, or integrity of a separate native seed.
2. **The threat-model change must be explicit.** An edit may succeed while an
   unrelated, unvisited object is already corrupt. The corruption must still
   fail with the exact existing error when that object is read or when the
   explicit full verifier runs.
3. **Do not launder a trusted scope into a verified permit.** The current
   `SameOpenValidationPermit` proves that a complete closure scrub happened;
   it covers equal edges and can authorize a carried verified witness after
   COMMIT. A trusted edit needs a distinct transaction-local scope and distinct
   `trusted_assumed_*` counters. It must not set verified carry-forward in the
   first candidate.
4. **Keep the current receipt/schema in the first candidate.** Bypassing the
   closure scrub does not require a schema, format, sidecar, receipt, or write-
   shape migration. In trusted mode, the receipt is internal consistency and
   fencing evidence; it is not advertised as cross-reopen freshness authority.
   Because receipt v1 has no closure-provenance bit, a later verified open must
   always scrub before issuing a verified permit, including after a trusted
   commit.
5. **Separate projection service time from end-to-end latency.** The observed
   `2.877 ms` clone and `4.104 ms` sparse patch are prepared projection
   primitives. A request that first performs an `8.043 ms` canonical edit and
   then projects it has approximately additive latency, even if the two stages
   improve steady-state throughput by pipelining.
6. **Bound the coalescing claim.** One in-flight build and one latest target
   bound live queue space, not total work. A long stream may still start many
   builds. Coalescing reduces work only when requests outrun the worker; every
   started/published/cancelled/failed build and wasted byte must be observed.
7. **Never overwrite uncaptured user edits.** The asynchronous destination is
   a LayerFS-owned disposable projection. Export to a caller-owned path remains
   exact and explicit.
8. **Do not persist native-seed authority in the first candidate.** SQLite
   cannot authenticate a separate mutable file merely because its root is
   recorded in a row. Start with one process-lifetime, root-bound read-only
   descriptor for the private active projection. Persistent cross-process seed
   custody is a later one-variable experiment if measured hit rate requires it.
9. **Do not add GC machinery for a GC that does not exist.** Objects remain
   append-only during these lanes. Destructive GC is forbidden while the
   projection experiment is active; pins become necessary only before deletion
   is implemented.
10. **Treat performance arithmetic as a hypothesis.** `154 ms -> 8-12 ms` and
   `4-7 ms` are modeled expectations until a direct adjacent candidate/control
   campaign measures the complete operation.

### Reject or defer

- Deleting the verified path or making trusted mode an implicit default.
- Returning canonical bytes without checking the fetched bytes against the
  requested ObjectId.
- Accepting caller-supplied ObjectIds without hashing the supplied canonical
  bytes.
- Removing the transaction boundary, expected-head check, head-last publish,
  or ambiguous-COMMIT reconciliation.
- Treating `PRAGMA integrity_check` as a LayerFS semantic verifier.
- Claiming rollback protection from the SQLite file alone.
- A persistent seed table, seed per revision, second writer transaction, or
  second publication COMMIT in the first implementation.
- An unbounded projection queue, worker pool, general scheduler, VFS, new
  dependency, cross-platform backend framework, or concurrent GC.
- Production integration, profile selection, format migration, G6, or WP5
  during G5 candidate work.

## 3. Target architecture

```text
                         authoritative state
edit request ──> canonical CAS/COW edit ──> SQLite COMMIT ──> canonical root
                        one writer              |
                                                +── return canonical success
                                                |
                                                v
                                      bounded projection service
                                  current + in-flight + latest only
                                                |
                             private clone/patch or streamed fallback
                                                |
                                      sync + atomic native publish
                                                v
                                         projected root
```

The public semantics must distinguish:

```text
canonical_root   newest successfully committed root
projected_root   root represented by the visible native projection
target_root      newest root requested from follow-latest
state            idle | building | current | failed
route            exact_clone | sparse_patch | full_fallback
```

`canonical_root == requested_root` means the edit is durable. It does not mean
the native projection is current. A caller needing a native read-after-write
waits for `projected_root == requested_root` or calls exact materialization.

## 4. Ordered implementation groups

Each group freezes its own source, diff, executable, fixtures, commands,
counters, equations, schedule, overhead rule, and retain/revise/revert rule
before producing a measured row. A later group starts only from the retained
result of the prior group.

### Fast but information-dense benchmark shape

Complex coverage is allowed; repeated setup is not. Every G5 runner follows
these rules:

- build each release executable once, outside the campaign wrapper;
- hash/qualify each source fixture, base database, sidecar, expectation set,
  and executable once during fail-fast preflight;
- keep one long-lived child per arm for stateful micro-operations instead of
  launching a process and hashing/preparing 100 MiB for every observation;
- prepare control/candidate stores and initial native state once per isolated
  sequence, then use the exact same deterministic operation log;
- on every operation verify root, transition, length, route, transaction/
  COMMIT count, counters, and terminal operation-local Q;
- perform full reconstruction/digest and full snapshot verification only at
  prospectively frozen checkpoints and sequence end;
- retain compact numeric per-operation timing/work sidecars plus one compact
  sequence record, not repeated explanatory JSON prose;
- use 1/10-MiB fixtures for the full semantic/fault matrix and 100 MiB only for
  mechanisms whose performance or scaling cannot be falsified smaller; and
- fail fast on the first semantic/resource/custody error while preserving the
  complete failed attempt.

The screen remains `<20 s` complete wall. The final integrated campaign remains
`<=120 s` from lock acquisition through analyzers, cleanup, fsync, manifest,
and terminal read-only verification. These are total campaign budgets, not
per-row allowances.

The test ladder is equally incremental:

```text
touched focused tests
-> zero-row schedule assertion/dry-run
-> <20 s mechanism screen
-> one frozen-source workspace/clippy/fmt/diff closure
-> <=120 s measured gate
```

Do not rerun a passing full workspace/static closure while source is unchanged.
Runner/analyzer-only repairs rerun their focused checks and custody rather than
unrelated product suites. Error and cleanup semantics use the smallest fixture
that reaches the boundary.

### Minimal source and evidence touch map

- `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs`: add the
  store-lifetime integrity mode, distinct edit-base scope/provenance, trusted
  counters, direct G5 timers, and the stateful fast-row entry point. Extend the
  existing witness/count-change/publication tests rather than adding a second
  framework.
- `crates/layerfs-engine/src/bin/phase4_g3_materialization.rs`: add only the
  private active-projection state, seed rotation, one worker/one pending target,
  bounded hint composition, and service counters. Reuse existing clone, patch,
  fallback, sync, atomic publish, reconciliation, and cleanup functions.
- `crates/layerfs-core/src/validation.rs`: no receipt codec or schema change in
  the first candidate.
- `implementation-detail/phase-4/experiments/g5-foundation-h11/`: preserve v1/
  v2 and create one new versioned H11 repair attempt.
- new versioned G5-1 and G5-2 experiment folders: preregistration, dry-run
  schedule assertion, runner, raw compact records/timing sidecars, primary and
  independent analyzers, manifest, and terminal audit.

No production API module, canonical profile, database table, dependency,
worker pool, VFS, or SDK integration changes in these benchmark-private lanes.

Reuse and extend these existing focused tests:

- `same_open_witness_requires_full_scrub_and_is_exactly_single_use`;
- `count_change_proof_matches_full_shadow_and_carries_same_open_authority`;
- `cow_edits_reject_a_corrupt_parent_mapping_before_rewrite`;
- `witnessed_changed_spine_authenticates_all_differences_before_commit`;
- `publication_faults_record_reconciliation_and_require_private_authority`;
  and
- `profile_rejection_is_read_only_and_authority_secrets_are_os_random`.

### G5-0 — repair the H11 authority

Objective: remove the known evidence blocker without changing the LayerFS
algorithm.

Use the preserved H11 v2 result only as diagnostic input. Create a new
versioned attempt that:

- charges the expected manifest string and parsed expectation vector;
- charges current and retained reachability sets, including node overhead by
  the frozen accounting rule;
- charges timing vectors, transient formatting, and the final report;
- returns reachability high-water to the whole-harness maximum;
- drops every owned capacity before reporting terminal Q zero;
- emits the selected historical root/transition/output tuples so both
  analyzers can recompute them;
- consumes the operation log or removes it from the claimed execution
  authority;
- splits preflight, SQLite connection open/profile initialization, and head
  lookup timers instead of presenting incomplete SQL counters as complete;
- verifies lock inode/token ownership before release; and
- fsyncs the result directory and all terminal custody artifacts.

Fast screen: the existing deterministic 1-MiB, N=1/10/100/1,000 schedule,
complete wall `<20 s`.

Exit:

- `PASS` only if identity, work, historical tuples, storage, exact whole-
  harness Q, RSS, cleanup, lock, timers, custody, and two recomputations pass.
- Otherwise preserve the attempt and repair only the exact remaining blocker.
- No GC or general concurrency implementation starts here.

### G5-1 — trusted reopen/edit candidate

Objective: make the first edit after reopen operation-local under the explicit
`TrustedLocalDev` contract while leaving the verified path byte-for-byte
available as the control.

Smallest implementation:

1. Add one benchmark-private store-lifetime mode selected at `Store::open`;
   verified remains the default. Do not allow per-operation mode switching on
   one open Store.
2. Reuse current store preflight, profile, authority sidecar, visible-head
   decode, transition verification, expected-head check, writer transaction,
   publication, and reconciliation.
3. Add a separately named, single-use, transaction-local trusted edit scope.
   The minimum shared representation may be an explicit
   `EditBaseScope::{Verified, Trusted}` only if the provenance remains present
   through construction proof, equal-edge accounting, publication, and report.
   Do not mint or report a `SameOpenValidationWitness` as authenticated unless
   a complete scrub actually occurred.
4. In trusted mode, establish the scope from the decoded current head and
   verified transition, re-read the head before mutation/publication as the
   existing fencing path requires, and omit only `scrub_file(current)` and
   `scrub_file(parent)`.
5. Leave object fetch/hash validation and new/incumbent object rules in their
   shared functions; do not add bypasses at individual callers.
6. A trusted proof never sets verified carry-forward. A later same-open trusted
   edit creates a fresh trusted transaction scope from the current head; a
   verified Store always performs its required scrub.
7. Retain a named foreground
   `verify_snapshot_closure(current_head, explicit_retained_roots)` operation.
   It verifies meta/profile/sidecar, the head/receipt tuple, transitions, and
   every object reachable from the listed roots. Unreachable object-table rows
   are outside this operation; a future all-row CAS audit is a separate API.
8. Report the provenance as `trusted-local-unverified-closure`, with exact
   zero closure-scrub calls/bytes and separate touched-object authentication.

No schema, canonical format, receipt encoding, SQLite profile, SQL write
shape, transaction count, COMMIT count, worker, cache, or materialization
change belongs in this group.

Keep the trust branch out of shared object access. `put_authenticated`, `get`,
`get_bytes`, `with_borrowed_bytes`, batched leaf fetch, receipt decode, and
COMMIT/reconciliation remain unconditional. The only mode branch belongs at
edit-scope establishment and provenance-aware equal-edge accounting.

#### G5-1 focused semantics

- verified mode still performs and reports the full closure scrub;
- trusted mode performs zero full-closure scrub work;
- both modes produce exact same child root, transition, canonical objects,
  database state, and errors for healthy inputs;
- a corrupted object fetched by the edit still fails with the exact error;
- a corrupt unrelated object demonstrates the declared difference: trusted
  edit may commit, later access/full verification must fail exactly;
- malformed/wrong-role mapping, transition mismatch, expected-head conflict,
  ordinary error rollback, lost COMMIT acknowledgement, and reconciliation
  remain unchanged;
- offline rollback to an older internally valid complete store is accepted and
  labeled `NotProtected`. This applies to both modes without an external
  monotonic authority: a verified scrub can prove old content is internally
  valid, but cannot prove it is the latest state;
- DB-only rollback with the current sidecar, DB-plus-sidecar rollback, and an
  old internally valid head/receipt replay have explicit expected results;
- missing/wrong-size/wrong-mode/symlink/replaced authority sidecars and a
  wrong-store database keep their exact existing open errors. The trusted
  contract treats DB path custody as a nonadversarial environmental
  precondition; plain SQLite path open is not claimed as same-UID/TOCTOU
  protection;
- no trusted permit, descriptor, receipt classification, transaction, journal,
  temp, or Q residue remains.

Exact error classifications remain:

| Boundary | Required result |
|---|---|
| Missing/unavailable authority sidecar | `ValidationAuthorityUnavailable` |
| Invalid head tuple/receipt authenticator | `InvalidValidationReceipt` |
| Missing accessed object | exact `MissingObject(id)` |
| Fetched canonical/ObjectId mismatch | `IdentityMismatch` |
| Wrong canonical role | `WrongLogicalRole` |
| Mapping length/topology mismatch | existing exact typed error |
| Head changes before publication | `PublicationConflict` |
| Unresolved post-COMMIT state | `AmbiguousDurability` |

SQLite page corruption may surface as a SQLite corruption/I/O error; it is not
relabeled as a CAS identity error. Touched-object corruption fails before
COMMIT in both modes. Unrelated corruption is the declared policy difference:
verified mode fails during scrub, trusted mode may commit, and later access or
explicit verification fails exactly.

#### G5-1 counter hypothesis

For each matched edit:

```text
trusted_complete_closure_scrub_calls       = 0
trusted_complete_closure_scrub_bytes       = 0
trusted_touched_object_authentication      = observed operation-local work
control_complete_closure_scrub_bytes       > 0
candidate root/transition/work mutation    = control exactly
candidate transactions                     = 1
candidate publication COMMITs              = 1
terminal Q                                 = 0
```

Timer equation:

```text
first_edit_after_reopen
  = store_preflight
  + sqlite_open_and_profile
  + visible_head_and_transition
  + edit_construction
  + canonical_publication_commit
  + optional_reconciliation
```

Every component is reported; reconciliation remains a separate conditional
timer and is never inferred from wall time.

The retained direct same-count row supplies the removable-budget equation:

```text
154.019083 ms total
  = 3.726500 ms reopen/head
  + 143.041917 ms full authority establishment
  + 5.068333 ms canonical edit/mapping
  + 0.133917 ms precommit proof
  + 2.048416 ms COMMIT

optimistic trusted model
  = 154.019083 - 143.041917
  = 10.977166 ms
```

Authority is `92.873%` of this row and authenticated 5,375 objects/
105,122,778 canonical bytes. The subtraction is a causal budget, not a measured
G5 result, because small trusted head/transition work remains.

Count-changing edits have different direct controls:

```text
early +1 first after reopen   = 248.491541 ms
middle +1 first after reopen  = 244.305666 ms

modeled trusted early  = 3.583 + 5.108458 = 8.691458 ms
modeled trusted middle = 3.583 + 4.576000 = 8.159000 ms
```

These models imply approximately `28.6x/30.0x`, but they combine separately
measured retained components and require direct adjacent G5 rows.

#### G5-1 performance screen and gate

Screen `<20 s`:

- 1-MiB and 10-MiB identity/error sentinels;
- one 100-MiB first-post-reopen same-count edit;
- one 100-MiB first-post-reopen middle `+1` edit;
- exact matched control/candidate work and a protected G4 create/range smoke;
- no production claim.

Gate `<=120 s`:

- use only the operations the retained harness implements:
  `first-edit-after-reopen`, `same-middle`,
  `one-byte-{early,middle,late}`, and `plus1-{early,middle}`;
- run adjacent balanced verified/trusted rows from byte-identical isolated
  copied bases; do not invent cumulative `+1 late` or `-1` semantics;
- retain at least 20 matched observations for the primary
  `first-edit-after-reopen` class so p50/p95 are meaningful, and five adjacent
  pairs for each remaining supported shape; report every sample;
- report `open + authority + capture` for every edit, even where the historical
  row reported durable capture separately;
- batch repeated sub-10-ms candidate observations inside one child after one
  frozen preflight; do not pay process launch, 100-MiB source hashing, or base
  generation once per micro-observation;
- use full verification only at prospectively frozen checkpoints and the end,
  with exact expected root/transition checks on every operation;
- retain per-operation timings in a compact numeric sidecar for primary and
  independent recomputation.

Use two comparisons, not one:

1. frozen G4 verified executable versus G5 executable in verified mode, to
   measure instrumentation/implementation overhead and protect the original
   path; and
2. verified versus trusted mode inside the same G5 executable, to isolate the
   trust-policy mechanism.

Prospective success targets:

```text
paired median improvement over verified control >= 50%
first-post-reopen mixed-batch p50             <= 15 ms
first-post-reopen mixed-batch p95             <= 25 ms
```

The targets do not waive any semantic, durability, identity, error, resource,
or custody gate. A healthy semantic PASS with weaker speed is `REVISE`, not a
false performance PASS.

### G5-2 — bounded warm projection candidate

Objective: make canonical success independent of native projection freshness,
reuse the accepted G4 clone/patch/publication primitives, and prevent rapid
edits from creating unbounded pending state.

The retained 100-MiB primitives already sit close to their current publication
floor:

| Primitive | Mean | Qualification | Clone/fetch/patch | Data sync | Rename publication | Other |
|---|---:|---:|---:|---:|---:|---:|
| Exact clone | `2.876729 ms` | `0.034 ms` | `0.274 ms` | included in other | `2.533 ms` | about `0.036 ms` |
| One-byte sparse | `4.104042 ms` | `0.042 ms` | `0.967 ms` | `0.129 ms` | `2.903 ms` | about `0.063 ms` |

Rename/publication is approximately 88% of exact clone and 71% of sparse
service wall. The worker is therefore a responsiveness/throughput/coalescing
mechanism, not a plausible 2x reduction of the 4.1-ms primitive. The retained
operation timers include qualification, clone/fallback, patch, sync, metadata,
rename, directory sync, reconciliation when needed, and cleanup. Their
100-MiB verification reread is outside `operation_total_ns` and remains
reported separately.

Implement only after G5-1 is frozen:

1. Keep exact export/materialization synchronous and root-specific. Exact jobs
   are never coalesced or silently replaced.
2. Add one benchmark-private latest-following projection service for one
   LayerFS-owned disposable destination.
3. Use one worker, one mutex/condition variable, one in-flight target, and one
   pending latest target. Replacing or merging the pending target increments
   exact request/coalescing counters; no queue or pool exists.
4. Use the private active projection as the next seed: retain an exact
   root-bound read-only descriptor, clone it into one private successor, and
   rotate the seed only after successful publication. Normal service-owned
   native state is one active projection; transient state is active plus
   successor. This avoids a separate seed plus visible output plus temporary
   `3L` shape.
5. Use the existing exact clone, same-size sparse patch, sync, atomic rename,
   lost-ack reconciliation, and streamed full fallback. Do not duplicate these
   mechanisms in a new framework.
6. Give the worker a read-only SQLite connection or the existing serialized
   read route; it performs zero writer transactions and zero COMMITs. Measure
   foreground writer blocking before choosing between them.
7. Keep the first candidate simple: do not cancel a valid in-flight build.
   New requests merge into the single pending target. This preserves a
   continuous base for patching and avoids merging a cancelled in-flight proof
   back into pending. Cancellation remains an explicit fault/shutdown path
   while output is private; exact builds never cancel.
8. On restart, remove owned abandoned temporaries, read the canonical head, and
   rebuild on demand. The first candidate persists no seed and no projection
   metadata in SQLite.
9. Forbid destructive GC and caller-owned editable destinations in this mode.

No append/truncate specialization is added until the exact/same-size service
passes. Count-changing projection uses the complete fallback and is reported
as such.

Pending same-size state needs more than a latest root ID. Each successful
canonical edit supplies an internal, move-only hint containing parent root,
target root, exact length class, and exact dirty ranges. The service accepts a
hint only when its parent equals the current in-flight/pending chain tip. It
merges overlapping/adjacent ranges into a fixed-capacity range vector. A chain
gap, count change, range-count cap, or dirty-byte cap sets the pending route to
`FullFallback` and discards patch detail. It must never apply an `R2 -> R3`
patch to an R1 seed.

Initial prospective bounds are 256 coalesced ranges, 8 MiB total dirty bytes,
and a 1-MiB streaming fetch/write buffer. The range vector itself is exactly
charged. These values admit the 100-random-byte gate without making arbitrary
large scattered edits an unbounded sparse route; they are frozen before the
first screen and are not raised after observation.

After a successful `R1 -> R2` publication, seed rotation is:

```text
open private R2 successor read-only
-> verify descriptor identity, mode, length, and bound root
-> atomically publish R2
-> install R2 as active projection/seed
-> release R1
```

Report `seed_rotations`, roots before/after, descriptor acquisitions/releases,
and rotation failures. A rotation failure may leave exact R2 visible but marks
the warm route unavailable; R1 must not be reused as the base for R3.

#### G5-2 state and fault semantics

- canonical COMMIT success is returned even when projection later fails;
- projection failure retains the last complete published projection and exact
  error/status;
- a stale worker token can never publish over a newer target;
- cancellation removes only the private temporary output;
- before-sync, before-rename, after-rename/lost-ack, clone failure, seed
  substitution, missing seed, wrong kind, symlink, and process-restart cleanup
  preserve exact old-or-new visibility;
- an exact request for `R2` completes as `R2` even if latest-following advances
  to `R3`;
- a latest burst `R1 -> R2 -> ... -> Rn` keeps at most one in-flight and one
  pending target at one instant; report requested, started, completed,
  cancelled, failed, stale-completed, published, coalesced, and wasted bytes;
- terminal worker, descriptor, buffer, temp, seed, Q, and pending-target counts
  are zero after shutdown.

The one-slot conservation equations are hard:

```text
requests = coalesced_before_start + builds_started
builds_started = published + cancelled + failed + stale_completed

terminal pending_targets = 0
terminal inflight_builds = 0
terminal projected_root = last_requested_root   # successful drain
terminal projection writer transactions/COMMITs = 0/0
```

Queue space is `O(1)`. Total build work remains `O(J * projection_work)` in
the worst arrival pattern because a long stream can let the worker finish many
intermediate builds. Coalescing reduces work only when arrivals outrun the
projection stage; it is not a universal Big-O time improvement.

#### G5-2 timer and throughput reporting

Record five exact event timestamps:

```text
t0 = edit request received
t1 = canonical COMMIT acknowledged or reconciled requested-visible
t2 = projection target enqueued
t3 = worker starts the selected target
t4 = native publication acknowledged or reconciled requested-visible
```

Derived intervals:

```text
edit_to_commit_ack          = t1 - t0
dispatch_to_enqueue         = t2 - t1
queue_wait                  = t3 - t2
projection_service          = t4 - t3
projection_request_visible  = t4 - t2
edit_request_visible        = t4 - t0

edit_request_visible
  = edit_to_commit_ack
  + dispatch_to_enqueue
  + queue_wait
  + projection_service
```

Projection service retains its own exact equation:

```text
projection_service
  = qualification
  + clone_or_fallback
  + canonical_patch_fetch
  + patch_write
  + data_sync
  + metadata
  + metadata_sync
  + rename
  + directory_sync
  + reconciliation
  + cleanup
  + unattributed
```

For a sustained pipeline, throughput is bounded by the slower stage, but
single-operation latency remains the sum. Do not report the `2.877 ms` clone or
`4.104 ms` patch as complete edit-to-native latency.

Current isolated same-size stage ceilings are:

```text
serial:   1000 / (8.043 + 4.104) = 82.3 projected edits/s
pipeline: 1000 / max(8.043, 4.104) = 124.3 projected edits/s
```

The `1.51x` pipeline ratio is an ideal upper bound before SQLite locking, CPU,
seed rotation, dispatch, and queueing. It is not a throughput claim.

#### G5-2 performance screen and gate

Screen `<20 s`:

- 1/10-MiB state, coalescing, cancellation, and fault cases;
- one 100-MiB exact-root clone;
- one 100-MiB one-byte same-size patch;
- one short 10-MiB precommitted-root enqueue storm that deterministically
  outruns the worker;
- one 100-MiB final count-change fallback;
- one foreground edit while the worker reads, plus protected G5-1 first-reopen
  edit and G4 full-fallback sentinels.

Gate `<=120 s`:

- exact clone and same-size sparse patch at 1/10/100 MiB;
- 64 deterministic same-size 100-MiB edits with every root required;
- 100 deterministic same-size edits with latest-following, treated primarily
  as a no-lag throughput test because the `4.104 ms` projection stage is
  already faster than the `8.043 ms` canonical producer;
- one forced queue-pressure count-changing storm over precommitted roots,
  followed by a 100-MiB final fallback sentinel. This is the primary
  coalescing test because `329.237 ms` projection is roughly 72x slower than a
  `4.576 ms` middle `+1` canonical edit;
- exact old/current root reads while projection is behind;
- foreground edit plus worker-read contention;
- frozen fault/cancellation cases on small fixtures;
- primary and independent recomputation.

Prospective operation targets:

| Operation | Target | Meaning |
|---|---:|---|
| Exact-root projection service p50 / p95 | `<=5 / <=8 ms` | Prepared process-lifetime seed hit |
| Same-size sparse projection service p50 / p95 | `<=6 / <=10 ms` | Clone, authenticated patch, sync, publish |
| Same-open edit-to-projected p50 / p95 | hard `<=18 / <=30 ms`; strong `<=15 / <=25 ms` | Canonical edit plus projection lag |
| Trusted first-reopen edit-to-projected p50 / p95 | hard `<=22 / <=35 ms`; strong `<=18 / <=30 ms` | Reopen, trusted edit, queue, projection |
| Final count-change convergence after last commit | strong `<=400 ms` | Final full fallback after forced queue pressure |
| Latest-following pending targets | `<=1` | Hard bound, excluding one in-flight build |
| Individual buffer | `<=1 MiB` | Existing G4 bound |
| Projection SQLite writes / COMMITs | `0 / 0` | Canonical commit remains the only writer publication |

RSS is measured for the complete foreground-plus-worker process. It may not be
compared with G4's single-thread result without labeling the changed execution
shape. Freeze combined peak RSS at `<=32 MiB` hard with a `<=24 MiB` strong
target; retain the `<=1 MiB` individual buffer, one active projection, one
temporary successor, one worker read connection, and terminal-zero bounds.
Report active/temp logical, apparent, and allocated bytes and any
reader-retained old generation. Never infer unique physical bytes from APFS
clone allocation and never waive memory/storage growth merely because wall
time improved.

The seed is process-lifetime in this candidate. A Store reopened inside the
same service may hit while the descriptor survives; a fresh LayerFS process
has no qualified seed and uses full fallback. Record seed admission wall/bytes,
hits, misses, rebuilds, rotations, and amortized hit cost. Persistent
cross-process reuse remains a separate later trust/custody experiment.

The worker's read-only SQLite connection is a direct risk under rollback
journal `DELETE`. It must not hold one explicit read transaction across a full
reconstruction. Report foreground transaction/COMMIT wall, worker
query/row/BLOB work, and Busy/Locked events. Unexpected Busy or Locked is a
hard failure; do not rescue the candidate with WAL, a profile change, retries,
or a worker pool.

### G5-3 — history/concurrency closure

Objective: use the corrected H11 authority to prove that current operations and
bounded projection remain independent of retained history, then stop before
destructive concurrency that has no product need.

Required:

- current-root head/range/edit/reconstruction work versus 1/10/100/1,000
  retained revisions;
- the exact storage slope per unique revision;
- multiple readers of immutable roots while one canonical writer commits;
- worker/read connection contention and writer progress;
- branch/revert and selected historical exact materialization;
- bounded service shutdown, cancellation, descriptor cleanup, and process
  restart reconciliation;
- explicit retained roots and a read-only reachability audit.

Stop there unless deletion is a current product requirement. If G5 includes
GC, start with one exclusive stop-the-world mark/sweep only after retained-root
and reader/projection pin semantics are frozen. Mark failure means no sweep.
Concurrent GC remains deferred.

## 5. Verification beyond the primary optimization targets

The current create/edit/read/reconstruction/materialization matrix is a strong
foundation, but it is not an implicit performance proof for a different
operation shape, size, cache state, process boundary, or concurrency state.

The selection rule is:

```text
changed trust/state/concurrency mechanism -> verify now
same shared implementation path           -> retain a compact regression sentinel
new operation or product surface          -> optimize later in its own lane
```

### 5.1 Mandatory in G5-1/G5-2

These operations directly exercise the new trust boundary or projection state
and therefore cannot be deferred.

| Verification family | Required cases | Why now |
|---|---|---|
| Trusted/verified transition | trusted edit -> close -> verified reopen; several trusted edits -> verified reopen; reject per-operation mode switching | Receipt v1 has no closure-provenance bit |
| Touched/unrelated corruption | touched missing/mismatched/wrong-role object; unrelated corrupt object; later access and explicit scrub | Defines the exact semantic weakening |
| Rollback/substitution labels | old DB/current sidecar; old DB/old sidecar; old head/receipt replay; wrong-store DB; wrong/missing/symlink sidecar | Neither mode proves newest-state freshness |
| Successive seed rotation | `R1 -> R2 -> R3 -> R4`, descriptor acquisition/release, rotation failure | One fast patch does not prove a reusable warm system |
| Pending-chain composition | contiguous hints, overlapping ranges, chain gap, count change, range/byte caps | Prevents applying an `R2 -> R3` patch to R1 |
| Projection lag API | direct root read while native state is stale; exact request; wait success/timeout; worker failure | Canonical COMMIT no longer means native freshness |
| SQLite contention | foreground edit/COMMIT during sparse and full worker reads; Busy/Locked counters | A second reader under DELETE journaling can block the writer |
| Native faults | sync/rename/directory-sync/lost-ack/substitution/cancel/restart cleanup | The worker adds new old-or-new boundaries |
| Repeated resources | 100/1,000 enqueues, Q/RSS/buffer/range/descriptor/temp/native storage high-water | A one-row measurement does not prove bounded service state |

Exact trusted/verified corruption expectations are:

| Case | Verified | TrustedLocalDev |
|---|---|---|
| Corrupt object touched by edit | fail before COMMIT | fail before COMMIT |
| Missing/wrong-role mapping on changed path | exact typed failure | exact typed failure |
| Corrupt unrelated object | eager scrub fails | edit may commit |
| Later access to unrelated corrupt object | exact typed failure | exact typed failure |
| Explicit snapshot verification | fail | fail |

Projection status must remain meaningful while behind:

```text
canonical_root = R8
projected_root = R5
target_root    = R8
state          = building
```

During that state, an authenticated direct range read from R8 must work, an
exact R5/R8 request must retain its requested-root semantics, and canonical R8
must remain successful even if native projection fails.

### 5.2 Required before G5 closes

These belong primarily to G5-3. They need verification before G5 closure but
do not belong in the first mechanism screen.

| Operation family | Required evidence | Optimization status |
|---|---|---|
| Retained history 1/10/100/1,000 | current head/range/edit/reconstruction work and storage slope | verify only |
| Same-byte hotspot/random edits | identity reuse, storage growth, latency distribution | verify only |
| A/B alternation and revert | incumbent reuse and exact old/current roots | verify only |
| Historical range/exact materialization | requested root is never replaced by latest | verify only |
| Multiple readers plus one writer | immutable snapshot reads and foreground writer progress | verify only |
| Two requests for one projection target | exactly one projection writer | verify only |
| Active shutdown/restart | drain/cancel semantics and owned cleanup | verify only |
| Read-only reachability audit | stored/current/retained graph accounting | verify only |
| Branch/revert history | retained-root semantics | verify only |
| Complete-store backup/restore | reopen correctness and explicit rollback label | verify only |

### 5.3 Deliberately delayed lanes

These operations remain important, but adding them now would stack another
algorithm, authority model, or product surface onto G5's two causal changes.

| Deferred lane | Why it can wait | What current G5 may claim |
|---|---|---|
| Append/truncate specialization | New projection routes and equations | fallback correctness only |
| Arbitrary middle insert/delete native output | Flat output remains suffix-sensitive | fast canonical COMMIT plus eventual fallback |
| Persistent cross-process seed | New restart/custody/rollback authority | process-lifetime warm only |
| Multi-file/directory projection | Needs generation-directory atomic publication | single private file only |
| Destructive/concurrent GC | Requires retained roots and reader/projection pins | append-only/read-only reachability |
| True controlled-cold/device I/O | Current evidence is warm or warm-unknown | no physical/cold claim |
| 500-MiB and multi-GiB scale | 100 MiB cannot prove suffix/linear scaling away | no scale-independent claim |
| Malicious same-UID/arbitrary SQL | Outside `TrustedLocalDev` | nonadversarial private-local custody only |
| Cross-platform backend | macOS/APFS mechanism first | universal streaming fallback only |

Append and truncate are promising after a qualified active seed:

```text
append   = clone parent + authenticated tail write + publish
truncate = clone parent + ftruncate + publish
```

They still receive separate prospective benchmarks before a performance claim.
General middle insertion/deletion of a flat native file retains an
`Omega(file_size - edit_offset)` movement/output lower bound even if the
canonical edit is fast.

### 5.4 What evidence may and may not transfer

An invariant can transfer only when the exact shared function and direct
counters prove that every consumer takes the same path.

| Tested fact | Reasonable inference | Not implied |
|---|---|---|
| Trusted edit reports zero closure-scrub calls | Other edits using the same trusted scope also skip that scrub | Their total latency |
| Shared publisher reports one transaction/COMMIT | Other callers routed through it keep that publication shape | Their write bytes or COMMIT wall |
| Shared fetched-object functions always validate ObjectId | Every fetched object is authenticated | Unvisited objects are healthy |
| Pending state has one slot | Queue memory does not grow with request count | Total projection work is O(1) |
| Individual buffer is structurally <=1 MiB | That owned buffer is file-size independent | RSS, SQLite cache, or storage is independent |
| Current traversal does not enumerate retained roots | Direct graph work is history-independent | SQLite locality/cache is unaffected by history |
| Exact clone succeeds on a qualified seed | Other qualified exact hits may reuse the primitive | Fresh-process hit or seed-miss speed |

The following substitutions are prohibited:

```text
one-byte sparse speed       != multi-range/append/truncate/count-change speed
warm seed speed             != cold or fresh-process materialization speed
100-MiB speed               != 500-MiB scale independence
same-size edit locality     != count-changing suffix locality
canonical COMMIT success    != native projection freshness
O(1) pending queue          != O(1) total projection work
SQLite integrity            != CAS identity or rollback freshness
one concurrency sentinel    != arbitrary concurrent load
```

## 6. Protected operation matrix

Every retained G5 candidate must preserve the accepted G4 results within the
prospectively frozen dual relative-and-absolute materiality rule.

| Protected operation | Current authority | G5 expectation |
|---|---:|---|
| 100-MiB durable full create | `279.463 ms` | No material change |
| Same-open same-count edit | `8.043 ms` | No material regression |
| Same-open early/middle `+1` | `5.108 / 4.576 ms` | No material regression |
| Reopen/head | `3.583 ms` | No material regression |
| First same-count edit after reopen | `154.019 ms` | optimistic model `10.977 ms`; target `<=15 ms` p50 |
| First early/middle `+1` after reopen | `248.492 / 244.306 ms` | modeled `8.691 / 8.159 ms`; direct evidence required |
| Authenticated returned 1-MiB range | `2.046 ms` | No material regression |
| Warm/fresh canonical reconstruction | `237.214 / 237.381 ms` | No material change |
| First/full native materialization | `307.652 ms` | Still the miss/fallback path |
| Prepared exact-root clone | `2.877 ms` | G5-2 service target `<=5 ms` p50 |
| Prepared one-byte incremental projection | `4.104 ms` | G5-2 service target `<=6 ms` p50 |

The protected rule from the G5 fast-iteration contract remains:

```text
candidate_sum * 100 > control_sum * 105
AND candidate_sum - control_sum >= 2,000,000 ns
```

It decides latency regression only. Exact identity, topology, bytes, errors,
work, transaction/COMMIT counts, durability, reconciliation, resource bounds,
cleanup, chronology, custody, and analyzer agreement remain hard gates.

## 7. Expected improvements and honest limits

| Path | Current | G5 expectation | Status |
|---|---:|---:|---|
| First early/middle count change after reopen | `248.492 / 244.306 ms` | modeled `8.691 / 8.159 ms` (about `28.6x / 30.0x`); gate target `<=15 ms` p50 | Separately measured arithmetic; direct candidate required |
| First same-count edit after reopen | `154.019 ms` | exact lower-bound model `154.019 - 143.042 = 10.977 ms` (`14.0x`); gate target `<=15 ms` p50 | Direct component arithmetic; candidate unmeasured |
| Exact warm native projection service | `2.877 ms` primitive | `3-5 ms` | Primitive observed; service unmeasured |
| Sparse warm native projection service | `4.104 ms` primitive | `4-6 ms` | Primitive observed; service unmeasured |
| Same-size edit through projected native state | about `8.043 + 4.104 ms` before dispatch overhead | `12-15 ms` class | Additive latency; pipeline may raise throughput |
| Trusted first-reopen edit through projected native state | about `10.977 + 4.104 ms` before dispatch/queue | `15-20 ms` class | Lower-bound model; candidate unmeasured |
| Rapid latest-following bursts | potentially one projection per root | one in-flight/one pending space; arrival-dependent work reduction | Worst-case total work remains linear in requests |
| Full/fresh native materialization | `307.652 ms` | approximately unchanged | Still `Theta(file bytes)` |
| Full create | `279.463 ms` | approximately unchanged | Still `Theta(new bytes)` |
| Canonical reconstruction | about `237 ms` | approximately unchanged | Still emits all bytes |
| Arbitrary middle count-changing native projection | `329.237 ms` fallback class | commit returns quickly; final projection remains suffix/full-work sensitive | No universal sub-10-ms claim |

### 7.1 Honest asymptotic changes

Let `L` be file bytes, `N` file-reference occurrences, `S` affected suffix
references, `Delta` changed bytes, `W` the bounded rejoin/search window
(`<=1 MiB` in the retained path), `H` mapping height, `R` retained revisions,
and `J` projection requests.

| Operation | Current verified path | G5 path | Conclusion |
|---|---|---|---|
| Reopen/head | fixed head/receipt records | same | unchanged |
| First same-size edit after reopen | `Theta(L + N)` scrub + `O(Delta + W + H)` | `O(Delta + W + H)` trusted-local | large hot-path change under weaker trust semantics |
| First count-changing edit after reopen | `Theta(L + N)` scrub + `O(S + H)` | `O(S + H)` trusted-local | scrub removed; suffix work remains |
| Worst-case count-changing edit | `Theta(N)` | `Theta(N)` | unchanged K64/F64 limit |
| Explicit snapshot verification | `Theta(L + N)` | `Theta(L + N)` | retained outside hot path |
| Full create | `Theta(L + N)` | same | unchanged |
| Canonical reconstruction | `Theta(L + N)` | same | unchanged |
| First/full native materialization | `Theta(L + N)` plus output/sync | same | unchanged |
| Exact APFS clone projection | filesystem-defined clone cost | same primitive | observed near-constant; no portable hard `O(1)` claim |
| Sparse warm projection | clone cost + touched authentication/`O(Delta)` writes | same primitive behind service | already fast; scheduling does not improve primitive Big-O |
| Count-changing projection fallback | `Theta(L + N)` | `Theta(L + N)` for each build | unchanged per final projection |
| Latest pending state | not present | `O(1)` | genuine space improvement |
| Latest total build work | up to `O(J * L)` | still `O(J * L)` worst case | coalescing is arrival-dependent |
| Current-root work versus history | intended independent of `R` | same | H11 must pass exact Q before qualification |
| Append-only history storage | unique-revision bytes accumulated over `R` | same | no GC improvement in G5-1/G5-2 |

The first-edit improvement is operation-local work under a deliberately weaker
trusted-local contract. It is not a same-semantics cryptographic algorithmic
win over verified mode.

G5's large win is removal of a policy-mandated complete scrub from the trusted
first-edit path and avoidance/coalescing of obsolete derived projections. It is
not a new sublinear algorithm for full create, full reconstruction, or a true
first materialization.

## 8. Terminal G5 decision rule

G5 may close only when:

1. H11 has a qualifying whole-harness evidence PASS.
2. `TrustedLocalDev` has a direct healthy-input identity/durability PASS, the
   declared corruption/rollback behavior is tested and labeled, and its
   first-post-reopen speed target passes.
3. Verified mode remains available and its protected controls pass.
4. The projection service has exact-vs-latest semantics, one-slot bounds,
   correct fault/cancellation behavior, zero extra SQLite writer COMMITs,
   bounded combined resources, and direct latency/throughput evidence.
5. Full create, same-open edits, range read, reconstruction, full fallback,
   storage, and exact errors have no protected material regression.
6. Both analyzers agree, all raw rows and failures are retained, terminal
   cleanup is exact, and the complete versioned manifest verifies read-only.

If any item fails, preserve the attempt as `REVISE`, identify the exact
blocker, repair the smallest shared cause, and rerun only the affected
versioned campaign. Do not weaken a gate after observation or stack an
unqualified next mechanism on top.

Even on PASS, G5 remains benchmark-private until a separate production/API
integration decision. G5 closure does not start G6 or WP5, select a new
canonical profile, enable destructive GC, or claim rollback-resistant
authentication.
