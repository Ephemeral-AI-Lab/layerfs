# Phase 4 G5-1 v9 preregistration

Objective: make first edit after reopen operation-local only under the explicit
benchmark-private `TrustedLocalDev` contract while preserving Verified as the
default and byte-identical logical control behavior.

## Fixed architecture and threat boundary

The candidate remains CAS + CDC + COW + canonical K64/F64 + SQLite. V9 changes
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

Any failure preserves v9. No observed row may be deleted, replaced, reordered,
or selectively rerun. A source or method repair creates v10.

The dry-run admits no Store, row, measurement timer, isolated copy, or global
lock. It performs one labeled Python SHA-256 pass and one external `shasum`
pass over the frozen 100-MiB source. Its conservative throughput is no greater
than half the slower observed implementation:

```text
floor_bytes_per_second = min(python_bps, external_shasum_bps) // 2
```

The complete-wrapper forecast is the fixed non-hash allowance plus every
prospectively scheduled external hash byte at that floor. Whether PASS or
REVISE, the runner must first write and fsync a complete `DRY-RUN-v9.json`,
including calibration, all byte/time components, the 200-arm assertion, zero-
row counters, and disposition, then fsync its directory. A forecast above
120 seconds additionally writes and fsyncs `PREMEASUREMENT-REVISE-v9.json` and
stops. A missing dry-run artifact is never treated as an authoritative PASS or
as a reconstruction of an earlier attempt.

## Screen

The screen retains seven sequences: 1-MiB healthy parity; 1-MiB touched
identity corruption; 10-MiB unrelated-corruption policy distinction; 10-MiB
trusted commit followed by Verified reopen plus lost-ack/rollback cleanup; one
100-MiB frozen-G4/G5-Verified/G5-Trusted first-edit comparison; one analogous
middle `+1` comparison; and one protected create/range sentinel. One trusted
row above 25 ms or below 50% improvement rejects the mechanism but cannot by
itself establish a percentile PASS.

## Gate schedule, ordering, and checkpoints

The two comparisons use `first-edit-after-reopen`, `same-middle`,
`one-byte-{early,middle,late}`, and `plus1-{early,middle}`:

1. frozen G4 Verified versus G5 Verified; and
2. the same G5 executable in Verified versus Trusted mode.

V9 voluntarily freezes a deliberately stricter v1-lineage design: each
comparison receives 20 primary pairs and five pairs for every secondary
shape. This is 50 pairs/100 arm observations per comparison and 200 arm
observations total. It is not represented as the exact or only reading of the
user minimum.

Primary ordering is 10 AB/10 BA. For each comparison, `same-middle`,
`one-byte-middle`, and `plus1-early` use `AB,BA,AB,BA,AB`; `one-byte-early`,
`one-byte-late`, and `plus1-middle` use `BA,AB,BA,AB,BA`. Thus secondary first
positions are exactly 15 A/15 B per comparison. Frozen G4 remains one-shot;
G5 roles retain mode-fixed persistent children. Only common direct internal
timer intervals decide overhead.

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
child, v9 verifies the complete sealed input manifest exactly once: path,
kind, mode, byte length, and SHA-256 for every fixture, master database,
authority, expectations file, preparation record, and manifest. The manifest
and source freeze are rechecked across lock acquisition. Unique masters are not
rehash-charged after that successful pass.

Every isolated row still receives an APFS `clonefile` copy from the verified
immutable master. The wrapper records nested `clone_receipt` schema
`g5-v9-native-clone-receipt-v1`. Its ordered `entries` bind each source master
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

V9 does not claim or compare physical SQLite file-byte parity. SQLite page
layout, free-list placement, journal history, and other physical serialization
choices are outside the logical parity decision.

Every arm emits nested `logical_catalog` schema
`g5-v9-ordered-logical-catalog-v1`. It is a domain-separated digest of the
complete benchmark-owned logical catalog in fixed table-tag and primary-key
order with typed, length-prefixed framing. CAS object rows commit to object ID,
logical kind/role, canonical length, and other logical columns; the invariant
`ObjectId = hash(canonical bytes)` and the existing object authentication rules
remain mandatory. Head, transition, receipt, profile, generation, and other
benchmark-owned logical rows are included. SQLite implementation metadata and
raw page order are excluded. Catalog schema, row count, per-table counts,
framed byte count, and digest are all emitted and checked.

Matched arms require exact `logical_catalog` equality. Every arm also hashes
the small authority and expectations sidecars and emits
`post_authority_sha256` and `post_expectations_sha256`; both must equal their
frozen expected values and their matched-arm values. The 56
`CompleteRoundTrip` checkpoints additionally perform the existing full closure,
reopen, reconstruction/range, durability, and cleanup verification. Exact
root, transition, catalog, sidecars, mutation work, SQL/BLOB/authentication
counters, transaction/COMMIT count, and Q/residue facts remain required on
every arm.

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

Maximum RSS is 20,971,520 bytes and each owned buffer is at most 1 MiB.
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
