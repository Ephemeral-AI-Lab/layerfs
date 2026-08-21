# Phase-4 SQLite writer-memory `cache_spill=2000` baseline v1

- Status: **G1 PASS / RETAIN — FROZEN**
- Date: 2026-08-21
- G0 parent commit: `286eb7a456165f5417ff0dfcfb603aed07f2e074`
- Profile: `94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b`
- FastCDC source: `bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6`
- Control executable: `454bc2f3deacd8581a3cc352c8b7495215cdc103a85580606246ea12bb25eba8`
- Candidate source: `157699e0cd4cb1e3b5ec631cefb7c967ff7433bdeeb10ee1336e70961b402ad2`
- Candidate source-only diff: `3e167cdcdc267ad18452f03960d6dd45a9ab1e137c0cc6b967722e65990e6a09`
- Retained candidate executable: `42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55`

## Disposition

G1 retains the benchmark-private runtime setting:

```text
PRAGMA cache_spill=2000;
```

The one-variable policy reduced the position-balanced SQLite page-cache
snapshot by 89.944% and maximum RSS by 86.005%, while durable total improved
5.843%. Exact identities, work, durability, one-transaction/one-COMMIT shape,
Q, logical/apparent/allocated storage, error behavior, custody, residue, and
static closure all passed.

No schema, profile, object format, metadata, page size, cache size, journal,
synchronous, temp-store, mmap, connection count, worker, queue, retry, VFS,
CDC/CAS/COW/mapping algorithm, or reopen authority changed. G1 is complete.
G2 is next but was not started.

## Prospective protocol and one-shot execution

The preregistered sequence was executed exactly once:

```text
warmup AB
measured AB / BA / AB / BA
```

There were 10/10 durable invocations and 8/8 measured rows. Measured arm
temporal centers were both 6.5. The result completed through payload manifest
and terminal verification in `6.863690500 s`, below the 20-second ceiling.
No row was deleted, replaced, or rerun.

| Pair | Order | Control | Candidate | Candidate/control |
|---:|:---:|---:|---:|---:|
| 1 | AB | 326.238667 ms | 305.692750 ms | 0.937022 |
| 2 | BA | 341.450500 ms | 313.144708 ms | 0.917101 |
| 3 | AB | 325.767792 ms | 302.758417 ms | 0.929369 |
| 4 | BA | 318.753667 ms | 313.940334 ms | 0.984900 |
| **Position-balanced** | — | **328.052657 ms** | **308.884052 ms** | **0.941569** |

All 4/4 pairs and both execution positions were within the protected 5% wall
rule. Position ratios were 0.961777 and 0.921611. Neither arm exhibited a
material monotonic drift.

The retained candidate throughput is `323.746076 MiB/s` for this exact
100-MiB durable capture/publication boundary. The adjacent G1 control was
`304.829112 MiB/s`.

## Memory result

| Resource | Control | Candidate | Candidate/control | Result |
|---|---:|---:|---:|---|
| SQLite cache before work | 14,592 B | 14,592 B | 1.000000 | exact |
| SQLite cache before COMMIT | 87,050,240 B | 8,753,408 B | 0.100556 | PASS |
| SQLite cache after COMMIT | 8,753,408 B | 8,753,408 B | 1.000000 | exact endpoint |
| Snapshot maximum | 87,050,240 B | 8,753,408 B | **0.100556** | **89.944% lower** |
| Maximum RSS | 93,507,584 B | 13,086,720 B | **0.139954** | **86.005% lower** |
| Peak footprint | 92,258,730 B | 11,776,360 B | **0.127645** | consistent |

The candidate snapshot maximum is approximately 8.35 MiB, maximum RSS 12.48
MiB, and peak footprint 11.23 MiB. The RSS and footprint movement agrees with
the SQLite cache movement; this is process-resource evidence, not a claim that
page-cache counters alone prove allocator release.

## Dirty-write and spill mechanism

| Pager observation | Control | Candidate | Change |
|---|---:|---:|---:|
| Cache hits | 27,575 | 27,566 | -9 |
| Cache misses | 3 | 12 | +9 |
| Dirty cache writes | 26,659 pages | 26,668 pages | +9 / +0.033760% |
| Derived pager bytes | 109,195,264 B | 109,232,128 B | +36,864 B |
| Mid-transaction spills | 6,658 pages | 24,658 pages | +18,000 / 3.703515x |

Every measured pair reproduced the same dirty-write and spill counts. The
candidate stayed well inside the 10% dirty-write limit and demonstrated the
preregistered spill-up/cache-down mechanism. Derived pager bytes equal dirty
cache writes times the observed 4,096-byte page size; they are not physical
media I/O.

## Phase and CPU results

| Phase/resource | Control | Candidate | Candidate change |
|---|---:|---:|---:|
| Canonical CAS + mapping | 217.150687 ms | 263.736949 ms | +46.586261 ms |
| Construction proof | 0.045219 ms | 0.043208 ms | -0.002011 ms |
| SQLite COMMIT observation | 110.856751 ms | 45.103896 ms | -65.752855 ms |
| COMMIT dispatch-to-return | 110.718740 ms | 44.957500 ms | -65.761240 ms |
| Durable total | 328.052657 ms | 308.884052 ms | **-19.168604 ms** |
| User CPU | 0.2850 s | 0.2700 s | -0.0150 s |
| System CPU | 0.1075 s | 0.1075 s | exact center |

The earlier spills raised mapping wall, while reduced dirty pressure near
COMMIT lowered the observed COMMIT interval by more. The controlling claim is
the protected durable total; COMMIT wall is not relabeled as sync-call or
physical-I/O evidence.

## Exact semantics, work, and storage

Every warmup and measured row retained:

```text
source/input bytes                 104,857,600
CDC occurrences                         5,284
objects created / reused              5,372 / 0
canonical bytes written             105,122,466
mapping bytes                           196,174
SQL calls                                 5,381
BLOB/row writes                          10,748
transactions / COMMITs                     1 / 1
dispatch / return / success / error    1 / 1 / 1 / 0
Q high-water / terminal                86,181 / 0
logical/apparent database           109,199,360 / 109,199,360
logical/apparent store              109,199,392 / 109,199,392
allocated store                     117,510,144
sampled journal allocation maximum       20,480
```

Every row retained the exact source fingerprint, occurrence commitment, root,
transition, closure, profile, construction counters, publication status,
timer equations, `DELETE/FULL/FILE/0`, page size 4096, cache size 2000, the
arm's frozen spill threshold, byte-identical common-base custody, post-run
modes, and no journal/WAL/SHM residue.

The allocated-store ratio was exactly 1.0 in every pair and both positions.
There is no new serialized metadata or temporary-file residue.

## Validation and sealed evidence

- focused G1 runtime-policy test: 1 passed;
- full workspace offline/all-target tests: **142 passed, 1 ignored, 0 failed**;
- offline all-target Clippy with warnings denied: PASS;
- rustfmt and diff checks: PASS;
- primary analysis: PASS;
- separately implemented independent recomputation: PASS and exact agreement;
- measured payload manifest: 78 entries, verified;
- measured terminal clock: PASS under 20 seconds;
- static closure manifest: 1 entry, verified;
- FastCDC source custody: exact.

Principal evidence hashes:

| Evidence | SHA-256 |
|---|---|
| Preregistration | `d73b3c070ddf17635f1e9e5ed8a40296bf7c5a884a283ca955d274a29858c660` |
| Methodology manifest | `da9a54d938275fb1064ecc8fa511521951dd5b308c2dc784d9383a00059e2116` |
| Zero-row dry run | `40e0324e651f7c3dc2e78c939591691c6c799c16b29d0a378b4eb89aed4a50fe` |
| Raw JSONL | `3b4ca568ac3fbf3dd32fc1fb74f2bd3b14bad5aa3800e964cf47cbd847a58520` |
| Primary analysis | `2c2a042930d7b97eba6115953201737e30282e26e2e9b0c983ad33ea0e636187` |
| Independent recomputation | `ddc289b7b612857204f288c16c7404b14fe362727af41398359a7c59ef3e1f9f` |
| Payload manifest | `f02664ea4d82a73126584ed6197b4cea5bc3a21fc08a1562488a7c253dac2a3c` |
| Terminal | `54692f9a8d4445bb7c6e17738b0bbb781c8554aad8111d881aa3826d35fc2f07` |
| Terminal verification | `0c89f9913b09ffe1259419b532e70e8d124244e0a942d6f8db20d4cdaeca2b85` |
| Static closure | `8c512b39a04481174fb4e9729d5385284d63e9fd5eb10b8a56f144b400d47566` |
| Static manifest | `5f45a0be4123ee6f440dd8388ce8ba903b8036f8a824c5651ab2a9538f03245c` |

Measured evidence is sealed at
`target/phase4-g1-writer-memory-cache-spill-20260821-v1`; static evidence is
sealed at `target/phase4-g1-writer-memory-static-20260821-v1`.

## Limitations and stop boundary

True continuous SQLite cache high-water, current dirty pages, VFS main/journal
read/write calls and bytes, sync calls/wall, true journal peak, temporary-file
peak, byte-level host physical I/O, instructions, cycles, and phase-local CPU
remain Unavailable. No physical-I/O or sync causality is inferred.

No 500-MiB work, second cache threshold, other page size, materialization,
reopen, edit, range-read, concurrency, H09, G2, WP5, or Phase-5 work occurred.
