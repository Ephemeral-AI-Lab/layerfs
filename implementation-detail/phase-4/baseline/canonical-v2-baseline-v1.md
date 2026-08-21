# Phase-4 Canonical-v2 baseline v1

- Status: **PASS / FROZEN — exact fresh-store Canonical-v2 baseline**
- Date: 2026-08-21
- Starting committed HEAD: `febc20f046bba84ccdce1256363d77799eabf2db`
- Profile ID: `94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b`
- Benchmark source: `16e9beedd2fe49d6da65f89f53f488cffbfdcfc71f10477e854cd2d37d00e120`
- Release executable: `f3dd4c9420cc7bb7e7390960db9bf6e4a4a44de3d15dc0573002d3172b570280`
- Comparison control: CP-0009 executable
  `9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7`

## Disposition

Canonical-v2 is the accepted Phase-4 baseline for subsequent optimization.
This freezes the exact source, executable, native-v2 profile, identities,
fresh-store behavior, transaction/COMMIT boundary, and validation evidence; it
does not integrate production, commit the worktree, start the next optimization
lane, or claim that all Phase-4 lanes are complete.

Known nonempty v1-to-v2 automatic migration remains unsupported. Opening that
combination must return `SchemaMigrationRequired` before read-write mutation.
CP-0009 remains the historical v1 control and rollback authority.

## What changed

Each file occurrence changed from the v1 68-byte tuple

```text
raw ChunkId[32] || u32be(raw length) || canonical ObjectId[32]
```

to the native-v2 36-byte tuple

```text
u32be(raw length) || canonical ObjectId[32]
```

The canonical ObjectId authenticates the complete framed `LFSO/Bytes` object.
The ordered occurrence commitment authenticates every `(length, ObjectId)` in
order. Full create no longer computes a separate raw-ChunkId lane or whole
source digest. Rejoin equality, CAS, `LogicalFile`, COW, mappings, deltas,
receipts, reconstruction, ranges, scrub, reopen, and SQLite authority all use
the same native-v2 identity/profile path.

The publication repair retains proof-derived, transaction-bound, move-only
authority and reduces the final publication boundary to the current-head check,
head write, and one COMMIT. Every candidate mutation reported zero graph
authentication in its `sqlite_commit` phase.

## Complete validation

Static validation passed:

- full workspace: **139 passed, 1 ignored, 0 failed**;
- clippy, all targets, `-D warnings`: PASS;
- rustfmt check: PASS;
- tracked whitespace diff check: PASS;
- Canonical-v2 source, experiment, and baseline untracked files: whitespace
  check PASS.

Four unrelated pre-existing untracked H05/research Markdown files retain
trailing-whitespace/EOF findings. They are outside this freeze and were not
rewritten; they do not overlap the validated source, runner, analyzer, baseline,
or sealed evidence.

The one-shot lifecycle campaign completed in 63.252107 seconds under a
119-second ceiling. It retained exactly 29 rows: 2 warmup rows, 1/10-MiB scale
pairs, two adjacent balanced 100-MiB pairs, seven paired lifecycle guards, and
five candidate-only guards. The result root has 303 manifested entries plus
manifest and verification, 305 files total, zero mismatch/extra/missing files,
read-only sealed files/directories, and no journal/WAL/SHM residue.

## Durable full create

| Size / sample | CP-0009 control | Canonical-v2 | Result |
|---|---:|---:|---:|
| 1 MiB scale | 7.700208 ms | 5.783334 ms | candidate faster |
| 10 MiB scale | 63.778334 ms | 46.255250 ms | candidate faster |
| 100 MiB pair 0, AB | 677.115208 ms | 523.121208 ms | **22.742659% faster** |
| 100 MiB pair 1, BA | 658.188834 ms | 501.306792 ms | **23.835415% faster** |
| 100 MiB position-balanced center | 667.652021 ms | 512.214000 ms | **23.281293% faster** |

The 1/10-MiB rows are single scale checks, not statistical claims. Historical
standalone subtraction is not used.

### 100-MiB phase breakdown

The two-pair arithmetic centers are:

| Phase | CP-0009 | Canonical-v2 | Change |
|---|---:|---:|---:|
| Canonical CAS + mapping | 512.301209 ms | 321.749854 ms | **−190.551355 ms** |
| Proof consumption | 0.043729 ms | 0.047855 ms | +0.004126 ms |
| SQLite observation / durable COMMIT | 155.307084 ms | 190.416292 ms | **+35.109208 ms** |
| Overall durable full create | 667.652021 ms | 512.214000 ms | **−155.438021 ms** |

The identity change produced the expected construction win. It did not improve
COMMIT; the COMMIT observations were 201.812916 and 179.019667 ms for v2 versus
169.323500 and 141.290667 ms for the control. Future durability work must use
fresh adjacent evidence rather than treating this two-pair difference as a
stable causal attribution.

### Exact 100-MiB work and storage

| Counter | CP-0009 | Canonical-v2 |
|---|---:|---:|
| CDC occurrences | 5,284 | 5,284 |
| Objects created / reused | 5,372 / 0 | 5,372 / 0 |
| SQL calls / BLOB writes | 5,381 / 10,748 | 5,381 / 10,748 |
| Canonical bytes written | 105,291,554 | 105,122,466 |
| Mapping bytes | 365,262 | 196,174 |
| Dirty pages / derived pager bytes | 26,676 / 109,264,896 | 26,659 / 109,195,264 |
| Logical DB bytes | 109,268,992 | 109,199,360 |
| Apparent store bytes | 109,269,024 | 109,199,392 |
| Allocated store bytes | 109,273,088 | 109,203,456 |
| Q high-water / terminal | 88,093 / 0 | 86,181 / 0 |

Logical/apparent/allocated endpoints are filesystem observations, not physical
I/O. Physical I/O, instructions, cycles, sync-call counts, and true cold-cache
state remain unavailable.

## Other lifecycle operations at 100 MiB

| Operation | CP-0009 | Canonical-v2 | Disposition |
|---|---:|---:|---|
| Same-count middle edit | 7.640250 ms | 6.960791 ms | no regression |
| +1 early | 6.055958 ms | 5.108458 ms | no regression |
| +1 middle | 4.569250 ms | 4.576000 ms | +0.006750 ms; no material regression |
| Warm logical materialization | 423.336958 ms | 338.775916 ms | no regression |
| Fresh-process logical materialization | 434.161458 ms | 366.356667 ms | no regression |
| Reopen / head | 1.804583 ms | 2.088334 ms | +0.283751 ms; no material regression |
| Authenticated returned 1-MiB range | 3.170000 ms | 2.279209 ms | no regression |

Candidate-only checks passed: one-byte early/middle/late were
6.410375/6.414750/6.725166 ms; first edit after reopen was 154.019083 ms; and
scrub-only was 176.882750 ms. These rows are correctness/behavior guards and
make no control-relative speed claim. The first-edit row’s exact lifecycle
equation includes reopen/head, authority establishment/full scrub, and durable
edit publication.

## Final audit

- exact source/binary/profile/fixture/root/transition/closure identities: PASS;
- strict native-v2 codec and error precedence: PASS;
- read-only v1 migration/profile rejection: PASS;
- one transaction / one publication COMMIT for every mutation: PASS;
- `FULL + DELETE`, timer equations, fresh reconciliation tests: PASS;
- exact bounded Q and terminal zero: PASS;
- candidate COMMIT phase with zero graph authentication: PASS;
- authority target runtime mode `0600`, distinct-copy custody: PASS;
- 29-row chronology and once-only invocations: PASS;
- both adjacent 100-MiB pairs faster: PASS;
- lifecycle protected-regression rule: PASS;
- independent analyzer recomputation equal to retained analysis: PASS;
- terminal manifest 303/303 and complete root closure: PASS.

The next Phase-4 candidate must be derived from this exact baseline and must
again use prospective, adjacent, short evidence. Canonical-v2 itself is closed;
do not keep modifying it while pursuing CDC, edit-locality, reopen authority,
materialization, or SQLite durability as separate variables.

Evidence:
[complete-validation report](../../../target/phase4-canonical-v2-complete-validation-20260821-v1/results-v1/REPORT-v1.md),
[raw rows](../../../target/phase4-canonical-v2-complete-validation-20260821-v1/results-v1/RAW-v1.jsonl),
[analysis](../../../target/phase4-canonical-v2-complete-validation-20260821-v1/results-v1/ANALYSIS-v1.json), and
[terminal manifest](../../../target/phase4-canonical-v2-complete-validation-20260821-v1/results-v1/TERMINAL-MANIFEST-v1.tsv).
