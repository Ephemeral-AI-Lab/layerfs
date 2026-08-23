# Phase 4 G5-1 v20 preregistration

V20 preserves v18's 200-row RSS REVISE. It removes the ineffective allocator
pressure-relief call and runs both frozen G4 and G5 product processes with
`MallocNanoZone=0`, prospectively reducing allocator arena residency without
changing product bytes, logical work, schedule, thresholds, or the 150-second
gate cap.

V20 preserves the v17 attempt-2 200-row REVISE. It retains the K64 scrub and
adds only two lifecycle/report repairs: chunked exact hex formatting and Darwin
allocator pressure relief after the report/request owners are dropped and Q is
zero. The prospective measured cap is 150 seconds; population, schedule,
performance thresholds, RSS/Q, trust, durability, and one-COMMIT gates are
unchanged.

Attempt 2 is one material product candidate after the preserved v16 200-row
REVISE. It changes only Verified full-scrub chunk reads from one authenticated
SQLite query per reference to the existing authenticated K64 batch primitive.
It preserves exact authenticated object/byte totals, current+parent scrub,
errors, trust, durability, 200 arms, 56 checkpoints, and all thresholds. The
G4/G5 work law requires the exact +65-byte/+1 authority read plus the exact
`R-ceil(R/32)` query reduction; mutation and durability work remain equal.

V20 preserves v19's 115.699891208-second RSS REVISE and reduces only the
scrub batch width from the K64 maximum to 32. This lowers the simultaneously
live SQLite statement/parameter footprint; K64 remains the hard upper bound.

Objective: make first edit after reopen operation-local only under the explicit
benchmark-private `TrustedLocalDev` contract while preserving Verified as the
default and byte-identical logical control behavior.

## Fixed architecture and threat boundary

The candidate remains CAS + CDC + COW + canonical K64/F64 + SQLite. V20 changes
no product schema, canonical bytes, receipt encoding, SQL write shape,
journal/profile, dependency, retry, worker, or production API. Trusted mode
assumes nonadversarial private-local path custody, provides no rollback
freshness, and may commit past unrelated corruption. Every fetched, supplied,
generated, or immutable incumbent object is authenticated exactly. Touched
corruption fails before COMMIT in both modes. Verified remains the default and
performs a complete closure scrub; Trusted never creates verified closure
provenance or verified carry-forward.

`verify_snapshot_closure(current_head, explicit_retained_roots)` independently
revalidates Store profile/authority/head receipt, each supplied transition, and
every object reachable from each supplied root. It performs no writes and does
not scan unreachable object-table rows.

## Fast law and durable zero-row disposition

The only admissible order is:

```text
focused tests
-> zero-row schedule assertion and calibrated dry-run
-> one complete <20-second screen
-> one frozen-source workspace/clippy/fmt/diff closure
-> one complete <=150-second gate
```

Any failure preserves v20. No observed row may be deleted, replaced, reordered,
or selectively rerun. A source or method repair creates v20.

The dry-run admits no product child, Store, row, measurement timer, or global
lock. It performs one labeled Python SHA-256 pass and one external `shasum`
pass over the frozen 100-MiB prepared master. Its conservative throughput is
no greater than half the slower observed implementation:

```text
floor_bytes_per_second = min(python_bps, external_shasum_bps) // 2
```

The same zero-row invocation performs exactly one versioned split wrapper
calibration over that prepared master. It starts no product, opens no Store,
dispatches no row, and acquires no global lock. First it measures exactly one
process-first read-only SQLite wrapper initialization on the selected sealed
master and charges four times that observation once. It then takes exactly
three post-initialization samples of the complete recurring wrapper path:
native clone, exact allowed-inventory evidence, constant-size published-visible
state (two query-only control PRAGMAs, one head row, three physical PRAGMAs,
and fixed schema-rootpage rows), sidecar hashes, the shared mutation-work hash
and wrapper-evidence assembly, evidence fsync, and immediate cleanup. The
recurring bound is four times the slowest of those three samples and the
forecast charges exactly `200 * recurring_bound_ns`. The schemas are
`phase4-g5-1-wrapper-calibration-intent-v20`,
`phase4-g5-1-wrapper-initialization-sample-v20`,
`phase4-g5-1-wrapper-calibration-sample-v20`, and
`phase4-g5-1-wrapper-calibration-result-v20`.
The zero-product sample's placeholder closure-sized field is classified
`CalibrationShapeOnlyNoProductParity`, and its reachable parity is
`NotClaimedCalibrationShapeOnly`; calibration never emits product/golden
provenance or becomes result authority.

Selection of the representative prepared master is itself frozen evidence.
The wrapper intent embeds plan schema
`phase4-g5-1-wrapper-calibration-plan-v20` and records the complete candidate
table, not only the winner. Every candidate binds the exact dominance tuple
`(file_count, total_manifest_bytes, directory_count, relative_path)`, and the
representative is the deterministic lexicographic maximum of that tuple.
Analyzers or later reconstruction may not infer a different tie-break or omit
a non-selected candidate.

The forecast also charges a prospective fixed `10,000,000,000 ns` campaign
allowance for global-lock custody, both analyzers, payload/final manifests, and
terminal/final verification. It is anchored by rounding up the retained H11
G5-0 terminal-audit observation `9,254,244,292 ns`; it is not a demonstrated
upper bound for finalizing a 200-row G5-1 campaign. The actual complete gate
wall `<=150 seconds` remains the sole terminal timing authority. The retained
evidence binds `campaign_finalization_inference` to
`ProspectiveAllowanceNotProvenUpperBound`. The complete-wrapper forecast is the
sum of retained timing evidence, the calibrated
external-hash component, the 200-arm wrapper-calibration component, and that
fixed allowance, plus the separately factor-four-charged one-time initialization.
The positive difference between 150 seconds and the forecast
is reported only as feasibility reserve; reserve is neither scheduled work nor
timing evidence and is never added to the forecast. Its exact classification is
`RemainingTimeNotWorkAndNotTimingEvidence`. Whether PASS or REVISE, the runner
must write and fsync the top-level `DRY-RUN-INTENT-v20.json` before invoking
either wrapper or hash calibration. One failure funnel covers every subsequent
step. Any exception—including failure before a wrapper result exists—must
write and fsync `DRY-RUN-FAILED-v20.json` first, then write and fsync a
`DRY-RUN-DISPOSITION-v20.json` with `status=REVISE` that binds the FAILED hash
and explicitly records absent optional artifacts. A success or calibrated
overrun writes and fsyncs complete `DRY-RUN-v20.json`, including both
calibrations, all byte/time components, the 200-arm assertion, and zero-
product/Store/row/lock counters; it then writes and fsyncs the disposition and
its directory. A summed
forecast above 150 seconds additionally writes and fsyncs
`PREMEASUREMENT-REVISE-v20.json` and stops before screen. A missing dry-run or
failure disposition is never treated as an authoritative PASS or reconstructed
from an earlier attempt.

V11 is byte-preserved and superseded by the retained
`v12/V11-SUPERSESSION-v12.json`. It binds the
complete v11 focused/readiness, input/method manifests, freeze/verification,
calibration/hash/dry/REVISE/disposition chain, zero rows/screen/gate/lock, the
`129.202697050 s` forecast, and the frozen three-record top-level/nested
calibration-semantics contradiction. V10 continuity remains bound through
`V10-SUPERSESSION-v11.json`.

V12 is also byte-preserved and superseded by the retained
`v13/V12-SUPERSESSION-v13.json`.
Its zero-product arithmetic reported `106.344946549 s` against `120 s`, but its
frozen post-run verifier rejected the top-level intent because the producer
omitted the required zero-valued `wrapper_initialization_samples_completed`
and `wrapper_recurring_samples_completed` fields. V12 therefore closed
`PREMEASUREMENT_REVISE` with zero product rows, screen, gate, or lock; its
timings are retained diagnosis only and cannot authorize screen.

V13 is byte-preserved and superseded by the retained
`v14/V13-SUPERSESSION-v14.json`. Its fresh
zero-row and post-run verifier passed at `106.287054720 s`, but the screen
preflight acquired the required global lock and then failed before result-root
creation or ordinal 1 because `verify_wrapper_calibration` treated the current
`LOCK.exists()` as contradicting the historical zero-lock calibration. The
lock-release record and renamed lock attestation are bound in the supersession;
v13 has zero product rows and no screen or gate result root.

V14 is byte-preserved by `V14-SUPERSESSION-v20.json`. Its zero-row passed, and
its screen reached one G5 Verified product row before the product reported
`first_edit_timer_equation_matches=false`: CompleteRoundTrip overwrote the
initial reopen aggregate with the post-COMMIT reopen while the equation still
used that field as the initial interval. The row, failure, cleanup, Q0, and lock
release are retained and never reused.

V20 changes the shared product timer equation and uses a fresh release. The
aggregate first-edit interval is initial Store preflight + SQLite/profile +
visible head + same-open authority + durable capture; post-COMMIT reopen remains
separate complete-lifecycle evidence. V20 reuses only the independently rehashed
93-file sealed v10 input root with `product_release_reuse=false` and
`input_reuse=true`. Their exact v10 paths and SHA-256 values remain named; they
are not copied, renamed, or represented as v20-generated. V20 reuses no v14
product row or timing observation. The new v20 input manifest rebinds the same sealed
files byte-for-byte before the v20 method manifest and source freeze are
generated.

V15 attempts 1–5 remain append-only and are not measurement authority for v20.
Attempt 5 completed its screen work under 20 seconds but exposed a product
timer-equation defect: CompleteRoundTrip wall begins after same-open authority,
while the lifecycle sum added that authority again. V20 removes only that term
from the lifecycle sum. It does not change the workload, trust or durability
semantics, population, schedule, thresholds, or decision equation.

V16 is preserved after one 12.121869666-second screen with eight operation
rows. Its G5 rows confirm the lifecycle equation repair. V20 changes only the
analyzers: lifecycle decomposition equality remains mandatory for G5, while
the immutable G4 baseline retains its legacy decomposition; S07 requires the
actual product label `regenerated-isolated-database` while its environment
custody label remains `fast-lane-isolated-prepared-row`. No v16 row is reused.

Each actual screen and gate process performs exactly one matching read-only
wrapper initialization after acquiring and verifying the global lock and
sealed master custody, before ordinal 1, and inside complete wall. The bounded
interval starts before database discovery and ends after the separate measured
evidence file and directory fsync, so its query time is only a reported subset.
The final `PREARM-WRAPPER-INITIALIZATION-v20.json` write is outside that bound
but remains inside complete-wall finalization. Both analyzers require exactly
one record, the measured evidence hash, exact frozen dry/plan/input/database
bindings, zero product/Store/row counts, calibration-only semantics, and the
complete bounded interval within the dry initialization bound. Every arm still
performs its own post-COMMIT head/generation/receipt query.

After every controlling product, harness, runner, analyzer, document,
schedule, expectation, limitation, and supersession byte has settled, v20
generates its method manifest and source freeze exactly once, last. A separate
verification pass independently reopens and SHA-256 hashes every manifest row,
the manifest itself, and every direct freeze binding before any dry-run or
child. It writes `FREEZE-VERIFICATION-v20.json` with classification
`PostFreezeIndependentRehashNotMethodAuthority`; that evidence must postdate
both manifests and the source freeze and is deliberately excluded from the
method authority to avoid a hash cycle. A stale, regenerated, self-trusting,
or partially verified manifest or freeze is a premeasurement failure.

## Screen

The screen retains seven sequences: 1-MiB healthy parity; a 1-MiB two-mode
touched boundary matrix covering exact `MissingObject`, `IdentityMismatch`,
`WrongLogicalRole`, and the native typed malformed mapping error; 10-MiB
unrelated-corruption policy distinction; 10-MiB
trusted commit followed by Verified reopen plus lost-ack/rollback cleanup; one
100-MiB frozen-G4/G5-Verified/G5-Trusted first-edit comparison; one analogous
middle `+1` comparison; and one protected create/range sentinel. One trusted
row above 25 ms or below 50% improvement rejects the mechanism but cannot by
itself establish a percentile PASS.

The touched matrix is exactly eight records: both `verified` and
`trusted-local-dev` crossed with `missing-object`, `identity-mismatch`,
`wrong-logical-role`, and `malformed-logical-record`. Their exact classes are
`MissingObject`, `IdentityMismatch`, `WrongLogicalRole`, and `UnexpectedEof`;
all have `failure_boundary=PreCommit`, transactions/COMMITs `1/0`, null
publication, `NotAttempted` reconciliation, no verified carry-forward,
unchanged head, cleanup success, no residue, and terminal Q zero. Existing
semantic cases emit null for the three matrix-only fields.

## Gate schedule, ordering, and checkpoints

The two comparisons use `first-edit-after-reopen`, `same-middle`,
`one-byte-{early,middle,late}`, and `plus1-{early,middle}`:

1. frozen G4 Verified versus G5 Verified; and
2. the same G5 executable in Verified versus Trusted mode.

V20 voluntarily freezes a deliberately stricter v1-lineage design: each
comparison receives 20 primary pairs and five pairs for every secondary
shape. This is 50 pairs/100 arm observations per comparison and 200 arm
observations total. It is not represented as the exact or only reading of the
user minimum.

Primary ordering is 10 AB/10 BA. For each comparison, `same-middle`,
`one-byte-middle`, and `plus1-early` use `AB,BA,AB,BA,AB`; `one-byte-early`,
`one-byte-late`, and `plus1-middle` use `BA,AB,BA,AB,BA`. Thus secondary first
positions are exactly 15 A/15 B per comparison. Frozen G4 remains one-shot.
Only the current sequence's persistent role pair is retained, with at most two
simultaneously live product processes including frozen-G4 one-shot overlap;
all children close failure-safely before advancing to the next sequence.

Primary first-reopen and every same-G5 comparison use classification
`full-first-edit-equation`, equal to `decision_ns`. Secondary G4-Verified versus
G5-Verified comparisons use `common-edit-through-reconciliation`, the exact sum
of edit-base scope + mapping/construction + proof + publication COMMIT + non-
double-counted reconciliation. No G4 zero-filled open/preflight/head field
enters that secondary decision.

Every operation row emits the selected `comparison_interval_ns`, exact
classification, and ordered component list. It also emits named
`comparison_intervals_ns` and `comparison_interval_classifications`. In S06,
the G5-Verified row carries both names: `g4_verified_vs_g5_verified` is common
and `g5_verified_vs_g5_trusted` is full; G4 and Trusted carry their respective
single named interval.

Every matched pair is covered by exact child-started, simultaneous-child high-
water, child-closed, terminal-present, terminal-owner-zero, terminal-Q-zero,
and failure-cleanup evidence. The sequence high-water must match the children
actually required and active children must return to zero before advancing.
Every external RSS observation is classified as one product child, never as a
sum across retained children; every product child independently satisfies the
20,971,520-byte cap. Every frozen-G4 RSS observation has the exact deterministic
kind `synchronous-one-shot`; aliases or a retained-child classification fail.

The standalone and raw lifecycle schema is
`phase4-g5-1-product-child-lifecycle-v20`. The two copies must be byte-value
equivalent. Its `pair_scopes` cover every scheduled pair exactly; its complete
per-product-child RSS list has one retained observation for every started and
reaped child and makes no aggregate RSS claim.

Each of the 14 gate sequences has two role-sequences, for 28 role-sequences.
The first and final observation in every role-sequence are prospectively fixed
`CompleteRoundTrip` checkpoints; all intervening observations are
`CaptureOnly`. Because every sequence has at least five observations per role,
first and final are distinct. The exact total is:

```text
14 gate sequences * 2 roles * 2 fixed checkpoints = 56 CompleteRoundTrip checkpoints
```

Every wrapper emits `validation_scope` and Boolean `fixed_checkpoint`.
Analyzers must derive and require the exact first/final positions from schedule,
comparison, role, pair, and role-sequence cardinality; an omitted, additional,
or shifted checkpoint is a hard failure.

## Once-under-lock input custody and APFS clone receipt

After acquiring the owner-bound global lock and before starting a benchmark
child, v20 verifies the complete sealed input manifest exactly once: path,
kind, mode, byte length, and SHA-256 for every fixture, master database,
authority, expectations file, preparation record, and manifest. The manifest
and source freeze are rechecked across lock acquisition. Unique masters are not
rehash-charged after that successful pass.

Every isolated row still receives an APFS `clonefile` copy from the verified
immutable master. The wrapper records nested `clone_receipt` schema
`g5-v20-native-clone-receipt-v1`. Its ordered `entries` bind each source master
relative path and manifest authority to destination relative path, exact bytes,
file kind, successful native clone result, required inode relation, and
`copy_content = "NotRehashedPerFastLaw"`. The receipt also binds the allowed
inventory and the exact mode transition observed prospectively on Darwin:
sealed source `0444` -> native clone `0444` -> private dispatch file `0600`,
with private row directories `0700`. This owner-only chmod is required because
`clonefile` preserves `0444` and such a clone rejects `O_RDWR`; it changes no
content and performs no fallback or rehash. Any missing/extra entry, size/mode
transition mismatch, wrong master authority, clone failure, or prohibited inode
relation fails before dispatch. No unverified copy fallback is allowed.

This is exact input custody under the frozen nonadversarial private-local
boundary: the immutable source is SHA-verified once under lock, and successful
native copy-on-write cloning plus exact receipt metadata transfers that
authority without rereading the same 100-MiB bytes for every observation.

## Rooted published-state authority

V10's zero-row failure isolated its voluntary Python scan of every object-table
row on every arm. That scan is not the product's snapshot-verification API: the
controlling G5 plan explicitly limits `verify_snapshot_closure` to current and
explicitly retained reachable closures and reserves unreachable/all-row CAS
inspection for a future separate API. V20 therefore removes the external
`O(arms * object-table-rows)` scan prospectively. It does not move, cache, or
relabel that work. CaptureOnly all-row/unreachable catalog parity is exactly
`NotClaimedSeparateFutureAllRowCasAudit`.

Every arm instead emits `rooted_logical_state` schema
`g5-v20-rooted-logical-state-v1` from one query-only, autocommit, constant-row
read of the published head. It binds exact generation, root, transition,
216-byte validation-receipt length and SHA-256, and the product-emitted ordered
closure digest. The receipt is classified only as
`ProductAuthenticatedHeadTupleOpaqueHashNotClosureOrFreshness`; it is never called a
closure proof or rollback-freshness authority. CaptureOnly closure provenance
is `PreparedGoldenBoundByExactRootTransitionAndProductQualification`.
CompleteRoundTrip provenance is `ObservedVerifiedCompleteRoundTrip`.

Reachable published-result parity remains a hard claim on all 200 arms: exact
root, transition, ordered closure, generation, receipt hash, authority and
expectations sidecar hashes, mutation work, authentication/SQL/BLOB counters,
transaction, one COMMIT, reconciliation, Q, clone receipt, inventory, and
cleanup receipt are individually hard-gated. Both comparisons require exact
pair equality for rooted result and mutation-side work. Full authentication,
read, SQL, and BLOB pair equality is required only for frozen G4 Verified
versus G5 Verified. G5 Verified versus Trusted deliberately differs in scrub,
authentication, and read work; those values remain individually hard-gated and
fully reported, not pair-equal. All 56 fixed checkpoints additionally perform a
fresh Verified reopen, complete reachable-closure scrub, reconstruction/range
verification, and fresh closure comparison. All screen success paths remain
CompleteRoundTrip checkpoints.

V20 does not claim physical SQLite file-byte parity. The sibling
`physical_allocation_observation` remains schema
`g5-v20-physical-allocation-observation-v1`, classification
`NotLogicalParity`, and separately reports file bytes, page size/count,
free-list count, derived allocated/free-list bytes, and ordered schema
rootpages. None of those physical fields is a rooted logical-identity claim.

Every operation arm also emits an exact cleanup receipt binding its isolated
work root, allowed and terminal inventory, cleanup status, work-root
simultaneous high-water `<=1`, and terminal active work roots `=0`. Missing,
overlapping, incompletely inventoried, or residually live row roots fail the
campaign even when the native row itself passed.

Each wrapper receipt uses schema `phase4-g5-1-arm-cleanup-receipt-v20` and is
repeated in order by the raw and standalone
`phase4-g5-1-work-root-lifecycle-v20` record. Started and cleaned roots equal
the exact operation count, receipt inventory SHA-256 is independently
recomputed, the parent-directory fsync is explicit, and both copies must be
value-identical.

## Hard equations and thresholds

```text
trusted complete closure scrub calls/bytes       = 0/0
verified complete closure scrub bytes             > 0
trusted verified receipt-covered assumptions     = 0
trusted verified carry-forward                    = false
healthy rooted reachable state/work parity        = exact
CaptureOnly all-row/unreachable catalog parity    = NotClaimed
healthy authority/expectations SHA-256 parity     = exact
transactions/publication COMMITs                  = 1/1
operation and child terminal Q                    = 0
fixed CompleteRoundTrip checkpoints               = 56
physical SQLite SHA-256 parity                     = NotClaimed
```

The first-edit timer is the exact sum of Store preflight, SQLite open/profile,
visible head/transition, edit-base scope, mapping/construction, proof,
publication COMMIT, and non-double-counted reconciliation. Trusted first-edit
p50/p95 must be `<=15/25 ms`; trusted early/middle `+1` p50 must be `<=15 ms`;
paired primary median improvement must be `>=50%` (`>=80%` is reported as the
strong expectation). Verified regression requires both ratio `>1.05` and mean
delta `>=1 ms`.

Maximum RSS is 20,971,520 bytes per product child and each owned buffer is at
most 1 MiB. Pair-scoped child high-water and failure-safe terminal closure are
hard gates; aggregating RSS across children or retaining unrelated children is
inadmissible.
Transactions/COMMITs, SQL/BLOB/authentication work, exact errors, durability,
reconciliation, Q, descriptors, journals, temps, clone receipts, rooted state,
sidecars, cleanup, lock custody, both analyzers, and final manifest
verification are hard gates without a materiality waiver.

Screen requires a hash-bound dry-run PASS. Static closure must postdate and
bind the screen terminal, complete wall, final manifest, and final read-only
verification. Gate rechecks that chain before and after lock acquisition.
Complete wall ends only after lock release, analyzer agreement, cleanup, final
manifest verification, and final read-only verification are fsynced, and must
be `<=150 s`.

## Mandatory limits

Rollback freshness is `NotProtected`. Controlled-cold and byte-level physical
I/O are `Unavailable`. The mode is benchmark-private/non-production. There is
no physical SQLite byte-parity claim, persistent authority or projection seed,
GC, 500-MiB claim, G6 claim, malicious-same-UID claim, or cross-platform claim.
