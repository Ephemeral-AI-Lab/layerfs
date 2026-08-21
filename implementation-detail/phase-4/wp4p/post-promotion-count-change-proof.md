# Post-promotion count-changing construction-proof optimization

- Status: **PASS / RETAIN — CP-0007 complete**
- Date: 2026-08-21
- Starting commit: `febc20f046bba84ccdce1256363d77799eabf2db`
- Selected format: K64/F64 + DIR256K
- Production profile ID:
  `b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1`

## One changed variable

Replace complete pre-COMMIT file-closure replay for `+1` early/middle edits
with a private, move-only, single-use, transaction-local count-changing
construction proof. After successful COMMIT acknowledgement or exact
requested-visible reconciliation, carry the proof forward as an in-memory
same-open witness for the new head.

No canonical bytes, mapping grammar, profile ID, selected goldens, SQLite
schema, durability setting, transaction boundary, or serialized metadata may
change.

## Retained baseline

CP-0006 measured:

| Operation | Publication median | Mapping | Pre-COMMIT | COMMIT |
|---|---:|---:|---:|---:|
| same-count middle | 8.639167 ms | ~6.29 ms | ~0.28 ms | ~2.05 ms |
| `+1` early | 432.939417 ms | ~3.08 ms | ~426.20 ms | ~3.98 ms |
| `+1` middle | 432.324667 ms | ~2.14 ms | ~427.11 ms | ~2.87 ms |

The `+1` verifier authenticates approximately 105–106 MiB and 5,478–5,560
objects. Construction itself rewrites only 86 mapping objects/365,495 bytes
early or 45 objects/185,915 bytes middle.

## Required authority

The proof must bind:

```text
store instance / open identity / validation authority / integrity epoch
production profile ID / authority serial / transaction identity
complete prior generation/root/transition/receipt
operation / insertion ordinal / old and new counts and raw lengths
every prior authenticated reference occurrence consumed
inserted chunk evidence
every rewritten leaf/branch/root evidence and exact summaries
new workspace root / transition / requested visible head
```

Persisted receipts alone never authorize skipping after reopen. Rollback,
mutation, mismatch, prior/different/unresolved reconciliation, or proof reuse
invalidates provisional authority. No post-COMMIT fallible instrumentation may
relabel a successful publication.

## Bounds

```text
mutation and proof fold:
  O(changed CDC bytes + suffix references + rewritten mapping objects + H)

pre-COMMIT qualification:
  O(rewritten mapping objects + prefix/namespace/transition spine + H^2)

resident memory:
  O(K + F*H + bounded page/SQL/proof buffers)
```

The fixed-radix count-changing operation remains `O(suffix)`, worst-case
`Theta(N)`. This milestone makes no logarithmic or scale-independent claim.

## Direct-counter gates

For both retained 100-MiB `+1` rows, within the changed
mapping/qualification interval after the separately required prior-authority
scrub:

```text
complete chunk-payload BLOB replay:          0
construction proof consumptions:             1
transactions / COMMITs:                      1 / 1
terminal Q:                                   0
canonical bytes authenticated pre-COMMIT:    <= 2 MiB
pre-COMMIT object fetch/authentication count: <= 256
```

Suffix references, rewritten raw bytes, leaves, branches, mapping objects,
mapping bytes, roots, transitions, closure digests, reconstruction, and ranges
must remain exact.

## Performance decision

Use one warmup and three measured samples for same-count, `+1` early, and `+1`
middle, plus one fresh complete-roundtrip per result. The package hard wall is
120 seconds; no 512-MiB or 100-GiB runtime is permitted.

```text
required retain gate:
  each +1 median <= 50 ms
  >= 90% pre-COMMIT wall reduction
  3/3 measured wins
  predicted counter collapse

strong result:
  each +1 median <= 25 ms

stretch:
  each +1 median <= 15 ms
```

If the required gate passes, retain K64/F64 with honest suffix scaling. If the
product requires near-8–10-ms count-changing edits at multi-GiB/100-GiB scale,
stop before WP5 and write a new canonical prolly-tree specification; do not
smuggle a format change into this milestone.

## Terminal result

CP-0007 retained the exact promoted K64/F64 identities and measured:

| Operation | CP-0006 | CP-0007 | Reduction |
|---|---:|---:|---:|
| `+1` early | 432.939417 ms | 7.868417 ms | 98.182559% |
| `+1` middle | 432.324667 ms | 6.946583 ms | 98.393202% |

The pre-COMMIT proof medians are 0.024500 and 0.043209 ms with one proof
consumption and zero object/payload authentication. The mapping/proof-fold
interval authenticates 730,964 bytes/179 objects early and 371,804 bytes/97
objects middle. Both fresh 100-MiB edit round trips pass exact reopen, full
scrub, reconstruction, ranges, roots, transitions, and closure. Q is 55,375
bytes high-water and zero terminal.

The mandatory first same-open authority remains separately visible at
239.660791/245.128417 ms. The next transaction can consume the carried
same-open authority with zero canonical object authentication; reopen cannot.

Decision: **retain K64/F64 and keep WP4-P closed**. Current product authority
accepts honest `O(suffix)` count-changing work. A canonical prolly tree is
required only if product policy changes to demand near-8–10-ms
count-changing edits at multi-GiB/100-GiB scale.

Controlling report:
[CP-0007](../test-checkpoint-report/cp-0007-dirty-88ffb0bd6a30-count-change-proof.md).
