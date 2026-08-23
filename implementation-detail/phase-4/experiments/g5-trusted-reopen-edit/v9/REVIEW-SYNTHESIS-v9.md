# G5-1 v9 review synthesis

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

## V8 failure and accepted v9 method repair

V8 honestly stopped before rows because its in-memory calibrated forecast was
582,626,960,709 ns. It planned 72,596,459,562 external hash bytes, dominated by
42,813,237,840 bytes of repeated isolated-clone hashing and 21,841,666,640
bytes of per-row post-image hashing. No original `DRY-RUN-v8.json` was written;
the retained v9 binding labels the exact arithmetic as a later read-only
diagnostic reconstruction, not an authoritative v8 artifact.

V9 keeps the half-slower calibration floor and 120-second wall. It changes the
frequency and authority of custody work:

- verify the complete sealed input manifest once under the global lock;
- use native APFS clone receipts, schema `g5-v9-native-clone-receipt-v1`, to
  transfer that preverified authority without hashing identical clone content;
- attest the deterministic sealed `0444` -> cloned `0444` -> private `0600`
  file-mode transition and private `0700` directories required for SQLite;
- emit an ordered logical catalog, schema
  `g5-v9-ordered-logical-catalog-v1`, plus small authority/expectations hashes
  on every arm;
- make the first and final observation of every role-sequence fixed
  `CompleteRoundTrip` checkpoints; and
- make intervening observations `CaptureOnly` while retaining exact roots,
  transitions, catalog, work, transactions, Q, and cleanup evidence.

Physical SQLite SHA-256 parity is removed from the G5 performance decision and
is explicitly `NotClaimed`. This is not weaker logical authority: SQLite page
layout was never a logical digest. Catalog framing, content-addressed object
IDs, exact roots/transitions, authenticated object use, small sidecars, and
full first/final checkpoints provide the prospectively frozen authority.

Frozen G4 remains one-shot and byte-preserved. Its launch and internal custody
work are part of complete arm wall and are not also charged as separate
external hash passes. Only common direct internal timer intervals decide
Verified-overhead materiality.

## Sample-count correction

An earlier review sentence incorrectly asserted that the user contract places
20 primary pairs and five pairs for every secondary shape independently inside
each of the two comparisons. The handoff requires at least 20 matched primary
observations and five adjacent secondary pairs while also requiring two
comparisons; it does not uniquely prescribe repeating the full minimum in each
comparison.

V9 nevertheless voluntarily freezes the stricter v1-lineage design: each
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
