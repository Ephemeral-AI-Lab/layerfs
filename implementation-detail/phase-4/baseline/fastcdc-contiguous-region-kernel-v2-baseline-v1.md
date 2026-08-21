# Phase-4 FastCDC contiguous-region kernel v2 baseline v1

- Status: **PASS / FROZEN — next exact Canonical-v2 control**
- Date: 2026-08-21
- Starting committed HEAD: `daf4cefc1fd7861681de3f94bf042b556cc21ccb`
- Profile: `94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b`
- Candidate CDC source: `bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6`
- Candidate durable executable: `454bc2f3deacd8581a3cc352c8b7495215cdc103a85580606246ea12bb25eba8`
- Previous Canonical-v2 control: `f3dd4c9420cc7bb7e7390960db9bf6e4a4a44de3d15dc0573002d3172b570280`

## Disposition

The safe-Rust contiguous-region FastCDC kernel is retained as the next exact
Canonical-v2 Phase-4 control. It changes only execution of the boundary loop;
the Gear recurrence, chunk profile, boundaries, emitted bytes, identities,
schema, transaction, COMMIT, durability, Q, and storage contracts are
unchanged.

The sealed v1 active-mask-field result remains independently valid:
`Active-mask field caching: NO-GO`. V2 resolves only the broader
phase-oriented contiguous-region hypothesis. The previous Canonical-v2 binary
remains the historical rollback/control operand.

No commit, production integration, H09, SQLite/page-size, concurrency,
materialization, reopen-authority, migration, or another CDC candidate was
started.

## Mechanism

The scanner now:

1. bulk-fills the existing bounded chunk to 8 KiB;
2. resolves the rare pending pair once at fragment entry;
3. scans one fixed small-mask region and one fixed large-mask region through a
   shared out-of-line kernel with hash, cursor, limit, and masks in registers;
4. appends accepted input spans only at a cut, maximum, or fragment exit; and
5. retains only one odd trailing byte as pending.

There are no cached mask fields, `next_even`, per-pair target-region
comparison, dependency, unsafe block, worker, queue, SIMD framework, or second
execution profile.

## Corrected CDC-only screen

The machine-code gate proved equal 216-byte out-of-line timed wrappers, the
same post-timer stack-probe side, two fixed-mask region calls, masks in
registers, no region-loop target comparison, and no mutable mask loads.

One warmup `AB` and measured `AB / BA / AB / BA` completed in 4.639739 seconds
under the 20-second ceiling. Exact parity passed with one retained boundary TSV
authority.

| Pair | Order | Control | Candidate | Saved |
|---:|:---:|---:|---:|---:|
| 1 | AB | 145.603083 ms | 44.125667 ms | 101.477416 ms |
| 2 | BA | 145.247792 ms | 45.277250 ms | 99.970542 ms |
| 3 | AB | 143.771708 ms | 43.225375 ms | 100.546333 ms |
| 4 | BA | 142.467750 ms | 42.518083 ms | 99.949667 ms |
| **Position-balanced** | — | **144.272583 ms** | **43.786594 ms** | **100.485990 ms / 69.650094%** |

All 4/4 pairs and both positions favored the candidate. Arm sequence centers
were both 6.5. Every row consumed 104,857,600 bytes and emitted the exact same
5,284 boundaries, transcript
`b932a2a719ce671d58d06a1e8c1aa3c20b6f27d4cbe7cbf0ec7e369c6b97588d`,
reconstructed source, 8,219/32,768-byte observed min/max, and bounded
capacities.

## Adjacent durable A/B

The corrected campaign completed in 13.118682 seconds under the 120-second
ceiling. One initial preparation attempt failed before any row because the
sealed control mode was `0444`; the preserved v2 correction used a
byte-identical `0555` copy in a new namespace. No measured row was rerun.

| Pair | Order | Control | Candidate | Saved |
|---:|:---:|---:|---:|---:|
| 1 | AB | 417.721083 ms | 357.634792 ms | 60.086291 ms |
| 2 | BA | 408.509209 ms | 324.007875 ms | 84.501334 ms |
| 3 | AB | 415.845041 ms | 338.664583 ms | 77.180458 ms |
| 4 | BA | 415.584583 ms | 338.685125 ms | 76.899458 ms |
| **Position-balanced** | — | **414.414979 ms** | **339.748094 ms** | **74.666885 ms / 18.017420%** |

All 4/4 pairs and both positions favored the candidate; both temporal centers
were 6.5. The candidate passed the 10-ms, 2%, pair, position, mapping,
CPU/RSS/storage, and every correctness/durability gate.

### Phase breakdown

| Phase | Adjacent control | Candidate | Candidate change |
|---|---:|---:|---:|
| Canonical CAS + mapping | 296.131885 ms | 223.399198 ms | −72.732687 ms |
| Proof | 0.044657 ms | 0.072542 ms | +0.027885 ms |
| SQLite observation / durable COMMIT | 118.238437 ms | 116.276354 ms | −1.962083 ms |
| Durable total | 414.414979 ms | 339.748094 ms | **−74.666885 ms** |

The mapping direction is consistent with the clean CDC screen. COMMIT was not
claimed as a CDC mechanism.

The earlier frozen Canonical-v2 center remains 512.214000 ms. It is historical
context, not subtracted as adjacent evidence. The qualifying adjacent campaign
is the controlling performance result.

## Exact semantics and resources

Every durable row retained:

- source/scanned bytes: 104,857,600;
- CDC occurrences: 5,284;
- root `93d1b461b5cbf88e8d122ad4e90a15a6e3029c408f0fe02ef51c561e5a94c6d1`;
- transition `2de8d2ce6b614373ba8fb8b29d3a3eccd535abfccc2654e7922514e5ca90fd89`;
- closure `29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1`;
- objects created/reused: 5,372 / 0;
- canonical/mapping bytes: 105,122,466 / 196,174;
- SQL calls/BLOB writes: 5,381 / 10,748;
- one transaction, one successful publication COMMIT, `FULL + DELETE`,
  `temp_store=FILE`, `mmap_size=0`, and zero publication graph rescan;
- Q high-water/terminal: 86,181 / 0;
- logical/apparent database: 109,199,360 / 109,199,360 bytes;
- logical/apparent store: 109,199,392 / 109,199,392 bytes;
- allocated store: exactly 117,510,144 bytes in every arm/row;
- no journal/WAL/SHM residue.

Measured arm means were 0.36/0.285 s user CPU, 0.12/0.1175 s system CPU,
93,560,832/93,528,064-byte maximum RSS control/candidate. All paired resource
gates passed.

The candidate durable center crossed 500 ms and 400 ms, but not 333.333 ms or
250 ms.

Physical I/O, sync-call counts, instructions, cycles, true cold-cache state,
phase-local CPU, and unobserved heap work remain unavailable and are not
inferred.

## Static and independent closure

- full workspace offline/all-target tests: **141 passed, 1 ignored, 0 failed**;
- offline all-target Clippy with warnings denied: PASS;
- rustfmt, tracked whitespace, and relevant untracked whitespace: PASS;
- independent screen/durable arithmetic and invariant recomputation: PASS;
- screen/durable manifest revalidation: 30/30 and 79/79;
- v1 sealed manifest unchanged: PASS.

The exact candidate executable above is eligible as the next Phase-4 control.
