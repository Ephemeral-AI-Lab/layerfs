# G4 user-approved micro-variance decision v1

- Date: 2026-08-22
- Controlling stage disposition: **G4 ACCEPTED WITH USER-APPROVED MICRO-VARIANCE EXCEPTION**
- Sealed campaign disposition: **v12 remains TERMINAL REVISE under its frozen relative-only contract**
- Scope: G4 performance disposition only; no production/VFS/SDK integration and no G5 execution

## Evidence preserved

The sealed v12 campaign, analyzers, terminal files, and manifests are unchanged.
V12 still fails its prospectively frozen
`candidate_sum <= control_sum * 1.05` protected-route equation at sequences 17,
20, and 26. This decision does not relabel those rows as passing, reanalyze the
campaign, delete an issue, or amend the v12 methodology after measurement.

The decision is anchored to:

- measured terminal SHA-256
  `d3c6dba7cd114817c9153a0426d0a9cc92723bf58a7efc9830877673ff111b31`;
- terminal verification SHA-256
  `2837c7484238282e03b45876100be9cc4ca4fdfa1931b4cb4e173798809e0478`;
- normalized ledger SHA-256
  `dc563d339401b0e7cdf84b20f1a8da20c99b5f0da849c700e86dceaa9de546b1`;
- measured candidate executable SHA-256
  `e72988fc25e96f608d0d405e157ea8e837029595ace916f066932082a736db33`;
- `phase4_g3_materialization.rs` SHA-256
  `320ecb529c11de4464ce9a76ce97cc11f60d719d418f33a40d945e5f6dde196a`.

## Controlling rule

For future G4 disposition and for the explicit management exception applied
to sealed v12:

1. Every frozen hard absolute performance cap remains mandatory.
2. Semantic identity, exact work, checked Q with terminal zero, cleanup,
   old-or-new durability/reconciliation, resource ceilings, and evidence
   custody remain mandatory without an effect-size exception.
3. A protected micro-route relative regression is product-material only when
   both conditions hold on the frozen aggregate estimator:

   ```text
   candidate_sum / control_sum > 1.05
   AND
   candidate_mean - control_mean > 0.500 ms
   ```

4. Injected before-publication timing is primarily correctness, atomicity,
   typed-error, durability, and cleanup evidence. Its wall time is still
   reported, but throughput is not its acceptance purpose.

This rule is a user-approved controlling product decision made after v12. It
is not inserted into the sealed v12 analyzer and is not evidence that the old
relative-only gate passed.

## Application to v12

| Sequence | Route | Control mean | Candidate mean | Relative result under old gate | Absolute delta | Decision-rule result |
|---:|---|---:|---:|---:|---:|---|
| 17 | 100-MiB clone/no-op | 2.650500 ms | 2.876729 ms | +8.5353%, FAIL | +0.226229 ms | below 0.500 ms; non-material micro-variance |
| 20 | 1-MiB count-change fallback | 4.198896 ms | 4.484417 ms | +6.7999%, FAIL | +0.285522 ms | below 0.500 ms; non-material micro-variance |
| 26 | 1-MiB before-publication fault | 0.693604 ms | 0.793208 ms | +14.3604%, FAIL | +0.099604 ms | below 0.500 ms; correctness route and non-material micro-variance |

All frozen hard absolute targets pass, and v12 already proves exact semantic,
work, durability, resource, buffer, Q, cleanup, chronology, custody, and static
closure under the documented benchmark-private threat model. The three old-
gate failures therefore receive the explicit user-approved micro-variance
exception. G4 optimization stops with no new source change and no new campaign.

## Boundaries

- The native mechanism remains benchmark-private and operation-local.
- Cleanup and lock claims remain limited to the mode-0700,
  no-malicious-same-UID model; no categorical race-free claim is made.
- Physical I/O, controlled-cold state, and stable-media completion remain
  `Unavailable`.
- Concurrent `research/phase-4/g5-round-0` planning remains foreign to and
  excluded from G4 custody. This decision authorizes no G5 implementation or
  measurement in the current task.
