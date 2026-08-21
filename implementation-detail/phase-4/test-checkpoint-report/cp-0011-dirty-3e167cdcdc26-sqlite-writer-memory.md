# CP-0011 — SQLite writer-memory G1 checkpoint

Status: `G1 PASS / RETAIN-CACHE-SPILL-2000 / PHASE-4-ACTIVE`
Date: 2026-08-21
Parent checkpoint: CP-0010
Operation: 100-MiB durable full create
Campaign: one warmup `AB`, measured `AB / BA / AB / BA`
Rows: `10/10 complete; 8 measured`
Screen through terminal verification: `6.863690500 s`

## Decision

Retain `PRAGMA cache_spill=2000;` in the benchmark-private SQLite connection
initializer. G1 is complete and G2 is next. G2 was not started.

The setting reduced the position-balanced SQLite cache snapshot from
87,050,240 to 8,753,408 bytes and maximum RSS from 93,507,584 to 13,086,720
bytes. Durable total improved from 328.052657 to 308.884052 ms. All semantic,
identity, durability, transaction, COMMIT, Q, storage, custody, residue,
independent-analysis, manifest, and static gates passed.

## Checkpoint identity

| Field | Value |
|---|---|
| Repository / branch | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` / `codex/empty-worktree` |
| G0 parent HEAD | `286eb7a456165f5417ff0dfcfb603aed07f2e074` |
| Candidate source-only diff | `3e167cdcdc267ad18452f03960d6dd45a9ab1e137c0cc6b967722e65990e6a09` |
| Candidate source | `157699e0cd4cb1e3b5ec631cefb7c967ff7433bdeeb10ee1336e70961b402ad2` |
| FastCDC source | `bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6` |
| Control executable | `454bc2f3deacd8581a3cc352c8b7495215cdc103a85580606246ea12bb25eba8` |
| Candidate executable | `42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55` |
| Profile | `94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b` |
| Fixture | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| Measured payload manifest | `f02664ea4d82a73126584ed6197b4cea5bc3a21fc08a1562488a7c253dac2a3c` |
| Terminal / verification | `54692f9a...2f07` / `0c89f991...2b85` |

The sole implementation variable is the runtime spill threshold. Cache size
remains 2000, page size 4096, and the SQLite durability profile remains
`DELETE + FULL + FILE + mmap_size=0`.

## Durable performance

| Pair | Order | Control | Candidate | Ratio |
|---:|:---:|---:|---:|---:|
| 1 | AB | 326.238667 ms | 305.692750 ms | 0.937022 |
| 2 | BA | 341.450500 ms | 313.144708 ms | 0.917101 |
| 3 | AB | 325.767792 ms | 302.758417 ms | 0.929369 |
| 4 | BA | 318.753667 ms | 313.940334 ms | 0.984900 |
| **Position-balanced** | — | **328.052657 ms** | **308.884052 ms** | **0.941569** |

The candidate won 4/4 pairs. Both position ratios passed at 0.961777 and
0.921611, measured arm centers were both 6.5, and no material monotonic drift
was observed. Candidate throughput is `323.746076 MiB/s`.

## Memory and pager mechanism

| Observation | Control | Candidate | Ratio/change |
|---|---:|---:|---:|
| Page-cache snapshot maximum | 87,050,240 B | 8,753,408 B | 0.100556 |
| Maximum RSS | 93,507,584 B | 13,086,720 B | 0.139954 |
| Peak footprint | 92,258,730 B | 11,776,360 B | 0.127645 |
| Dirty writes | 26,659 pages | 26,668 pages | 1.000338 |
| Derived pager bytes | 109,195,264 B | 109,232,128 B | +36,864 B |
| Cache spills | 6,658 pages | 24,658 pages | 3.703515x |

Cache used before work remained 14,592 bytes in both arms. Before COMMIT it
was 87,050,240 versus 8,753,408 bytes; after COMMIT both arms were 8,753,408
bytes. Peak footprint moved consistently with RSS and cache. Dirty-write
amplification was only 0.033760%, well inside the 10% rule.

## Phase and CPU decomposition

| Phase/resource | Control | Candidate |
|---|---:|---:|
| Canonical CAS + mapping | 217.150687 ms | 263.736949 ms |
| Construction proof | 0.045219 ms | 0.043208 ms |
| SQLite COMMIT observation | 110.856751 ms | 45.103896 ms |
| COMMIT dispatch-to-return | 110.718740 ms | 44.957500 ms |
| User CPU | 0.2850 s | 0.2700 s |
| System CPU | 0.1075 s | 0.1075 s |

Earlier spilling increased mapping wall and reduced remaining COMMIT pressure;
the protected total still improved. COMMIT and pager observations do not prove
sync-call or physical-media behavior.

## Exact work, durability, Q, and storage

Every row retained:

```text
source / CDC bytes                 104,857,600
CDC occurrences                         5,284
objects created / reused              5,372 / 0
canonical / mapping bytes          105,122,466 / 196,174
SQL calls / BLOB writes                 5,381 / 10,748
transactions / COMMITs                     1 / 1
COMMIT dispatch / return / success       1 / 1 / 1
COMMIT errors                                 0
Q high-water / terminal                86,181 / 0
logical/apparent database           109,199,360 / 109,199,360
logical/apparent store              109,199,392 / 109,199,392
allocated store                     117,510,144
```

Exact source fingerprint, occurrence commitment, root, transition, closure,
profile, timer equations, publication status, error behavior, common-base
hash/mode/inode custody, and zero journal/WAL/SHM residue passed in every row.

## Validation

- focused `g1_writer_memory_` test: PASS;
- full workspace/all-target offline tests: 142 passed, 1 ignored, 0 failed;
- Clippy all targets with warnings denied: PASS;
- rustfmt and diff checks: PASS;
- primary analysis: PASS;
- independent recomputation: exact agreement;
- measured manifest: 78/78 entries verified;
- static closure/manifest: PASS.

Full evidence and limitations are frozen in the
[G1 baseline report](../baseline/sqlite-writer-memory-cache-spill-2000-baseline-v1.md)
and its [manifest](../baseline/sqlite-writer-memory-cache-spill-2000-baseline-v1-manifest.tsv).

## Limitations and scope confirmation

True cache high-water, current dirty set, VFS I/O calls/bytes, sync calls/wall,
true journal/temp peaks, physical-media bytes, instructions, cycles, and
phase-local CPU remain Unavailable.

No 500-MiB work, second cache threshold, other page size, materialization,
reopen, edit, range-read, concurrency, H09, G2, WP5, push, amend, merge, or
rebase occurred.
