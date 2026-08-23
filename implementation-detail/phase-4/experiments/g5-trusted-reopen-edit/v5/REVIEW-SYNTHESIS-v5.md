# G5-1 v5 review synthesis

Status: implementation-ready; measurement remains closed until focused tests,
zero-row dry-run, screen, and frozen static closure pass.

## Accepted source recommendations

- Add `IntegrityMode::{Verified, TrustedLocalDev}` once per Store lifetime;
  existing open APIs remain Verified defaults and there is no setter.
- Keep the verified witness intact and add a distinct single-use transaction-
  local trusted scope. Share only the exact Store/open/profile/epoch/head/
  receipt/authority-serial/transaction fencing tuple.
- Branch only at edit-base establishment. Trusted authenticates the current
  head and transition but omits the eager current/parent `scrub_file` calls.
- Replace the four real permit consumers with
  `EditBaseScope::{Verified, Trusted}`. CAS fetch/put/incumbent, mapping,
  construction proof, expected-head, publication, COMMIT, and reconciliation
  remain branch-free.
- Keep verified receipt-covered edges distinct from
  `trusted_assumed_equal_edges`, `trusted_assumed_prior_references`, and
  `trusted_assumed_prior_raw_bytes`. Record direct verified scrub calls/bytes.
- Trusted proof and publication never authorize verified carry-forward.
- Keep the post-COMMIT reopen Verified and add the exact foreground
  `verify_snapshot_closure` operation for current and explicit retained
  root/transition descriptors. Unreachable object-table rows remain a separate
  future all-row audit.
- Add only focused semantic/fault tests and no dependency, schema, receipt,
  canonical format, SQLite profile, worker, retry, or API integration.

## Accepted benchmark recommendations

- Reuse v9's owner-bound global lock, fsync, manifest, and read-only
  verification protocol.
- Build the G5 executable once, freeze it, and use the same bytes for persistent
  Verified and Trusted children. Store mode is fixed by the child handshake.
- Copy the frozen G4 v12 candidate executable exclusively into each result root,
  verify SHA-256 `e72988fc25e96f608d0d405e157ea8e837029595ace916f066932082a736db33`,
  and change mode only on the copy.
- Frozen G4 remains one-shot because modifying it would invalidate the control.
  Its comparison is labeled compatibility/overhead, includes launch on both
  sides, and decisions use common direct internal timers.
- Prepare deterministic 1/10/100-MiB fixtures and isolated DB/authority/
  expectations masters once. Use APFS clone/copy outside row timers and hash
  every isolated operand before dispatch.
- Charge or stream child arguments, schedules, timings, and report output;
  operation and whole-child terminal Q must be zero.

## Resolved disagreement

One custody recommendation reduced frozen-G4 versus G5-Verified to two samples
per shape. Rejected: the user contract places the 20-primary/five-secondary
minimum on the two-comparison design. Both comparisons therefore use 20
first-edit pairs and five pairs for each of six secondary shapes. The gate has
200 arm observations total. If that complete schedule misses 120 seconds, v5
is preserved as REVISE and a new in-scope mechanism/method version is required;
the sample law is not weakened.

## Frozen success law

Verified G5 must have no material regression versus frozen G4 under both
`candidate/control > 1.05` and mean delta `>=1 ms`. Same-G5 Trusted must improve
paired primary median by at least 50%, achieve first-edit p50 `<=15 ms` and p95
`<=25 ms`, and achieve early/middle `+1` p50 `<=15 ms`. Root, transition,
post-row physical DB and authority hashes, exact errors, frozen mutation work,
one transaction/COMMIT, durability,
reconciliation, Q/RSS/buffers/descriptors, cleanup, custody, and analyzer
agreement remain hard without a materiality waiver.
