# G5-1 v10 review synthesis

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

## V9 terminal and accepted v10 method repair

V9 honestly closed as `PREMEASUREMENT_REVISE`. Its exact retained terminal is
bound by `V9-SUPERSESSION-v10.json`. V9 started no release build, prepared no
input, created no authoritative dry-run, screen, gate, or lock, and emitted no
measured row. V10 therefore reuses neither input nor rows and uses the distinct
`target/phase4-g5-trusted-reopen-edit-inputs-20260823-v10` root.

V10 keeps the 200-arm schedule, 56 checkpoints, half-slower calibration floor,
thresholds, and 120-second wall. It repairs only the terminal v9 blockers:

- verify the complete sealed input manifest once under the global lock;
- use native APFS clone receipts, schema `g5-v10-native-clone-receipt-v1`, to
  transfer that preverified authority without hashing identical clone content;
- attest the deterministic sealed `0444` -> cloned `0444` -> private `0600`
  file-mode transition and private `0700` directories required for SQLite;
- emit an ordered logical catalog, schema
  `g5-v10-ordered-logical-catalog-v1`, plus small authority/expectations hashes
  on every arm;
- make the first and final observation of every role-sequence fixed
  `CompleteRoundTrip` checkpoints; and
- make intervening observations `CaptureOnly` while retaining exact roots,
  transitions, catalog, work, transactions, Q, and cleanup evidence;
- classify primary and same-G5 cells as `full-first-edit-equation`, but
  secondary frozen-G4-versus-G5 cells as `common-edit-through-reconciliation`,
  the exact edit-base/mapping/proof/COMMIT/reconciliation sum;
- retain only the current sequence's persistent role pair, with no more than
  two simultaneous product children including one-shot overlap, recording
  exact pair coverage, terminal owner/Q zero, and failure-safe cleanup before
  advancing;
- classify the unchanged 20,971,520-byte RSS cap per product child, not as a
  sum or allowance across retained children;
- move page size/count, free-list count, schema rootpages, and derived database
  bytes out of the logical catalog and digest into a separate physical
  observation; and
- expand touched semantic evidence to exact two-mode `MissingObject`,
  `IdentityMismatch`, `WrongLogicalRole`, and typed malformed mapping records,
  all fail-before-COMMIT with unchanged head and terminal cleanup; and
- require a per-arm cleanup receipt plus work-root simultaneous high-water
  `<=1` and terminal active count zero.

Physical SQLite SHA-256 parity is removed from the G5 performance decision and
is explicitly `NotClaimed`. Page allocation, free-list, and schema-rootpage
facts are observed but never included in logical equality. This is not weaker logical authority:
catalog framing, content-addressed object IDs, exact roots/transitions,
authenticated object use, small sidecars, and full first/final checkpoints
provide the prospectively frozen authority.

The method manifest and source freeze are generated exactly once, last, only
after all controlling bytes settle. A separate pass independently reopens and
SHA-256 rehashes every manifest row and direct freeze binding before any
dry-run; generation is never accepted as its own verification. The resulting
`FREEZE-VERIFICATION-v10.json` is classified
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
`PRODUCT-CHILD-LIFECYCLE-v10.json` under schema
`phase4-g5-1-product-child-lifecycle-v10`. Per-arm cleanup receipts use schema
`phase4-g5-1-arm-cleanup-receipt-v10`; their ordered terminal record is likewise
duplicated as `WORK-ROOT-LIFECYCLE-v10.json` under schema
`phase4-g5-1-work-root-lifecycle-v10`.

The future zero-row dry-run contains exactly one versioned wrapper calibration
over the frozen 100-MiB prepared master. It starts zero products, opens zero
Stores, dispatches zero rows, and acquires no global lock. Exactly three
samples each cover native clone, allowed-inventory evidence, ordered logical-
catalog extraction, sidecar hashes, evidence file/directory fsync, and
immediate cleanup. Four times the slowest sample is the conservative per-arm
bound, and the complete forecast charges `200 * wrapper_bound_ns`. This is in
addition to the external-hash forecast at the retained half-slower throughput
floor. The exact versioned schemas end in
`wrapper-calibration-intent-v10`, `wrapper-calibration-sample-v10`, and
`wrapper-calibration-result-v10`, all under the `phase4-g5-1-` prefix. The
intent embeds `phase4-g5-1-wrapper-calibration-plan-v10`, retains the complete
representative-candidate table, and selects the lexicographic maximum of each
candidate's exact
`(file_count, total_manifest_bytes, directory_count, relative_path)` tuple.

The fixed `10,000,000,000 ns` campaign component is a prospective allowance
for lock custody, analyzers, payload/final manifests, and terminal/final
verification, anchored by rounding the retained H11 G5-0 terminal audit's
`9,254,244,292 ns` upward. That retained observation does not prove an upper
bound for 200-row campaign finalization. Only the actual complete gate wall
`<=120 seconds` is terminal timing authority. Its exact retained-evidence
inference is `ProspectiveAllowanceNotProvenUpperBound`. The forecast is the sum
of its enumerated evidence and calibration components. Any remaining amount
below 120 seconds is a feasibility reserve only, not work or timing evidence
and not an additive forecast component; its exact classification is
`RemainingTimeNotWorkAndNotTimingEvidence`. A sum above 120 seconds durably
closes v10 as `PREMEASUREMENT_REVISE` before screen; no schedule or threshold
is reduced.

`DRY-RUN-INTENT-v10.json` is durably written before either calibration begins.
A single outer failure funnel then covers wrapper calibration, hash calibration,
forecasting, and sealing: it always writes `DRY-RUN-FAILED-v10.json` before a
REVISE `DRY-RUN-DISPOSITION-v10.json`, even when the wrapper result is absent.
The disposition binds the FAILED artifact and records absent optional evidence
without inventing or laundering a successful wrapper result.

## Sample-count correction

An earlier review sentence incorrectly asserted that the user contract places
20 primary pairs and five pairs for every secondary shape independently inside
each of the two comparisons. The handoff requires at least 20 matched primary
observations and five adjacent secondary pairs while also requiring two
comparisons; it does not uniquely prescribe repeating the full minimum in each
comparison.

V10 nevertheless voluntarily freezes the stricter v1-lineage design: each
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

Every matched arm requires exact root, transition, nested `logical_catalog`,
`post_authority_sha256`, `post_expectations_sha256`, frozen mutation work,
SQL/BLOB/authentication counters, transaction/COMMIT, error, durability,
reconciliation, Q, buffer/RSS, descriptor/residue, and clone-receipt evidence.
The 56 fixed checkpoints additionally require complete roundtrip verification.
Lock chronology, durable dry-run disposition, both analyzers, cleanup, final
manifest custody, and complete wall `<=120 s` remain hard without a
materiality waiver.
