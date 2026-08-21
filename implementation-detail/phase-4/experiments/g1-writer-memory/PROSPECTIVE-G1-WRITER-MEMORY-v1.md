# Phase-4 G1 prospective SQLite writer-memory screen v1

Status: **FROZEN BEFORE MEASUREMENT — G1 CURRENT**  
Date: 2026-08-21  
Repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`  
Branch: `codex/empty-worktree`

## Phase boundary

```text
G0 COMPLETE at 286eb7a456165f5417ff0dfcfb603aed07f2e074
G1 CURRENT
G2 NOT STARTED
```

This is a prospective one-variable screen. It neither rewrites CP-0010's
historical identity nor starts G2, materialization, concurrency, H09, WP5, or
Phase 5.

## One implementation variable

Control runtime:

```text
PRAGMA cache_size=2000
PRAGMA cache_spill=20000
```

Candidate runtime:

```text
PRAGMA cache_size=2000
PRAGMA cache_spill=2000
```

The sole implementation variable is the benchmark-private connection setting
`PRAGMA cache_spill=2000;`. There is no flag, configuration layer, public API,
cache-policy abstraction, dependency, schema/profile/format change, worker,
queue, pool, async path, retry, VFS, second connection, or transaction/COMMIT
change.

The candidate preserves `journal_mode=DELETE`, `synchronous=FULL`,
`temp_store=FILE`, `mmap_size=0`, `page_size=4096`, `cache_size=2000`, one
synchronous caller-thread writer, one `BEGIN IMMEDIATE` transaction, one
publication COMMIT, fresh ambiguous-COMMIT reconciliation, atomic visible-head
publication, exact error precedence, and exact Q accounting.

## Frozen custody

| Item | SHA-256 | Size |
|---|---|---:|
| Starting HEAD / G0 | `286eb7a456165f5417ff0dfcfb603aed07f2e074` | — |
| FastCDC-v2 control source | `16e9beedd2fe49d6da65f89f53f488cffbfdcfc71f10477e854cd2d37d00e120` | — |
| Candidate benchmark source | `157699e0cd4cb1e3b5ec631cefb7c967ff7433bdeeb10ee1336e70961b402ad2` | — |
| Candidate source-only binary diff | `3e167cdcdc267ad18452f03960d6dd45a9ab1e137c0cc6b967722e65990e6a09` | — |
| FastCDC source, unchanged | `bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6` | — |
| Exact control executable | `454bc2f3deacd8581a3cc352c8b7495215cdc103a85580606246ea12bb25eba8` | 1,372,784 B |
| Once-built candidate executable | `42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55` | 1,372,784 B |
| Canonical-v2 profile | `94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b` | — |
| 100-MiB fixture | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` | 104,857,600 B |
| Common empty database | `8657363e0f90d61bdb911c138a734b66c6adf4cd2dcd50c63c1ca1dae814e30c` | 20,480 B |
| Common authority | `7855ea6096359925f639b91c8d6b9708cfe0bc0df4a3ffd97a280a8e9a9ded48` | 32 B |
| Common expectations | `a7489b01445e53aa8a0c5824059b8a6b04f92e15a3b6cf953fbb4c83d6b5e18a` | 1,096 B |

The result namespace is
`target/phase4-g1-writer-memory-cache-spill-20260821-v1`; the atomic sibling
lock is `target/phase4-g1-writer-memory-cache-spill-20260821-v1.lock`. The
runner refuses to start when either exists.

The methodology manifest freezes this document, `run_g1.py`,
`analyze_g1.py`, and the separately implemented `recompute_g1.py`. Its exact
SHA-256 is supplied to both dry-run and acquisition through
`G1_METHODOLOGY_SHA256`. The successful dry-run JSON is separately hash-bound
through `G1_DRY_RUN_SHA256` and copied into the sealed result.

## Common-base custody

Both arms use the same already sealed, logically empty Canonical-v2 base
triplet. Base preparation and all ten fresh row copies occur before the first
benchmark child.

For every invocation the runner:

1. creates a fresh row directory;
2. copies the same database, authority, and expectations bytes;
3. verifies all three hashes and source/copy modes;
4. records source and copy device/inode identities and requires distinct copy
   inodes, including no copy-inode reuse between rows;
5. changes only the fresh database/authority/expectations modes from
   `0444/0400/0444` to `0600/0600/0400`;
6. rehashes and requires byte identity after mode changes;
7. runs that row exactly once; and
8. records post-run modes and journal/WAL/SHM residue.

The retained fixture is copied once into the result input directory before
timing; fresh row directories use only a relative symlink to that frozen copy.
No mutated database is reused.

## Exact schedule and timer boundary

```text
warmup pair 0: AB
measured pair 1: AB
measured pair 2: BA
measured pair 3: AB
measured pair 4: BA
```

Total: 10 durable invocations, 8 measured rows. Measured arm temporal centers
must both equal 6.5. Each child is isolated under `/usr/bin/time -l` and uses
`--fast-row ... 104857600 write ... capture-only`.

The single global monotonic clock starts before result preparation, which is
strictly earlier than the required first-child boundary. It includes every
child, both analyses, disposition, payload manifest, terminal record, and
terminal verification. Everything must finish in less than 20,000,000,000 ns.
An exceeded clock seals `G1 TIMEOUT`; it never resumes.

The durable timer equations remain:

```text
durable_capture_total
  = canonical_cas_mapping_stage
  + precommit_closure_validation
  + sqlite_commit_durability

sqlite_commit_durability
  = commit_dispatch_to_return
  + commit_pre_and_post_dispatch
  + commit_caller_wrapper
```

Every row must report both equations true and retain one COMMIT dispatch, one
return, one successful return, zero COMMIT errors, and ordinary acknowledged
publication without reconciliation work.

## Hard semantic, work, durability, and storage gates

Every warmup and measured row must retain exactly:

```text
source/input bytes                  104857600
CDC occurrences                          5284
objects created / reused               5372 / 0
canonical bytes written              105122466
mapping bytes                            196174
SQL calls                                   5381
BLOB/row writes                            10748
transactions / COMMITs                       1 / 1
COMMIT dispatch/return/success/error     1 / 1 / 1 / 0
Q high-water / terminal                 86181 / 0
logical/apparent database             109199360 / 109199360
logical/apparent store                109199392 / 109199392
```

Exact identities are:

```text
source fingerprint  bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7
occurrence commit   5bf63ef8adad7bb373be4e968759997b385590d076d75a1488e1958b64a3f8e2
root                93d1b461b5cbf88e8d122ad4e90a15a6e3029c408f0fe02ef51c561e5a94c6d1
transition          2de8d2ce6b614373ba8fb8b29d3a3eccd535abfccc2654e7922514e5ca90fd89
closure             29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1
```

Every row also requires the exact executable for its arm, exact common-base
hash/mode/inode custody, `DELETE/FULL/FILE/0`, page size 4096, cache size 2000,
the arm's frozen spill threshold, exact publication/timer/error behavior, zero
journal/WAL/SHM residue, and terminal Q zero.

The position-balanced allocated-store candidate/control ratio must be at most
1.05; at least 3/4 pairs and both execution positions must be at most 1.05.
All four positive candidate allocation deltas are an unexplained contradiction
and fail the storage gate. Logical and apparent endpoints remain exact.

Any semantic, identity, authority, custody, durability, timer, transaction,
COMMIT, storage, Q, error, or residue failure is `G1 FAILURE`, regardless of
memory or speed. It is not eligible for a checkpoint commit.

## Memory, mechanism, and performance gates

All centers are position-balanced: compute the mean within each arm/position,
then the mean of the two position means.

PASS requires:

- candidate SQLite page-cache snapshot maximum / control at most 0.50;
- candidate maximum RSS / control at most 0.50;
- candidate peak footprint lower than control and within 0.10 ratio points of
  the RSS ratio, otherwise classify an unexplained contradiction;
- candidate durable-total / control at most 1.05;
- at least 3/4 measured pairs and both positions within the 1.05 wall rule;
- no arm with strictly monotonic four-pair wall movement whose first-to-last
  change exceeds 5% of that arm's mean;
- candidate dirty-cache writes / control at most 1.10;
- candidate cache spills strictly greater than control and candidate cache
  snapshot strictly lower than control; and
- the hard storage gates above.

User CPU, system CPU, mapping, proof, complete SQLite-COMMIT observation,
dispatch-to-return, RSS, footprint, cache before/before-dispatch/after-return,
hits/misses, dirty writes, spills, derived pager bytes, journal sampled
allocation, and logical/apparent/allocated endpoints are all retained and
reported. A faster COMMIT cannot rescue a slow total; a faster total cannot
rescue failed memory reduction.

The prospective mechanism is spill-up/cache-down: the candidate should spill
substantially more pages during mapping, retain substantially fewer dirty cache
bytes before COMMIT, reduce RSS/footprint, possibly increase mapping wall, and
possibly reduce COMMIT wall. These are hypotheses, not accepted facts.

Pager bytes are derived only as:

```text
SQLITE_DBSTATUS_CACHE_WRITE * 4096
```

No physical-media I/O, VFS read/write bytes, sync calls/wall, true SQLite cache
high-water, current dirty set, true journal peak, or temporary-file peak is
inferred. Those remain Unavailable with the frozen F1 reasons.

## Analysis and decision

`analyze_g1.py` recomputes chronology, all hard invariants, pair/position
statistics, temporal centers, memory/wall/CPU/dirty/spill/storage results, and
the disposition from raw rows. `recompute_g1.py` independently parses the raw
rows and recomputes the same gates and ratios without importing primary code.
Status, disposition, and every controlling ratio must agree within `1e-12`.

Decision rules:

- `G1 MEASURED PASS / STATIC CLOSURE REQUIRED` only when all measured gates
  pass and both analyses agree;
- `G1 NO-GO / RETAIN PREDECESSOR` when semantics pass but memory, protected
  wall, footprint consistency, mechanism direction, or dirty-write gates fail;
- `G1 FAILURE` or `G1 TIMEOUT` on any hard/protocol/custody/semantic/storage
  failure.

After measured PASS only, run:

```text
cargo test --workspace --offline --all-targets
cargo clippy --workspace --offline --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Static closure, source-diff audit, FastCDC custody, terminal-manifest
verification, and final read-only audit must pass before the final disposition
can become `G1 PASS / RETAIN`.

On measured NO-GO, preserve and seal evidence, restore only the one candidate
pragma using the smallest patch, retain the focused test only if it remains an
honest historical regression test (otherwise remove it with the same source
revert), verify exact G0 source custody, and create only the authorized
documentation checkpoint. No second threshold is tested.

## Commands frozen before timing

```text
cargo test --offline -p layerfs-engine --bin phase4_create_edit_benchmark g1_writer_memory_
cargo fmt --all -- --check
git diff --check

CARGO_TARGET_DIR=target/phase4-g1-writer-memory-build-20260821-v1 \
  cargo build --release --offline -p layerfs-engine \
  --bin phase4_create_edit_benchmark

G1_METHODOLOGY_SHA256=<frozen-manifest-sha256> \
  python3 implementation-detail/phase-4/experiments/g1-writer-memory/run_g1.py \
  --dry-run

G1_METHODOLOGY_SHA256=<frozen-manifest-sha256> \
G1_DRY_RUN_SHA256=<frozen-dry-run-sha256> \
  python3 implementation-detail/phase-4/experiments/g1-writer-memory/run_g1.py \
  --execute
```

The candidate is built once and never rebuilt after measurement begins.

## Irreversible failure and sealing rule

After any measured row starts there is no deletion, replacement row, selective
rerun, repair campaign, threshold/gate amendment, binary change, or continuation
after partial failure. Every terminal path after namespace creation records
actual row counts, binds methodology/source/binary/fixture/base hashes,
manifests every existing payload except the manifest/terminal circular trio,
binds the status and input records from the terminal, verifies the manifest,
records the final monotonic clock, seals files read-only and directories
non-writable, and removes the execution lock only after best-effort sealing.

No 1-MiB, 10-MiB, or 500-MiB row; edit; reopen; range read; materialization;
other cache threshold; other page size; G2 work; or later phase is authorized.
