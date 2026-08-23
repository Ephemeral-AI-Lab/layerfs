# Phase 4 G5-1 v10 preregistration

Objective: make first edit after reopen operation-local only under the explicit
benchmark-private `TrustedLocalDev` contract while preserving Verified as the
default and byte-identical logical control behavior.

## Fixed architecture and threat boundary

The candidate remains CAS + CDC + COW + canonical K64/F64 + SQLite. V10 changes
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
-> one complete <=120-second gate
```

Any failure preserves v10. No observed row may be deleted, replaced, reordered,
or selectively rerun. A source or method repair creates v11.

The dry-run admits no product child, Store, row, measurement timer, or global
lock. It performs one labeled Python SHA-256 pass and one external `shasum`
pass over the frozen 100-MiB prepared master. Its conservative throughput is
no greater than half the slower observed implementation:

```text
floor_bytes_per_second = min(python_bps, external_shasum_bps) // 2
```

The same zero-row invocation performs exactly one versioned wrapper-calibration
routine over that prepared master. The routine starts no product, opens no
Store, dispatches no row, and acquires no global lock. It takes exactly three
samples of the complete prospective wrapper path: native clone, exact allowed-
inventory evidence, ordered logical-catalog extraction, sidecar hashing,
evidence-file and evidence-directory fsync, and immediate isolated-root
cleanup. The conservative per-arm bound is four times the slowest of those
three samples; the forecast charges exactly `200 * wrapper_bound_ns`. The
calibration is one zero-row routine with three samples, not 200 speculative
benchmark arms. Its intent, sample, and result schemas are respectively
`phase4-g5-1-wrapper-calibration-intent-v10`,
`phase4-g5-1-wrapper-calibration-sample-v10`, and
`phase4-g5-1-wrapper-calibration-result-v10`.

Selection of the representative prepared master is itself frozen evidence.
The wrapper intent embeds plan schema
`phase4-g5-1-wrapper-calibration-plan-v10` and records the complete candidate
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
wall `<=120 seconds` remains the sole terminal timing authority. The retained
evidence binds `campaign_finalization_inference` to
`ProspectiveAllowanceNotProvenUpperBound`. The complete-wrapper forecast is the
sum of retained timing evidence, the calibrated
external-hash component, the 200-arm wrapper-calibration component, and that
fixed allowance. The positive difference between 120 seconds and the forecast
is reported only as feasibility reserve; reserve is neither scheduled work nor
timing evidence and is never added to the forecast. Its exact classification is
`RemainingTimeNotWorkAndNotTimingEvidence`. Whether PASS or REVISE, the runner
must write and fsync the top-level `DRY-RUN-INTENT-v10.json` before invoking
either wrapper or hash calibration. One failure funnel covers every subsequent
step. Any exception—including failure before a wrapper result exists—must
write and fsync `DRY-RUN-FAILED-v10.json` first, then write and fsync a
`DRY-RUN-DISPOSITION-v10.json` with `status=REVISE` that binds the FAILED hash
and explicitly records absent optional artifacts. A success or calibrated
overrun writes and fsyncs complete `DRY-RUN-v10.json`, including both
calibrations, all byte/time components, the 200-arm assertion, and zero-
product/Store/row/lock counters; it then writes and fsyncs the disposition and
its directory. A summed
forecast above 120 seconds additionally writes and fsyncs
`PREMEASUREMENT-REVISE-v10.json` and stops before screen. A missing dry-run or
failure disposition is never treated as an authoritative PASS or reconstructed
from an earlier attempt.

V9 is prospectively superseded by the exact SHA-256 of
`PREMEASUREMENT-REVISE-v9.json`. V9 produced no release executable, prepared
input, authoritative dry-run, screen, gate, lock acquisition, or measured row.
V10 reuses neither input nor rows and exclusively targets
`target/phase4-g5-trusted-reopen-edit-inputs-20260823-v10`; the absent v9 input
root is never adopted or relabeled.

After every controlling product, harness, runner, analyzer, document,
schedule, expectation, limitation, and supersession byte has settled, v10
generates its method manifest and source freeze exactly once, last. A separate
verification pass independently reopens and SHA-256 hashes every manifest row,
the manifest itself, and every direct freeze binding before any dry-run or
child. It writes `FREEZE-VERIFICATION-v10.json` with classification
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

V10 voluntarily freezes a deliberately stricter v1-lineage design: each
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
`phase4-g5-1-product-child-lifecycle-v10`. The two copies must be byte-value
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
child, v10 verifies the complete sealed input manifest exactly once: path,
kind, mode, byte length, and SHA-256 for every fixture, master database,
authority, expectations file, preparation record, and manifest. The manifest
and source freeze are rechecked across lock acquisition. Unique masters are not
rehash-charged after that successful pass.

Every isolated row still receives an APFS `clonefile` copy from the verified
immutable master. The wrapper records nested `clone_receipt` schema
`g5-v10-native-clone-receipt-v1`. Its ordered `entries` bind each source master
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

## Ordered logical state authority

V10 does not claim or compare physical SQLite file-byte parity. SQLite page
size/count, free-list count, schema `rootpage`, their derived database-byte
observation, raw page layout, journal history, and other physical serialization
choices are outside the logical parity decision and outside the nested logical
digest. They are reported separately as typed physical observations and never
paired for logical equality.

The sibling `physical_allocation_observation` has schema
`g5-v10-physical-allocation-observation-v1` and classification
`NotLogicalParity`. It reports file bytes, page size/count, free-list count,
derived allocated/free-list bytes, and ordered typed schema rootpages. The
logical catalog requires
`sqlite_schema_hash_semantics=type-name-table-sql-excludes-rootpage`.

Every arm emits nested `logical_catalog` schema
`g5-v10-ordered-logical-catalog-v1`. It is a domain-separated digest of the
complete benchmark-owned logical catalog in fixed table-tag and primary-key
order with typed, length-prefixed framing. CAS object rows commit to object ID,
logical kind/role, canonical length, and other logical columns; the invariant
`ObjectId = hash(canonical bytes)` and the existing object authentication rules
remain mandatory. Head, transition, receipt, profile, generation, and other
benchmark-owned logical rows are included. SQLite implementation metadata and
raw page order are excluded. Catalog schema, row count, per-table counts,
framed byte count, and digest are all emitted and checked. No page, free-list,
schema-rootpage, or derived physical database-byte field may appear inside
`logical_catalog` or influence its digest.

Matched arms require exact `logical_catalog` equality. Every arm also hashes
the small authority and expectations sidecars and emits
`post_authority_sha256` and `post_expectations_sha256`; both must equal their
frozen expected values and their matched-arm values. The 56
`CompleteRoundTrip` checkpoints additionally perform the existing full closure,
reopen, reconstruction/range, durability, and cleanup verification. Exact
root, transition, catalog, sidecars, mutation work, SQL/BLOB/authentication
counters, transaction/COMMIT count, and Q/residue facts remain required on
every arm.

Every operation arm also emits an exact cleanup receipt binding its isolated
work root, allowed and terminal inventory, cleanup status, work-root
simultaneous high-water `<=1`, and terminal active work roots `=0`. Missing,
overlapping, incompletely inventoried, or residually live row roots fail the
campaign even when the native row itself passed.

Each wrapper receipt uses schema `phase4-g5-1-arm-cleanup-receipt-v10` and is
repeated in order by the raw and standalone
`phase4-g5-1-work-root-lifecycle-v10` record. Started and cleaned roots equal
the exact operation count, receipt inventory SHA-256 is independently
recomputed, the parent-directory fsync is explicit, and both copies must be
value-identical.

## Hard equations and thresholds

```text
trusted complete closure scrub calls/bytes       = 0/0
verified complete closure scrub bytes             > 0
trusted verified receipt-covered assumptions     = 0
trusted verified carry-forward                    = false
healthy root/transition/logical catalog parity    = exact
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
reconciliation, Q, descriptors, journals, temps, clone receipts, logical
catalogs, sidecars, cleanup, lock custody, both analyzers, and final manifest
verification are hard gates without a materiality waiver.

Screen requires a hash-bound dry-run PASS. Static closure must postdate and
bind the screen terminal, complete wall, final manifest, and final read-only
verification. Gate rechecks that chain before and after lock acquisition.
Complete wall ends only after lock release, analyzer agreement, cleanup, final
manifest verification, and final read-only verification are fsynced, and must
be `<=120 s`.

## Mandatory limits

Rollback freshness is `NotProtected`. Controlled-cold and byte-level physical
I/O are `Unavailable`. The mode is benchmark-private/non-production. There is
no physical SQLite byte-parity claim, persistent authority or projection seed,
GC, 500-MiB claim, G6 claim, malicious-same-UID claim, or cross-platform claim.
