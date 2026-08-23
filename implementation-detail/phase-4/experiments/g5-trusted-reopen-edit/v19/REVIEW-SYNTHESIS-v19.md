# G5-1 v19 review synthesis

V18 completed all 200 rows in a 115.521346875-second failure path, but G4 and
G5 processes exceeded the 20-MiB per-child RSS cap. V19 changes only the common
Darwin allocator environment and removes the v18 between-row pressure call.

V17 attempt 2 completed all 200 rows but closed REVISE at 144.914391833
seconds because one persistent child reached 21,135,360 bytes against the
20,971,520-byte cap. V19 releases free allocator pages only between rows after
Q0, and replaces per-byte hex formatter calls with bounded 4-KiB stack chunks;
serialized bytes remain exact.

Attempt 2 batches only full-scrub leaf authentication through the existing K64
reader. Focused parity requires identical closure length/reference counts and
authenticated object/byte totals, exact query reduction, K64 maximum batch,
and Q0. V16 attempt 3 remains REVISE at 148.356563458 seconds and supplies
diagnosis only; no row is reused.

Status: prospectively specified; measurement remains closed until focused
checks, a durable zero-row dry-run PASS, one complete screen, and frozen static
closure all pass in order.

## Retained product decisions

- `IntegrityMode::{Verified, TrustedLocalDev}` is fixed once per Store
  lifetime; all existing open APIs remain Verified defaults and there is no
  setter.
- The verified witness remains intact. Trusted uses a distinct, single-use,
  transaction-local scope sharing only Store/open/profile/epoch/head/receipt/
  authority-serial/transaction fencing.
- Trusted branches only at edit-base establishment. CAS fetch/put/incumbent,
  mapping, construction proof, expected-head, publication, COMMIT, and
  reconciliation remain branch-free and authenticate every used object.
- Verified receipt-covered counters remain distinct from trusted assumption
  counters. Trusted proof/publication never creates verified carry-forward.
- Post-COMMIT reopen remains Verified. Foreground snapshot verification covers
  current and explicit retained transition-root closures; unreachable object-
  table rows remain outside this milestone.
- No dependency, schema, receipt, canonical format, SQLite profile, retry,
  worker, or production API change is admitted.

## V16 screen and accepted v19 analyzer repair

V15 screen attempts 1–4 are preserved append-only harness failures. Attempt 5
completed in `11.329085334 s` with cleanup, Q0, RSS, and lock release intact,
but its CompleteRoundTrip first-edit rows reported
`lifecycle_phase_sum_matches=false`: the lifecycle wall begins after same-open
authority while the product equation added that authority again. No v15 row is
reused.

V16 confirmed the product repair: all G5 CompleteRoundTrip equations passed,
and its screen completed in `12.121869666 s` with cleanup, Q0, RSS, and lock
release intact. The frozen analyzers rejected the immutable G4 baseline's known
legacy equation and still expected an obsolete S07 product base-copy label.
V19 changes only those two analyzer predicates and the versioned child/report
schemas. The product algorithm, first-edit decision equation, workload, trust,
durability, schedule, and thresholds are unchanged. It retains v14's lock-context repair, split intent
contract, and top-level wrapper semantics. It measures one process-first wrapper
initialization and charges factor four once, then measures three complete
post-initialization recurring wrapper samples and charges factor four across all
200 arms. Each actual campaign performs the same initialization once under the
lock, after frozen custody and before ordinal 1, inside complete wall. The exact
93-file sealed input root remains a named, rehashed external operand; no v14
product row or timing is promoted to v19 authority.

V19 keeps the 200-arm schedule, 56 checkpoints, half-slower calibration floor,
thresholds, and 150-second wall. Its only authority redesign is:

- verify the complete sealed input manifest once under the global lock;
- use native APFS clone receipts, schema `g5-v19-native-clone-receipt-v1`, to
  transfer that preverified authority without hashing identical clone content;
- attest the deterministic sealed `0444` -> cloned `0444` -> private `0600`
  file-mode transition and private `0700` directories required for SQLite;
- emit constant-size rooted state, schema `g5-v19-rooted-logical-state-v1`,
  binding generation/root/transition, the 216-byte head-receipt hash, exact
  ordered closure, and small authority/expectations hashes on every arm;
- make the first and final observation of every role-sequence fixed
  `CompleteRoundTrip` checkpoints; and
- make intervening observations `CaptureOnly` while retaining exact rooted
  reachable state, work, transactions, Q, and cleanup evidence;
- classify primary and same-G5 cells as `full-first-edit-equation`, but
  secondary frozen-G4-versus-G5 cells as `common-edit-through-reconciliation`,
  the exact edit-base/mapping/proof/COMMIT/reconciliation sum;
- retain only the current sequence's persistent role pair, with no more than
  two simultaneous product children including one-shot overlap, recording
  exact pair coverage, terminal owner/Q zero, and failure-safe cleanup before
  advancing;
- classify the unchanged 20,971,520-byte RSS cap per product child, not as a
  sum or allowance across retained children;
- keep page size/count, free-list count, schema rootpages, and derived database
  bytes as a separate `NotLogicalParity` physical observation; and
- expand touched semantic evidence to exact two-mode `MissingObject`,
  `IdentityMismatch`, `WrongLogicalRole`, and typed malformed mapping records,
  all fail-before-COMMIT with unchanged head and terminal cleanup; and
- require a per-arm cleanup receipt plus work-root simultaneous high-water
  `<=1` and terminal active count zero.

Physical SQLite SHA-256 parity and CaptureOnly all-row/unreachable catalog
parity are explicitly `NotClaimed`. The validation receipt is only an
authenticated head-tuple/fencing record, never a closure proof or rollback
freshness. CaptureOnly closure provenance is
`PreparedGoldenBoundByExactRootTransitionAndProductQualification`; checkpoint
provenance is `ObservedVerifiedCompleteRoundTrip`. Exact roots/transitions,
ordered reachable closure, authenticated object use, mutation/SQL/BLOB work,
small sidecars, and 56 full first/final checkpoints remain hard authority.

The method manifest and source freeze are generated exactly once, last, only
after all controlling bytes settle. A separate pass independently reopens and
SHA-256 rehashes every manifest row and direct freeze binding before any
dry-run; generation is never accepted as its own verification. The resulting
`FREEZE-VERIFICATION-v19.json` is classified
`PostFreezeIndependentRehashNotMethodAuthority`, postdates the freeze, and is
excluded from method authority to avoid a hash cycle.

Frozen G4 remains one-shot and byte-preserved. Its launch and internal custody
work are part of complete arm wall and are not also charged as separate
external hash passes. Primary and same-G5 decisions use
`full-first-edit-equation`; only secondary G4-versus-G5 overhead uses
`common-edit-through-reconciliation`. Its RSS evidence kind remains the exact
hard value `synchronous-one-shot`.

The S06 triple reports both named decisions without laundering one through the
other: G4-Verified versus G5-Verified uses the common interval, while
G5-Verified versus G5-Trusted uses the full interval.

Child evidence is duplicated identically in raw JSONL and standalone
`PRODUCT-CHILD-LIFECYCLE-v19.json` under schema
`phase4-g5-1-product-child-lifecycle-v19`. Per-arm cleanup receipts use schema
`phase4-g5-1-arm-cleanup-receipt-v19`; their ordered terminal record is likewise
duplicated as `WORK-ROOT-LIFECYCLE-v19.json` under schema
`phase4-g5-1-work-root-lifecycle-v19`.

The future zero-row dry-run contains one versioned split wrapper calibration
over the frozen 100-MiB prepared master. It starts zero products, Stores, rows,
or locks. One process-first read-only initialization sample is factor-four
charged once. Three later complete recurring samples cover native clone,
inventory, constant published state, sidecars, shared mutation-work/wrapper
assembly, evidence fsync, and cleanup; four times their maximum is charged
across 200 arms. The exact schemas include
`wrapper-calibration-intent-v19`, `wrapper-initialization-sample-v19`,
`wrapper-calibration-sample-v19`, and `wrapper-calibration-result-v19`. The
calibration placeholder is `CalibrationShapeOnlyNoProductParity` with reachable
parity `NotClaimedCalibrationShapeOnly`; it is timing shape, never product or
prepared-golden authority. The
intent embeds `phase4-g5-1-wrapper-calibration-plan-v19`, retains the complete
representative-candidate table, and selects the lexicographic maximum of each
candidate's exact
`(file_count, total_manifest_bytes, directory_count, relative_path)` tuple.

Every screen/gate raw set and standalone artifact contains exactly one
`phase4-g5-1-prearm-wrapper-initialization-v19` record after lock/custody and
before ordinal 1. Its bounded total spans database discovery, the read-only
published-state query, and the measured evidence write plus file/directory
fsync; `query_ns` is a strict subset, not the total. The terminal prearm
artifact write is separately classified outside that bound but inside complete
wall. Both analyzers independently reopen the frozen dry run and input manifest
and require the exact dry bound, plan hash, input hash, selected database row,
calibration-only rooted/physical shape, zero product/Store/row counts, lock
ownership, and complete bounded total no greater than the dry factor-four bound.

The fixed `10,000,000,000 ns` campaign component is a prospective allowance
for lock custody, analyzers, payload/final manifests, and terminal/final
verification, anchored by rounding the retained H11 G5-0 terminal audit's
`9,254,244,292 ns` upward. That retained observation does not prove an upper
bound for 200-row campaign finalization. Only the actual complete gate wall
`<=150 seconds` is terminal timing authority. Its exact retained-evidence
inference is `ProspectiveAllowanceNotProvenUpperBound`. The forecast is the sum
of its enumerated evidence and calibration components. Any remaining amount
below 150 seconds is a feasibility reserve only, not work or timing evidence
and not an additive forecast component; its exact classification is
`RemainingTimeNotWorkAndNotTimingEvidence`. A sum above 150 seconds durably
closes v19 as `PREMEASUREMENT_REVISE` before screen; no schedule or threshold
is reduced.

`DRY-RUN-INTENT-v19.json` is durably written before either calibration begins.
A single outer failure funnel then covers wrapper calibration, hash calibration,
forecasting, and sealing: it always writes `DRY-RUN-FAILED-v19.json` before a
REVISE `DRY-RUN-DISPOSITION-v19.json`, even when the wrapper result is absent.
The disposition binds the FAILED artifact and records absent optional evidence
without inventing or laundering a successful wrapper result.

## Sample-count correction

An earlier review sentence incorrectly asserted that the user contract places
20 primary pairs and five pairs for every secondary shape independently inside
each of the two comparisons. The handoff requires at least 20 matched primary
observations and five adjacent secondary pairs while also requiring two
comparisons; it does not uniquely prescribe repeating the full minimum in each
comparison.

V19 nevertheless voluntarily freezes the stricter v1-lineage design: each
comparison receives 20 primary pairs and five pairs for each of six secondary
shapes. The result is 100 arms per comparison and 200 arms total. This is a
deliberate prospective design choice, not an exact restatement of the user
minimum, and it cannot be reduced after observation.

Ordering is unchanged: primary is 10 AB/10 BA. The secondary operations split
three AB-first and three BA-first five-pair blocks, producing exactly 15/15
first positions per comparison.

## Fixed checkpoint law

There are 14 gate sequences and two roles per sequence. The first and final
observation of each role-sequence are distinct fixed checkpoints:

```text
14 * 2 * 2 = 56 CompleteRoundTrip checkpoints
```

The wrapper fields are `validation_scope` and Boolean `fixed_checkpoint`.
Checkpoint position must be derived from the frozen schedule and role sequence;
the analyzers reject any missing, added, or displaced checkpoint.

## Frozen success law

Verified G5 has no material regression versus frozen G4 unless both
`candidate/control >1.05` and mean delta `>=1 ms`. Same-G5 Trusted achieves at
least 50% paired-primary median improvement, first-edit p50/p95 `<=15/25 ms`,
and early/middle `+1` p50 `<=15 ms`.

Every matched arm requires exact root, transition, nested `rooted_logical_state`,
`post_authority_sha256`, `post_expectations_sha256`, frozen mutation work,
SQL/BLOB/authentication counters, transaction/COMMIT, error, durability,
reconciliation, Q, buffer/RSS, descriptor/residue, and clone-receipt evidence.
Both comparisons pair-equal the rooted result and mutation-side work. Only
G4-Verified versus G5-Verified pair-equals full authentication/read/SQL/BLOB
work; Verified versus Trusted intentionally differs in scrub/auth/read work,
which remains individually hard-gated and fully reported.
The 56 fixed checkpoints additionally require complete roundtrip verification.
Lock chronology, durable dry-run disposition, both analyzers, cleanup, final
manifest custody, and complete wall `<=150 s` remain hard without a
materiality waiver.
