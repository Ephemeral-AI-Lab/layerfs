# WP04 baseline checkpoint 1

This checkpoint freezes the first honest release-mode diagnostic baseline for
the repaired Phase 4 WP4 SQLite/radix candidate. It is an optimization
comparison point, not WP4-M promotion evidence.

## Repository scope

- repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`
- branch: `codex/empty-worktree`
- parent checkpoint: `d566fc2fcb699e3ede4926d0ee443511e2c98f62`
- checkpoint name and commit subject: `wp04-baseline-checkpoint-1`
- pre-checkpoint implementation diff SHA-256:
  `920ae75f821795c6d727873f7c17a63a87f18e6afe2a603253e6fcd5cfdbd543`
- SQLite remains the authoritative Phase 4A disk engine.
- The discarded append-only/pack carrier was not restored.
- `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` was not modified.

The implementation portion of this checkpoint changes:

- `crates/layerfs-core/src/content/persistence.rs`
- `crates/layerfs-core/src/cow/persistence.rs`
- `crates/layerfs-core/src/validation.rs`
- `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs`
- `crates/layerfs-engine/tests/phase4_engine_parity.rs`

## Qualification status

The campaign completed successfully, but its WP4-M disposition is
**INCONCLUSIVE / NO-GO for promotion**:

- 144 isolated release invocations completed;
- 24 were warmups and 120 were measured rows;
- no row failed;
- every measured row preserved `qualification=false` and
  `throughput_measurement_admissible=false`;
- no file or directory profile was promoted, ranked, rejected, or deleted;
- K64/F64 and DIR256K remain frozen defaults only because no challenger passed
  the promotion gates;
- WP4-P was not run.

The release measurements below are valid as current-implementation diagnostic
baselines when the exact fixture, geometry, lifecycle, and isolation rules are
preserved. They are not admissible profile-selection evidence because logical
resident `Q`, 512 MiB scaling, low-SQL sensitivity, and parts of the full
Memory/SQLite parity matrix remain unresolved.

## Frozen fixture and executable

| Item | Value |
|---|---|
| retained fixture | S1-100 |
| exact source bytes | 104,857,600 |
| raw fingerprint | `bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7` |
| CDC references | 5,284 |
| CDC sequence fingerprint | `5bb376c3c54d8724973a7b160acab599f2f5cee4b4a56e855ff0cbe987425994` |
| build profile | `release` |
| debug assertions | `false` |
| executable SHA-256 | `d99c350f187f26b96826b8dff00f484387f3caf551f88721bb95ebbac3fa906f` |
| cache statement | `warm_or_unknown_after_manifest_preflight` |
| store statement | `fresh_logical_store_cache_unknown` |

The retained S1-512 fixture was also prepared and authenticated outside
timers: 536,870,912 bytes, 27,162 references, raw fingerprint
`84f895c546504bd80a343c7c7300b26cc010dad27c7c897efc6f37fc2821efc2`,
and CDC-sequence fingerprint
`8b9c305cc4e128acbbe16d6aea4d000f3a483604c7b5f914d953bcccd7225d0b`.
It was not measured in this campaign.

Build and campaign commands:

```text
cargo build --release -p layerfs-engine --bin phase4_create_edit_benchmark
target/release/phase4_create_edit_benchmark \
  --campaign target/wp4m-mini-20260818
```

Each candidate/operation used one warmup and five measured child processes.
Every row used an isolated SQLite database and authority sidecar. Fixture
preflight and edit-base preparation were outside measured intervals.

## Frozen file baselines

The primary optimization baseline is the frozen-default K64/F64 full-create
median: **1,847,956,667 ns complete lifecycle, or 54.113823 MiB/s**.

| Profile | Complete median | Min-max | Spread | Complete rate | Capture median | Capture rate |
|---|---:|---:|---:|---:|---:|---:|
| K64/F64 | 1.847956667 s | 1.812478500-1.921257750 s | 0.108779250 s | 54.113823 MiB/s | 1.079909292 s | 92.600370 MiB/s |
| K59/F101 | 1.831035041 s | 1.825179875-1.849767208 s | 0.024587333 s | 54.613919 MiB/s | 1.068803208 s | 93.562593 MiB/s |
| K256/F256 | 1.823051250 s | 1.807793166-1.889365167 s | 0.081572001 s | 54.853093 MiB/s | 1.061317208 s | 94.222537 MiB/s |

The challengers did not pass the exact selection gate:

| Challenger | Median improvement vs K64/F64 | Matched wins | 5% and 4-of-5 gate |
|---|---:|---:|---|
| K59/F101 | 0.916% | 2/5 | FAIL |
| K256/F256 | 1.348% | 3/5 | FAIL |

The often-cited 1.826079333-second K59/F101 row is one representative
iteration, not a campaign median. Its durable capture was 1.068803208 seconds
(93.562593 MiB/s), and its complete lifecycle was 1.826079333 seconds
(54.762133 MiB/s).

### File operation medians

| Profile | Full | Same middle | +1 early | +1 middle |
|---|---:|---:|---:|---:|
| K64/F64 | 1.847956667 s | 1.235875333 s | 1.226549417 s | 1.221839333 s |
| K59/F101 | 1.831035041 s | 1.211457125 s | 1.211175042 s | 1.216285666 s |
| K256/F256 | 1.823051250 s | 1.213041666 s | 1.221348417 s | 1.220315625 s |

Whole-file-equivalent throughput is intentionally not reported for edits.
Their primary metrics are latency and exact changed/suffix work.

## Lifecycle boundary

For the representative K59/F101 row, the disjoint lifecycle equation was:

| Phase | Wall time | Share |
|---|---:|---:|
| canonical CAS/mapping, with source CDC nested | 535.650125 ms | 29.33% |
| pre-COMMIT closure/reconstruction qualification | 405.246292 ms | 22.19% |
| durable SQLite COMMIT | 127.906791 ms | 7.00% |
| fresh reopen/head authentication | 1.105625 ms | 0.06% |
| fresh full scrub | 290.509584 ms | 15.91% |
| streamed reconstruction | 465.087666 ms | 25.47% |
| exact range verification | 0.573250 ms | 0.03% |
| complete lifecycle | 1,826.079333 ms | 100% |

```text
durable capture
= 535.650125 + 405.246292 + 127.906791
= 1,068.803208 ms

post-COMMIT lifecycle
= 1.105625 + 290.509584 + 465.087666 + 0.573250
= 757.276125 ms

complete lifecycle
= 1,068.803208 + 757.276125
= 1,826.079333 ms
```

Complete-lifecycle throughput is not SQLite write throughput. It includes
independent trust boundaries after durable publication. Future comparisons
must not remove, merge, nest, or double-count these phases to improve the
reported number.

## Representative work and resource evidence

The same K59/F101 row observed:

| Counter | Value |
|---|---:|
| CPU time | 1.780000 s |
| maximum RSS | 93,700,096 bytes |
| objects created | 5,377 |
| payload-row writes | 5,377 |
| payload-row reads | 16,151 |
| object authentications | 21,528 |
| canonical bytes written | 105,291,892 |
| canonical bytes authenticated | 421,324,006 |
| SQL statements | 26,906 |
| physical database bytes | 109,318,144 |
| allocated-store delta | 117,440,512 |
| total physical allocation | 117,465,088 |
| largest single canonical object | 32,781 bytes |

Peak journal bytes, peak temporary bytes, SQL preparation counts,
sync/fsync observations, process I/O, host physical I/O, query plans, and
logical resident `Q` were unavailable and were not replaced with zero or a
different metric.

## Algorithmic checkpoint

| Operation | Current established behavior | Status |
|---|---|---|
| full capture | linear in source bytes and produced objects; high per-object SQL/authentication amplification remains | supported, below target |
| same-count file edit | authenticated persisted routing; changed chunk/leaf plus ancestor-spine COW; no all-reference vector | supported path-local behavior |
| fixed-ordinal +1 | bounded persisted-reference traversal and exact suffix accounting | correctly `O(suffix)`, not logarithmic; alarm fails |
| ranges | zero/first/cross-chunk/leaf/branch/last/EOF probes authenticate active paths | supported path-local behavior |
| directory lookup | selected page plus authenticated ancestors | supported |
| directory replacement | one page plus index/wrapper/root state | supported |
| directory leading insertion | canonical greedy repartition rewrites all entries/pages | honestly `O(E)` |
| delta | real Add/Remove/Replace/Metadata decode and replay tests | substantially supported |
| publication/receipt | one SQLite mutation transaction, 216-byte protected receipt, lost-ack reconciliation tests | substantially supported |
| resident memory | external RSS observed, exact protected logical `Q` unavailable | unqualified |

K64/F64 `+1` work was deterministic:

| Edit | Suffix refs | Suffix raw bytes | Rewritten leaves | Rewritten branches | Mapping rewritten |
|---|---:|---:|---:|---:|---:|
| early | 5,284 | 104,857,600 | 83 | 2 | 365,495 bytes |
| middle | 2,642 | 52,377,184 | 42 | 2 | 185,915 bytes |

The fixed-ordinal `+1` publication ratios were approximately 42.2%-43.3% of
full capture, far above the frozen 5% alarm. This is a local gate failure, but
without protected `Q` and the 512 MiB slope it is not admissible evidence for
rejecting radix/COW.

## Directory baselines

| Profile | Create | Lookup | Replacement | Leading insert |
|---|---:|---:|---:|---:|
| DIR64K | 2.443970375 s | 1.168459 ms | 2.291693291 s | 2.430779083 s |
| DIR256K | 2.413669417 s | 1.525625 ms | 2.287713125 s | 2.410570291 s |
| DIR1M | 2.392542875 s | 5.162834 ms | 2.317154666 s | 2.397891292 s |

No directory challenger passed the 5% and 4-of-5 replacement-primary gate.

## Optimization targets

Comparisons must use the same profile and complete lifecycle.

1. First meaningful K64/F64 improvement: at least 5% median reduction and
   four of five matched wins. This requires complete lifecycle at or below
   1.755558834 seconds, equivalent to at least 56.961919 MiB/s.
2. Product durable-capture minimum: 100 MiB in at most 500 ms, or at least
   200 MiB/s. K64/F64 currently needs approximately 2.16x speedup.
3. Internal complete-lifecycle diagnostic: at most 500 ms. K64/F64 currently
   needs approximately 3.70x speedup, or a 72.9% wall-time reduction.
4. A 300 MiB/s durable-capture result is a later stretch target, not the
   immediate scope.

Measured optimization priority is canonical mapping/CAS traversal,
reconstruction, pre-COMMIT qualification, and fresh scrub. COMMIT is only
about 7% of the representative lifecycle. The next optimization should target
repeated row crossings and avoidable decode/copy/hash/authentication work
within each required phase without collapsing independent trust boundaries.

## Remaining promotion blockers

1. exact logical resident `Q` high-water;
2. measured 512 MiB file matrix and 100-to-512 slopes;
3. low-SQL prepared/batched sensitivity control;
4. exhaustive Memory-versus-SQLite operation and typed-error parity;
5. explicit Q/W/D admission-boundary evidence;
6. fixed-ordinal `+1` scaling evidence and an approved 100 GiB work budget;
7. a decision on whether fresh reopen must cross an OS-process boundary rather
   than only creating a fresh store/connection in the isolated row process.

The 100 GiB K64/F64 analytical projection is structurally supported using
checked arithmetic: 5,410,816 references, 84,544 leaves, and height 2. No
100 GiB timing is claimed.

## Verification and retained artifacts

Verification completed before this checkpoint:

```text
cargo test --workspace --all-targets       # 69 passed, 0 failed
cargo test --workspace --doc
cargo check --workspace --all-targets
cargo fmt --all -- --check
git diff --check
```

Retained artifacts, intentionally not committed because the campaign tree is
approximately 11 GiB:

- `target/wp4m-mini-20260818/wp4m-profile-selection.jsonl`
- `target/wp4m-mini-20260818/wp4m-profile-selection-summary.json`
- `target/wp4m-mini-20260818/wp4m-profile-selection-commands.txt`
- `target/wp4m-mini-20260818/wp4m-profile-selection-resources.stderr`
- `target/wp4m-mini-20260818/wp4m-profile-selection-failures.jsonl`
- `target/wp4m-mini-20260818/wp4m-retained-fixture-manifest.json`
- `target/wp4m-mini-20260818/wp4m-profile-selection-environment.json`

The next optimization pass must preserve these artifacts and this checkpoint,
must report capture and complete lifecycle separately, and must keep
qualification false until every frozen protected gate is satisfied.
