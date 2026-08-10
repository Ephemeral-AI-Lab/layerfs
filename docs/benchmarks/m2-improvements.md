# M2 improvements spec

Measured baseline and ranked optimization targets for the accepted M2 SQLite engine,
executed against the [`m2-minibench.md`](./m2-minibench.md) matrix before M3 starts. All
changes must preserve the M2 acceptance contracts: bounded memory, bounded statements,
exact usage accounting, workerd parity, and the host-neutral core.

## Measured baseline (2026-08-11, Node 24.11.1, file-backed WAL)

Baseline numbers from the mini-bench matrix before the M2 improvements (candidate
`2e06a44`, pure-JS SHA-256). After-measurement values appear in the outcomes table.

| Workload            | Measured (before)                                                                | Dominant cost                                              |
| ------------------- | -------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| Sequential write    | 17.9 MiB/s (100 MiB streamed); 7.19% storage overhead                            | pure-JS SHA-256 ~40%, SQLite/statements ~45%, FastCDC ~15% |
| Sequential read     | 43.8 MiB/s cold, 44.4 MiB/s warm (100 MiB streamed)                              | digest re-verification ~95%                                |
| Small random read   | 2.85 ms/op (4 KiB, cold cache)                                                   | per-op transaction + root-to-leaf descend + verify         |
| One-byte edit       | ~6.2 s/edit on the 100 MiB file (streamed fallback, O(file))                     | full-file re-chunk + re-persist                            |
| Materialization     | 33.8 MiB/s (100 MiB reopen-and-read)                                             | digest re-verification + descend                           |
| Pure-JS SHA-256     | 66 MiB/s                                                                         | -                                                          |
| node:crypto SHA-256 | 2,218 MiB/s (34x)                                                                | -                                                          |
| FastCDC chunking    | 179 MiB/s                                                                        | -                                                          |
| Storage overhead    | 7.19% fresh data (single file); ~52% on 100 x 1 MiB; +4.7 MiB rewrite of 100 MiB | metadata + changed chunks + page rounding                  |

## Ranked improvements

| #   | Change                                                                                                                                                 | Expected effect                                                                                                    | Evidence                                                        |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------- |
| R3  | Host-injected native hashing (WebCrypto/`node:crypto`) for write hashing and read verification; pure-JS remains the shared fallback                    | write 26 -> 42-50 MiB/s (~1.6-1.9x); read 61 -> 150-300 MiB/s (2.5-5x)                                             | 34x hashing headroom; hashing is ~40% of write and ~95% of read |
| R7  | Carry the authenticated cursor across read pulls under a pinned lease; raise pull window to the query-batch limit                                      | small reads 8 ms -> ~1 ms (8-10x); fewer transactions on sequential reads                                          | per-window transaction + re-descend today                       |
| R5  | Batch reconciliation/insertion edges (one `hash IN (...)` per leaf, batched queue inserts)                                                             | ~5-8 statements/entry -> ~1.5                                                                                      | statement counts in the 100,001-entry closure test              |
| R1  | Wire bounded local reconnection into the small-edit fallback; stream the leaf through path-copy instead of the flat `maxManagedResidentBytes/9` window | one-byte edit O(file) -> O(changed window), sub-10 ms; removes the default-leaf 16 MiB vs 14.2 MiB window mismatch | durable-edit tests; M3 acceptance depends on it                 |
| R6  | Derive WAL checkpoint/backpressure thresholds from a journal target instead of the 1 GiB ceiling                                                       | no multi-hundred-MiB WAL hysteresis on small filesystems                                                           | node-driver WAL tests                                           |

## Measured outcomes (after R3 + R5 + FastCDC copy reduction)

Measured on the mini-bench matrix after the M2 improvement commit; storage behavior
(dedup, fresh-data overhead, exact quotas) is unchanged.

| Workload            | Today (before)               | After R3+R5+copy (measured)    | After R1 (M3, planned)                   |
| ------------------- | ---------------------------- | ------------------------------ | ---------------------------------------- |
| Small edit (1 byte) | fallback ~6.2 s (O(file))    | fallback ~3.0 s/edit; B4 46 ms | sub-10 ms, never O(file)                 |
| Big write           | 17.9 MiB/s                   | 46.3 MiB/s (2.6x)              | ~same + storage ~0 for identical content |
| Big read            | 43.8 MiB/s                   | 121.2 MiB/s (2.8x)             | ~same + warm GB/s class with R7 (M3)     |
| Small random read   | 2.85 ms/op                   | 1.19 ms/op (2.4x)              | ~1 ms/op with R7 cursor carry (M3)       |
| Many small ops      | ~89 ms/edit (B4)             | ~46 ms/edit (B4)               | ~same with R7 (M3)                       |
| Storage             | 7.19% overhead, exact quotas | same                           | dedup on changed windows                 |
| Write statements    | 12,472 (A1)                  | 3,065 (A1, 4x fewer)           | ~same                                    |

## Safety constraints

- Memory: all new paths remain admission-bounded; cursor leases release on
  cancellation/failure/close; no O(file) buffers.
- CPU: per-unit-of-work statement/elapsed budgets stay; no O(n^2).
- Host neutrality: the hashing seam is a capability injected by the host adapter
  (`node:crypto` on the Node adapter; WebCrypto exists on Node and workerd but is async
  and is not used inside read transactions); pure-JS stays the shared fallback so M1
  golden vectors remain byte-identical on both runtimes. No node-only module may enter
  `packages/fs/src` algorithm paths (architecture gate enforces).
- Storage: exact `efs_usage` accounting and quota ceilings are untouched.

## Milestone mapping

- M2 scope (this spec): R3, R5, FastCDC copy reduction; R7's verification half.
- M3 scope: R7's cursor carry (read sessions), R1 (path-copy window + local
  reconnection), resident read caches, B01/B02/B03/B05 mini-benchmark readiness.
- M9 scope: DOFS comparisons, 80% bounded-range gate, 1.10x materialization gate,
  mmap/zero-copy profile, pack-file index evaluation.
