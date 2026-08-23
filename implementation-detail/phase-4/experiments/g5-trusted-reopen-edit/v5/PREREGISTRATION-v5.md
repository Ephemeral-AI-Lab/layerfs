# Phase 4 G5-1 v5 preregistration

Objective: make first edit after reopen operation-local only under the explicit
benchmark-private `TrustedLocalDev` contract while preserving Verified as the
default and byte-identical control behavior.

## Fixed architecture and threat boundary

The candidate is CAS + CDC + COW + canonical K64/F64 + SQLite. It changes no
schema, canonical bytes, receipt encoding, SQL write shape, journal/profile,
dependency, retry, worker, or production API. Trusted mode assumes
nonadversarial private-local DB path custody, provides no rollback freshness,
and may commit past unrelated corruption. Every fetched, supplied, generated,
or immutable incumbent object remains authenticated exactly. Touched corruption
fails before COMMIT in both modes. Verified remains the default and performs a
complete closure scrub; trusted never creates verified closure provenance or
carry-forward.

`verify_snapshot_closure(current_head, explicit_retained_roots)` independently
revalidates Store profile/authority/head receipt, each supplied transition, and
every object reachable from each supplied root. It does no writes and does not
scan unreachable object-table rows.

## Fast law

For this candidate the only admissible order is:

```text
focused tests
-> zero-row schedule assertion/dry-run
-> one complete <20-second screen
-> one frozen-source workspace/clippy/fmt/diff closure
-> one complete <=120-second gate
```

Any failure preserves v5. No row is deleted, replaced, reordered, or selectively
rerun. A source or method repair creates v6.

## Screen

The screen retains seven sequences: 1-MiB healthy parity; 1-MiB touched
identity corruption; 10-MiB unrelated-corruption policy distinction; 10-MiB
trusted commit then Verified reopen plus lost-ack/rollback cleanup; one 100-MiB
frozen-G4/G5-Verified/G5-Trusted first-edit comparison; one analogous middle
`+1` comparison; and one protected create/range sentinel. One trusted row above
25 ms or below 50% improvement rejects the mechanism but cannot establish a
percentile PASS.

## Gate schedule

Two comparisons use the retained operations
`first-edit-after-reopen`, `same-middle`, `one-byte-early`,
`one-byte-middle`, `one-byte-late`, `plus1-early`, and `plus1-middle`:

1. frozen G4 Verified versus G5 Verified; and
2. the same G5 executable in Verified versus Trusted mode.

Each comparison uses 20 adjacent balanced primary pairs and five adjacent pairs
for each secondary shape: 50 pairs/100 arm observations per comparison, 200 arm
observations total. G5 roles use persistent children over isolated prepared
copies. Frozen G4 is necessarily one-shot; its process launch is labeled and
only common internal intervals decide overhead.

## Hard equations and targets

```text
trusted complete closure scrub calls/bytes = 0/0
verified complete closure scrub bytes       > 0
trusted verified receipt-covered assumptions = 0
trusted verified carry-forward              = false
healthy root/transition/post DB+authority hashes/exact work = exact parity
transactions/publication COMMITs             = 1/1
operation and child terminal Q                = 0
```

The post-row database SHA-256 is labeled physical-byte parity only; it is not
called a logical database digest. Root, transition, authority SHA-256, and the
frozen mutation-work tuple provide the accompanying logical/state authority.

The first-edit timer is the exact sum of Store preflight, SQLite open/profile,
visible head/transition, edit-base scope, mapping/construction, proof,
publication COMMIT, and non-double-counted reconciliation. Trusted first-edit
p50/p95 must be `<=15/25 ms`; early/middle `+1` trusted p50 must be `<=15 ms`;
paired primary median improvement must be `>=50%` (`>=80%` is reported as the
strong expectation). Verified regressions require both ratio `>1.05` and mean
delta `>=1 ms`.

Maximum RSS is `20,971,520` bytes and each owned buffer is at most 1 MiB.
Transactions/COMMITs, SQL/BLOB work, exact errors, durability, reconciliation,
Q, descriptors, journals, temps, cleanup, lock custody, two analyzers, and final
manifest verification are hard gates.

## Mandatory limits

Rollback freshness is `NotProtected`. Controlled-cold and byte-level physical
I/O are `Unavailable`. The mode is benchmark-private/non-production. There is
no persistent authority or projection seed, no GC, no 500-MiB claim, no G6,
and no malicious-same-UID or cross-platform claim.
