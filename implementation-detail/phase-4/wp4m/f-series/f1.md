# WP4-M F1 — COMMIT and physical-I/O observability

## Preregistration — frozen before F1 source edits

- Date: 2026-08-19.
- Scope: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` only, branch
  `codex/empty-worktree`.
- Classification: diagnostic observability. F1 changes no storage algorithm,
  write shape, format, profile, schema, metadata, durability setting, worker,
  retry, transaction, or COMMIT count.
- F1 is not a throughput optimization and cannot select or promote a profile,
  integrate production, start F2, or complete Phase 4.
- Required terminal decision: exactly one of **PASS** or **FAIL / REVISE**;
  implementation action is exactly one of retain, revise, or revert.

### Frozen F0 custody

The pre-edit audit established this exact state:

| Item | Frozen value |
|---|---|
| Repository | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` |
| Branch | `codex/empty-worktree` |
| HEAD | `a6aa42e53effc29246b0e1838e266f518d51dc18` |
| Tree | `f4c186790fe9a70a9768a7240987fef7967cd15f` |
| Initial tracked/untracked status | clean |
| Initial `git diff --binary HEAD` SHA-256 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| F0 commit | `26f4f10122a16dd14474e93076c92f80876b798f` |
| F0-to-current documentation patch SHA-256 | `6c1f0945fd08669e7406f911b61ab8f38f7f9f2d69311a7b677b8aba217b20a9` |
| F0 manifest SHA-256 | `0c5711abfdad902c0db986590d09fff64ea46baf907ecb4e50dbb71ff67200e0` |
| Benchmark source SHA-256 | `0a078b25216fdc4da83722807dd8e921b523f99f074c86e5480a38e2a9ea2061` |
| Controlling M4.5 spec SHA-256 | `739620380446c8fc2fee5f7edc96c867bc32ed83bb6b54dcc98ecd76d5eab4c8` |
| SQLite visible-head spec SHA-256 | `cfddcc291cfff40ffcfd19e8e93ba2a4e51b3b16c412d137ece5463acc7625df` |
| F0 control build command | `CARGO_TARGET_DIR=target/wp4m-f1-commit-io-k64-20260819-v1/control-build cargo build --release --offline -p layerfs-engine --bin phase4_create_edit_benchmark` |
| Rebuilt F0 control SHA-256 | `7c395935457f99acfd3b02e08cdba976b971c61a407aeb65c243f2f26ddaf1a2` |
| Rebuilt versus frozen v4 executable | byte-identical |

The control was built once from the clean F0 source into the versioned F1
artifact root before this file or any source was edited. Its bytes are frozen
at:

```text
target/wp4m-f1-commit-io-k64-20260819-v1/
  binaries/phase4_create_edit_benchmark-f0-control
```

Toolchain and host:

| Item | Frozen value |
|---|---|
| Rust | `rustc 1.96.0 (ac68faa20 2026-05-25)`; LLVM 22.1.2 |
| Cargo | `cargo 1.96.0 (30a34c682 2026-05-25)` |
| Target | `aarch64-apple-darwin` |
| Host | Apple M3 Max, 38,654,705,664 bytes RAM |
| OS | macOS 26.4.1 build 25E253; Darwin 25.4.0 |
| rusqlite / libsqlite3-sys | 0.40.2 / 0.38.2 |
| Runtime SQLite | 3.51.0 |

### One variable and hypothesis

The single variable is synchronous caller-thread observation at the existing
candidate `Store::publish`/SQLite connection and explicit filesystem
snapshots. The hypothesis is:

> Existing SQLite connection-status counters and bounded snapshots can
> separate publication staging, COMMIT dispatch-to-return, reconciliation,
> pager-cache work, endpoint allocation, process CPU, and block-operation
> observations without changing logical work, storage bytes, transaction or
> COMMIT count, durability, identities, or write shape, and with no more than
> 5% retained full-create overhead.

Classification: diagnostic observability. Before and after algorithmic bounds
remain unchanged:

```text
full create work       = Theta(source bytes + references)
resident semantic Q    = existing bounded windows and mapping frontier
writer transactions    = 1
publication COMMITs    = 1
```

No F1 counter authorizes F2's construction proof or F3 batching.

## Preregistered observation matrix

Every emitted F1 observation has a machine-readable classification and a
source. A numeric zero is emitted only for a supported counter that was read
successfully and actually returned zero.

| Field / fact | Classification | Source / API | Unit / equation / fallback |
|---|---|---|---|
| `commit_dispatches` | Observed | existing checked increment immediately before `sqlite3_step` for `COMMIT` through `Connection::execute_batch("COMMIT")` | calls; normal mutation row must equal 1 |
| `commit_returns` | Observed | checked increment immediately after the SQLite COMMIT call returns | calls; normal row must equal 1 |
| `commit_return_status` | Observed | `Result` returned by the COMMIT call | `ok` or `error`; not inferred from wall time |
| `commit_dispatch_to_return_wall_ns` | Observed | `Instant` immediately before and after the COMMIT call | nanoseconds; successful normal-row acknowledgement interval |
| `commit_publish_call_wall_ns` | Observed | `Instant` around the existing `Store::publish` call | nanoseconds; includes head staging, COMMIT, and any reconciliation |
| `commit_pre_and_post_dispatch_wall_ns` | Derived | publish call minus dispatch-to-return | `publish_call_wall_ns - dispatch_to_return_wall_ns`; includes staging and, when invoked, reconciliation; never called sync time |
| `commit_caller_wrapper_wall_ns` | Derived | existing outer SQLite-COMMIT phase minus publish-call wall | `sqlite_commit_durability_wall_ns - commit_publish_call_wall_ns` |
| commit timer equation | Derived hard gate | preceding timers | `sqlite_commit_durability = publish_call + caller_wrapper = dispatch_to_return + pre_and_post_dispatch + caller_wrapper` |
| `commit_reconciliation_calls` | Observed | fresh independent reconciliation entry | calls; zero is supported on an ordinary acknowledged COMMIT |
| `commit_reconciliation_wall_ns` | Observed | `Instant` around fresh independent reconciliation | nanoseconds; nested within post-return publication time, not additive to the commit equation |
| reconciliation result | Observed | existing `RequestedVisible` / `PriorVisible` / `DifferentHead` / `Ambiguous` classifier | enum; ordinary acknowledged success remains `RequestedVisible` in existing provenance semantics |
| `sqlite_page_cache_used_bytes_*` | Observed | `sqlite3_db_status(SQLITE_DBSTATUS_CACHE_USED)` on the measured connection | approximate bytes at reset/before-dispatch/after-return; API return code checked |
| SQLite page-cache true high-water | Unavailable | SQLite 3.51 header states CACHE_USED high-water is always zero | unsupported; no zero is reported as an observation |
| `sqlite_page_cache_snapshot_max_bytes` | Derived | max of supported discrete cache-used snapshots | bytes; explicitly not a true continuous high-water |
| `sqlite_cache_hits` / `sqlite_cache_misses` | Observed | reset then read `SQLITE_DBSTATUS_CACHE_HIT/MISS` | pager-cache events during durable interval |
| `sqlite_main_db_dirty_pages_written` | Observed | reset then read `SQLITE_DBSTATUS_CACHE_WRITE` | dirty cache entries/pages written to the main DB in rollback mode; not OS write calls |
| `sqlite_main_db_pager_write_bytes` | Derived | dirty pages written times observed `PRAGMA page_size` | pager-level page bytes; explicitly not physical media bytes |
| `sqlite_cache_spill_pages` | Observed | reset then read `SQLITE_DBSTATUS_CACHE_SPILL` | dirty entries spilled mid-transaction |
| dirty pages currently resident | Unavailable | SQLite connection-status API has writes/spills but no current dirty-page count | unsupported with current permitted API |
| main DB read/write calls and bytes | Unavailable | no SQLite status/trace API exposes VFS xRead/xWrite totals | would require a VFS shim or privileged syscall trace; both excluded |
| journal read/write calls and bytes | Unavailable | same as main DB; CACHE_WRITE excludes rollback-journal writes | no substitution from pager writes or file length |
| sync calls and sync wall | Unavailable | requires VFS xSync interception or privileged syscall tracing | no VFS is authorized; `fs_usage` requires root and `dtruss` requires additional DTrace privileges on this host |
| main DB logical bytes | Observed | `PRAGMA page_count * PRAGMA page_size` with checked arithmetic | bytes at explicit snapshots |
| main DB/journal/authority apparent bytes | Observed | filesystem metadata length | bytes at pre-row, COMMIT dispatch, COMMIT return, and post-lifecycle snapshots |
| main DB/journal/authority allocated bytes | Observed | macOS `st_blocks * 512` | allocated bytes at the same snapshots |
| journal sampled allocation maximum | Derived | max of explicit snapshot allocations | bytes; a sampled lower bound, not true peak |
| journal true peak allocation | Unavailable | DELETE journal can grow and disappear between synchronous snapshots | no worker/sampler/VFS added |
| temporary-file peak allocation | Unavailable | `temp_store=FILE` exposes neither filenames nor peak allocation through SQLite status | no directory-wide guess or zero substitute |
| process user CPU / system CPU | Observed externally | `/usr/bin/time -l` per isolated child | seconds and derived nanoseconds, preserved separately |
| process block input/output operations | Observed externally | `/usr/bin/time -l` per isolated child | operation counts; explicitly not byte counts or proof of media I/O |
| process RSS / peak footprint / instructions / cycles | Observed externally | `/usr/bin/time -l` | existing units; unavailable fields remain strings with reasons |
| byte-level host physical I/O | Unavailable | permitted host tools do not expose it without privilege | never derived from logical/apparent/allocated bytes, RSS, Q, block operations, or wall time |
| logical Q | Observed | existing checked charge/decharge tracker | bytes; terminal must be exactly zero |

`sqlite3_trace_v2` is not used: the installed header documents statement,
profile, row, and close events, not VFS read/write/sync calls. A new VFS,
sampling thread, worker, async path, cache, pool, or secondary database would
violate the one-variable F1 scope.

## Timer and counter boundaries

SQLite status counters are reset on the already-open measured connection
immediately before the durable capture timer. Snapshots are taken:

```text
durable observation reset
  -> existing mapping/CAS work
  -> existing pre-COMMIT qualification
  -> complete-head staging inside Store::publish
  -> filesystem/cache snapshot immediately before COMMIT dispatch
  -> COMMIT dispatch
  -> SQLite call return / acknowledgement classification
  -> filesystem/cache snapshot immediately after return
  -> fresh reconciliation only when required
  -> Store::publish result
```

The existing phase equations remain exact per row:

```text
durable_capture_total
  = canonical_cas_mapping_stage
  + precommit_closure_validation
  + sqlite_commit_durability

complete_lifecycle_total
  = durable_capture_total
  + fresh_reopen_head
  + fresh_full_scrub
  + reconstruction
  + range_verification
```

The new disjoint COMMIT equation is the hard gate stated in the matrix. The
reconciliation timer is a named nested diagnostic and is never added twice.

## Focused correctness preregistration

The minimal new direct tests must prove:

1. successful COMMIT has one dispatch, one return, `ok`, no reconciliation,
   and exact timer equations;
2. pre-dispatch failure has no COMMIT dispatch/return and no false ack;
3. requested-visible lost acknowledgement, prior-visible rejected COMMIT,
   different-head, and ambiguous outcomes each retain exact dispatch, return,
   reconciliation, provenance, and fresh independent connection behavior;
4. no post-COMMIT observation/formatting failure relabels committed visibility;
5. supported SQLite status values are `Observed`, unsupported high-water/
   dirty-current/VFS/sync/temp facts have explicit `Unavailable` reasons;
6. status counter conversion/addition and pager-byte equations reject overflow;
7. snapshot absence and filesystem errors stay unavailable rather than zero;
8. cleanup and all success/error paths end at exact `Q=0`; and
9. every mutation row still has one transaction and one COMMIT.

The smallest executable gates before timing are preregistered as:

```text
cargo test --offline -p layerfs-engine --bin phase4_create_edit_benchmark \
  f1_commit_observations_separate_dispatch_return_and_reconciliation -- --exact --nocapture
cargo test --offline -p layerfs-engine --bin phase4_create_edit_benchmark \
  actual_commit_error_uses_fresh_reconciliation -- --exact --nocapture
cargo test --offline -p layerfs-engine --bin phase4_create_edit_benchmark \
  real_commit_dispatch_boundaries_cover_requested_different_and_ambiguous -- --exact --nocapture
cargo test --offline -p layerfs-engine --bin phase4_create_edit_benchmark \
  row_json_reconciles_q_sql_and_changed_work_fields -- --exact --nocapture
cargo test --workspace --offline --all-targets
cargo clippy --workspace --offline --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check HEAD
git status --short --branch
```

No release candidate is built until all gates pass and the complete tracked/
untracked diff is inspected.

## Release overhead campaign preregistration

### Frozen inputs and executables

- Fixture: exact retained `S1-100.source`, 104,857,600 bytes, SHA-256
  `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4`.
- Fixture manifest SHA-256:
  `8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca`.
- Candidate/profile/scenario: `K64-F64`, `full`.
- A/control: frozen once-built F0 executable `7c3959…af1a2`.
- B/candidate: built exactly once after correctness from the final F1 source
  into a separate target directory, then copied read-only into the artifact
  root and hashed.

Each warmup/measured pair is prepared once from the control executable as a
fresh empty candidate store plus expectations. The database, 32-byte authority
sidecar, and expectations are copied byte-for-byte with `/bin/dd` to each arm.
All three hashes and apparent/allocated starting bytes must match within each
pair before either arm runs. Preparation, copying, hashing, and preflight are
outside timers.

### Run order and samples

Run one uncounted adjacent warmup pair, then five adjacent measured pairs:

```text
warmup: AB
pair 1: AB
pair 2: BA
pair 3: AB
pair 4: BA
pair 5: AB
```

Each arm runs as one isolated child under `/usr/bin/time -l`. Raw stdout JSON
is augmented only after child exit with comparison, arm, pair/order, separate
user/system CPU, RSS, peak footprint, instructions, cycles, and block-operation
counters. No optional extension, row deletion, selective rerun, or throughput-
improvement claim is authorized. An infrastructure failure before a timed row
is retained and repaired once; any started row is never replaced silently.

### Equality gates

Every control/candidate row must agree exactly on:

- source and ordered CDC identities/count;
- root, transition, and closure identities;
- reconstructed bytes and every range result;
- objects/chunks/references/pages/branches;
- canonical new writes, mapping rewrites, SQL executes/changed rows, and BLOB
  writes;
- one writer transaction, one COMMIT dispatch, and committed publication;
- journal mode DELETE, synchronous FULL, temp_store FILE, and mmap_size 0;
- logical/apparent/allocated endpoint bytes; and
- terminal Q zero. Candidate Q high-water must not exceed control by more than
  5% and must remain within the frozen bound.

### Overhead equations and gate

For each measured pair `i`:

```text
paired_overhead_i_percent
  = 100 * (candidate_durable_capture_ns_i - control_durable_capture_ns_i)
        / control_durable_capture_ns_i

arm_median_overhead_percent
  = 100 * (median(candidate_durable_capture_ns)
           - median(control_durable_capture_ns))
        / median(control_durable_capture_ns)
```

F1 observability overhead passes only when:

1. arm-median durable-capture overhead is at most 5%;
2. paired-median durable-capture overhead is at most 5%;
3. at least four of five pairs have overhead at most 5%;
4. candidate median CPU, RSS, peak footprint, exact Q, and allocated-store
   delta do not regress by more than 5%; and
5. all equality, timer, status-label, transaction/COMMIT, publication, and
   custody gates pass.

Negative overhead is reported as noise/favorable observation only and is
never called an optimization. No result from this campaign replaces the M3
full-create performance baseline.

## Smallest permanent M4.5 regression proof

Because candidate executable bytes change, after the F1 full-create campaign
run exactly one uncounted warmup C0/C1 pair and one measured adjacent C0/C1
pair on the frozen v4 exact-XOR same-middle fixture, using one byte-identical
physical base per pair. This is a release regression proof, not a new M4.5
performance campaign. It must reproduce:

- identical source/CDC/root/transition/closure and operation oracle;
- C0 complete closure versus C1 changed spine only;
- C1 123 covered equal edges and eight new/different edges;
- eleven created objects, 110,745 new canonical bytes, and 7,382 rewritten
  mapping bytes in both arms;
- one transaction, one COMMIT dispatch/return, committed publication;
- exact Q 2,222,803 bytes and terminal zero; and
- no C1 full-closure replay.

Any mismatch is a hard F1 FAIL / REVISE regardless of overhead.

## Terminal decision rule

F1 is **PASS / retain** only if all focused/full/static tests, release custody,
five-pair overhead gates, observability labels/equations, smallest M4.5
regression proof, artifact-manifest verification, and final read-only audit
pass. Any identity, closure, durability, exact-Q, one-COMMIT, custody, timer,
or committed-publication relabeling failure is **FAIL / REVISE**. Unsupported
physical observations with precise API/permission reasons are acceptable;
invented zeros or logical/Q/RSS/allocation substitutes are hard failures.

F2 remains ineligible until a separate user-authorized task after F1 review.

## F1-v1 preserved disposition

- F1 disposition: **FAIL / REVISE**.
- Implementation action: **revise**; preserve the uncommitted candidate and
  all evidence, but do not make it the next control.
- F2 eligibility: **no**.
- Profile selection/promotion, production integration, F2 work, and Phase 4
  completion: not started/not claimed.

The instrumentation produced useful, internally consistent COMMIT and SQLite
pager evidence and passed semantic regression. It did not pass every
preregistered overhead/storage gate. No threshold was relaxed and no measured
row was rerun or discarded.

### Minimal implementation

Only
`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs` changed in the
measured candidate. There is no Cargo, dependency, shared-core, schema, format,
receipt, authority-sidecar, VFS, worker, pool, cache, retry, write-shape, or
durability-setting change.

The candidate:

1. reads `sqlite3_db_status` on the existing measured connection;
2. records filesystem metadata synchronously immediately before COMMIT
   dispatch and immediately after the SQLite call returns;
3. records COMMIT dispatch, SQLite return, and fresh reconciliation as distinct
   counters/timers; and
4. emits exact values or explicit Unavailable reasons in the existing row.

Measured custody:

| Item | SHA-256 |
|---|---|
| Measured source diff | `faf1934413813b6f29d1b58b9e709f6387488856d8460c1159256c17b81450d4` |
| Benchmark source | `aeb19ba3ff4c7a01326bd55de67cdfee88048c33961b9022706be69b4a5f55ed` |
| F0 control executable | `7c395935457f99acfd3b02e08cdba976b971c61a407aeb65c243f2f26ddaf1a2` |
| F1 candidate executable | `1ac9754c8c9a72ad08aa872e29d1c78a814f3d4fa29db9581dd833c09e60f5a3` |

The control and candidate were each built once in separate target directories.
The rebuilt control is byte-identical to the frozen v4 executable.

### Correctness and static gates

Focused tests passed for:

- acknowledged dispatch/return with no reconciliation;
- pre-dispatch failure with zero COMMIT dispatches/returns;
- actual rejected COMMIT with fresh prior-visible reconciliation;
- requested-visible lost acknowledgement, different complete head, and
  genuinely unavailable reconciliation;
- committed-publication custody after later failure;
- timer/status JSON equations and unsupported labels;
- status arithmetic overflow; and
- terminal Q cleanup.

The first preregistered exact filter omitted the Rust module prefix and ran
zero tests. It is excluded from evidence and retained as a command defect. The
corrected exact filter
`tests::f1_commit_observations_separate_dispatch_return_and_reconciliation`
ran one test and passed.

Final pre-build gates:

```text
cargo test --workspace --offline --all-targets
  99 passed; 0 failed
    layerfs-core: 44
    layerfs-engine library: 4
    private benchmark: 34
    phase4_engine_parity: 12
    layerfs-eval: 5

cargo clippy --workspace --offline --all-targets -- -D warnings
  PASS

cargo fmt --all -- --check
  PASS

git diff --check HEAD
  PASS

debug self-test
  PASS; root=f1cfdd7fdd658506caf39ace169dd83f1a95fbb25aafcbb7f9b72864f9d2e42a;
  objects=20; auth_bytes=1,054,925
```

### Full-create overhead evidence

The exact retained 100-MiB K64/F64 `full` fixture received one uncounted AB
warmup and five adjacent measured AB/BA pairs. All 12 rows passed their
semantic and timer self-gates. Every pair began from a once-prepared base whose
database, authority, and expectations were physically copied and hash-equal to
both arms.

Durable capture:

| Result | F0 control | F1 candidate | Candidate overhead |
|---|---:|---:|---:|
| Median | `936.497375 ms` | `927.187541 ms` | `-0.994112%` |
| Min | `927.991666 ms` | `924.572375 ms` | — |
| Max | `946.649958 ms` | `929.067458 ms` | — |
| Spread | `18.658292 ms` | `4.495083 ms` | — |

Paired overhead percentages were:

```text
-2.314420%; -1.516128%; +0.115927%; -0.418419%; -0.975234%
```

Paired median was `-0.975234%`; all 5/5 pairs were at or below the 5%
overhead ceiling. Negative movement is treated as favorable/noisy overhead
evidence only, never as an optimization or a replacement full-create
baseline.

Protected resource medians:

| Resource | F0 control | F1 candidate | Change | Gate |
|---|---:|---:|---:|---|
| User + system CPU | `1.580 s` | `1.580 s` | `0.000%` | PASS |
| RSS | `93,454,336` | `93,503,488` bytes | `+0.052595%` | PASS |
| Peak footprint | `92,291,528` | `92,324,296` bytes | `+0.035505%` | PASS |
| Allocated-store delta | `117,944,320` | `117,755,904` bytes | `-0.159750%` | PASS as protected median |
| Exact Q | `35,603` | `38,246` bytes | **`+7.423532%`** | **FAIL** |

The Q increase is the larger exact F1 report output. Both arms returned to
`q_current=0`; the candidate remained far below the 1-GiB safety bound, but it
failed the preregistered 5% instrumentation-overhead gate.

Logical and apparent main-database endpoints were exactly `109,268,992` bytes
in every row. Post-row allocated main-database bytes differed within all six
warmup/measured pairs, with mixed direction and sub-1% magnitude. Because the
preregistration required byte-identical allocated endpoints as an equality
gate, those six differences are also a terminal F1 failure even though the
candidate allocated-delta median was favorable. The physical differences are
not hidden, averaged into logical bytes, or retroactively reclassified.

### Observability baseline

The five measured candidate rows were deterministic for every SQLite pager
counter:

| Observation | Result | Classification |
|---|---:|---|
| SQLite COMMIT phase median | `136.924291 ms` | Observed |
| COMMIT dispatch-to-return median | `136.649708 ms` | Observed; acknowledged call interval |
| Pre/post-dispatch publication work median | `0.258084 ms` | Derived: publish call minus dispatch-to-return |
| Caller wrapper median | `0.016499 ms` | Derived: outer COMMIT phase minus publish call |
| Reconciliation | `0` calls / `0 ns` | Observed supported zero for ordinary acknowledged rows |
| Page-cache used before durable work | `14,336` bytes | Observed approximate current bytes |
| Page-cache used before COMMIT dispatch | `87,049,984` bytes | Observed approximate current bytes |
| Page-cache used after COMMIT return | `8,753,152` bytes | Observed approximate current bytes |
| Discrete page-cache snapshot maximum | `87,049,984` bytes | Derived; not true continuous high-water |
| Page-cache hits / misses | `71,129 / 6,698` | Observed |
| Dirty main-DB pages written | `26,676` | Observed `SQLITE_DBSTATUS_CACHE_WRITE` |
| Pager main-DB write bytes | `109,264,896` | Derived: `26,676 * 4,096`; not physical media I/O |
| Mid-transaction dirty spills | `6,676` pages | Observed `SQLITE_DBSTATUS_CACHE_SPILL` |
| Main DB at COMMIT dispatch | `27,435,008` apparent / `27,426,816` allocated bytes | Observed filesystem snapshot |
| Rollback journal at COMMIT dispatch | `17,928` apparent / `20,480` allocated bytes | Observed filesystem snapshot |
| Main DB after COMMIT return | `109,268,992` apparent / variable allocated bytes | Observed filesystem snapshot |
| Rollback journal after COMMIT return | `0` apparent / `0` allocated bytes | Observed endpoint state after DELETE |
| User CPU median | `1.340 s` | Observed externally |
| System CPU median | `0.240 s` | Observed externally |
| Block input/output operations | `0 / 0` in every candidate row | Observed operation counts; not bytes |

The exact COMMIT equation passed every candidate row:

```text
sqlite_commit_durability
  = commit_dispatch_to_return
  + commit_pre_and_post_dispatch
  + commit_caller_wrapper
```

`commit_reconciliation_wall_ns` is nested within post-return publication work
when used and is never double-counted.

### Explicitly unavailable observations

| Requested observation | Final classification and reason |
|---|---|
| SQLite page-cache true high-water | Unavailable: `SQLITE_DBSTATUS_CACHE_USED` defines high-water as always zero |
| Dirty pages currently resident | Unavailable: SQLite exposes writes/spills, not the current dirty set |
| Main DB/journal read/write calls and bytes | Unavailable: requires VFS xRead/xWrite or privileged syscall trace |
| Sync calls and sync wall | Unavailable: VFS is excluded; `fs_usage` required root and `dtruss` required additional DTrace privilege |
| True journal peak allocation | Unavailable: DELETE journal can grow/disappear between synchronous snapshots; `20,480` is only a sampled lower bound |
| Temp-file peak allocation | Unavailable: no filename/peak API is exposed under `temp_store=FILE` |
| Byte-level host physical I/O | Unavailable: block-operation counts, logical bytes, allocation, Q, RSS, and wall time are not substitutes |
| Native SQLite prepares | Unavailable: rusqlite statement-cache acquisition is not a native-prepare counter |
| W / D | Unavailable: the private benchmark still does not implement the governing cumulative definitions |

### Permanent M4.5 control regression

The F1 executable ran one warmup C0/C1 pair and one measured adjacent BA pair
against physical copies of the frozen v4 exact-XOR base.

| Metric | C0 | C1 |
|---|---:|---:|
| Durable edit | `429.935542 ms` | `9.184333 ms` |
| Pre-COMMIT qualification | `420.369875 ms` | `0.287541 ms` |
| Statement-cache acquisitions | `16,334` | `10,976` |
| SQL queries | `16,418` | `11,060` |
| Covered equal / new-different edges | `0 / 0` | `123 / 8` |

All four rows preserved the exact 5,284-reference CDC sequence, root,
transition, closure, eleven objects, 110,745 new canonical bytes, 7,382
mapping bytes, 18 executes, 12 changed rows, 26 BLOB writes, one transaction,
one COMMIT dispatch/return, committed publication, Q `2,222,803`, and terminal
zero. C1 did not restore complete pre-COMMIT replay. This regression is PASS
and is not a replacement M4.5 performance campaign.

### Artifacts

Versioned artifact root:

```text
target/wp4m-f1-commit-io-k64-20260819-v1/
```

Key hashes:

| Artifact | SHA-256 |
|---|---|
| F1 overhead raw JSONL | `79239cb80c99493a434ccfde1baab81e7a06ce40397f56e6297989068371915b` |
| F1 overhead preflight | `978fe207d5a1826423af44b0f1f17219a94ef67fdd0ddfed0188d7c4fc425403` |
| F1 overhead summary | `9b02fd5c439c28113f0abfcf21f5d1e0cffa18a5377b4745fb35a1002e801c1c` |
| F1 overhead commands | `039462d1dd5a99bc8c58ea0aa74e08db594f468f782e2fdd74a8b3e0734d2ee9` |
| F1 external observations | `960ea8dfe29a3ae9abeb4bc43730fa80d1278121d4cae83cb0387819e48db96f` |
| M4.5 regression raw JSONL | `117c3867cd16fa40b18a7789111cf4c47044a7bab1c428010c291d1d150af7bb` |
| M4.5 regression preflight | `6dea0ca1024effdf7f8099a9ef9c3ed15541eb7ea0a7dc8b3b4996d31380c084` |
| M4.5 regression summary | `fed014350e6f897ca2b49eb124ad447938b98400eae295295c7fe60f4a0afd0b` |

Two no-row campaign failures are retained as `failed-preflight-1.txt` and
`failed-preflight-2.txt`. Both occurred before any campaign file, base, or
timed row existed; they corrected zsh-reserved local names and did not replace
measurement evidence.

### Revision boundary and non-claims

The next F1 revision, if separately authorized, should compact per-row
classification text or move invariant source/reason dictionaries into one
hashed campaign manifest while retaining exact Q charging. That is the
smallest path to test the Q gate. Any change from exact allocated-byte equality
to a protected paired allocation gate must be amended prospectively before a
new campaign; this task does not make that policy change after observing data.

F1 does not establish sync/fsync or byte-level physical-I/O causality, does not
show that COMMIT is an irreducible durability floor, does not optimize full
create, and does not authorize batching or a transaction-local construction
proof. **Stop before F2.**

## F1-v2 prospective repair preregistration — before source edits or measurements

### Version and custody

- Date: 2026-08-19.
- Version: `wp4m-f1-commit-io-k64-20260819-v2`.
- F1-v1 remains **FAIL / REVISE**. Nothing in this amendment relabels,
  overwrites, regenerates, or deletes v1 source or evidence.
- Frozen branch / HEAD / tree:
  `codex/empty-worktree` /
  `a6aa42e53effc29246b0e1838e266f518d51dc18` /
  `f4c186790fe9a70a9768a7240987fef7967cd15f`.
- Frozen pre-v2 dirty tracked diff SHA-256:
  `1e7daf5bf63f426e408911b2820637e1d7566bf3ab6bc50b03f516102dff35d5`.
- Frozen pre-v2 status SHA-256:
  `05f2cf8abc70443f80eee29a565a7da9df5e796cdce9701f5ab87b959784f50a`.
- Frozen F1-v1 benchmark source/report/ledger SHA-256:
  `aeb19ba3ff4c7a01326bd55de67cdfee88048c33961b9022706be69b4a5f55ed` /
  `49fa35eab06c3d622f4bba240cde6e00ae906670cde20890223310a6783bea48` /
  `5209e259d896649540389e0a31ebe26ed58673a36c6e8de4912d6404fa188ece`.
- Frozen F1-v1 artifact manifest / complete hashes / terminal hashes / audit:
  `16173cb9b5d4d5b2b1638eac5c31f7c77df2e4ec45afe83592a127d3ecaa3e15` /
  `26f5e67b22c8b1b36418596a0cc407c61937c9dbf32863abc9bd1039d4ea7fa7` /
  `1597196448c9a08efcedf63b1990241635758cbf02cfd02d1b1e2541426703ba` /
  `75547c1207d193b5f42f8d6b9b99d75f3d190007987f31e2b561b27cb7db21c5`.
- F0 remains sealed by manifest
  `0c5711abfdad902c0db986590d09fff64ea46baf907ecb4e50dbb71ff67200e0`.

The new versioned root must be created only after focused correctness passes.
F1-v1 `target/wp4m-f1-commit-io-k64-20260819-v1` remains byte-for-byte
immutable throughout this repair.

### One variable and minimal repair

This remains diagnostic F1 observability. The one changed variable is the
representation and accounting of F1 observations:

1. replace repeated owned classification/reason prose in every row with short,
   stable codes;
2. retain the complete code-to-meaning dictionary once in this report and one
   hashed v2 schema artifact;
3. separately count measurement-only SQLite PRAGMA queries/rows and direct
   `sqlite3_db_status` reset/read/error calls; and
4. tighten the existing single private `sqlite3_db_status` wrapper and direct
   tests without introducing a framework or dependency.

There is no CDC/CAS/COW/root/delta, codec, schema, receipt, metadata,
write-shape, transaction, COMMIT, durability, worker, VFS, cache, retry, pool,
or async change. Before/after complexity remains:

```text
full create             = Theta(source bytes + references)
same-count mutation     = O(Xb + Xc + K + F*H)
C1 qualification        = O(K + F*H + A_delta + V_delta + H^2)
resident semantic state = existing bounded windows/frontier
writer/COMMIT count     = 1 / 1
```

### Stable row classification codes

The row declares `measurement_status_schema=f1-v2-status-codes-v1`. These are
the only permitted codes and meanings:

| Code | Classification | Complete meaning |
|---|---|---|
| `O` | Observed | Directly read from the named in-process API/counter or explicit filesystem snapshot |
| `O_EXT` | Observed | Directly read by the isolated parent from `/usr/bin/time -l` after child exit |
| `D` | Derived | Exact checked equation over named Observed inputs; not an independent observation |
| `NA` | NotApplicable | The operation/lane has no such semantic or durability event |
| `U_WD` | Unavailable | Governing cumulative W/D definitions are not implemented; narrower counters are not substitutes |
| `U_HEAP` | Unavailable | Other heap-copy bytes are not completely instrumented |
| `U_STATUS_API` | Unavailable | `sqlite3_db_status` returned non-`SQLITE_OK` or a negative/out-of-range current value |
| `U_CACHE_HWM` | Unavailable | `SQLITE_DBSTATUS_CACHE_USED` defines its high-water output as always zero |
| `U_DIRTY_CUR` | Unavailable | SQLite exposes dirty writes/spills but not the current dirty-page set |
| `U_VFS_IO` | Unavailable | Main/journal xRead/xWrite call and byte totals require a prohibited VFS or unavailable privileged trace |
| `U_VFS_SYNC` | Unavailable | Sync call/wall attribution requires a prohibited VFS or unavailable privileged trace |
| `U_JRN_PEAK` | Unavailable | DELETE journal can grow/disappear between synchronous snapshots; sampled maximum is only a lower bound |
| `U_TMP_PEAK` | Unavailable | `temp_store=FILE` exposes no complete filename/peak-allocation API |
| `U_PHYS_BYTES` | Unavailable | Byte-level media I/O is not derivable from logical/apparent/allocated bytes, Q, RSS, block operations, or wall time |
| `U_PLAN` | Unavailable | Query plans are not collected in F1 |
| `U_NATIVE_PREP` | Unavailable | Statement-cache acquisition is not the native SQLite prepare count |
| `MIXED_IO` | Mixed | Supported pager/filesystem observations are Observed/Derived; prohibited or unsupported VFS/physical facts retain their Unavailable codes |

The code dictionary is invariant campaign metadata, not hidden row state. The
row keeps every classification field and equation, but owns only the stable
code. One focused test must compare every emitted code with this exact
dictionary and reject unknown, missing, or meaning-drifted entries. All row,
phase JSON, range JSON, and report output capacities remain charged exactly;
no required byte is moved outside Q.

The exact-Q equation is likewise represented once by stable code `Q1`:

```text
Q1 = pre_admitted_checked_sum:
     canonical + decoded_nodes + file_refs + tree_nodes + dfs + cdc +
     sql + expectations + ranges + receipts + report
```

`Q1` changes only the owned row representation; the checked equation and all
capacity charging/decharging behavior are unchanged.

### Measurement-only SQLite call inventory and equations

F1-v2 must report workload and observation calls in distinct namespaces.

Workload counters remain the existing semantic counters:

```text
workload_sql_calls
  = workload_sql_query_calls + workload_sql_execute_calls
```

For the retained full-create row, control and candidate workload counters must
remain exactly equal. The frozen v1 values are:

```text
statement-cache acquisitions = 16,236
workload SQL queries         = 10,953
workload SQL executes        = 5,379
workload rows returned       = 16,153
workload rows changed        = 5,373
```

Measurement-only SQL calls are:

- `physical_snapshot` before the durable operation: `PRAGMA page_count` and
  `PRAGMA page_size`;
- `start_sqlite_observations`: one `PRAGMA page_size`; and
- `physical_snapshot` after the lifecycle: `PRAGMA page_count` and
  `PRAGMA page_size`.

The normal full-create candidate equation is therefore:

```text
measurement_sql_queries = 2 + 1 + 2 = 5
measurement_sql_rows    = 2 + 1 + 2 = 5
```

Direct connection-status calls are not SQL and remain separate:

```text
measurement_status_reset_calls = 4
measurement_status_read_calls  = 5 before + 5 pre-dispatch + 5 post-return
measurement_status_calls       = 4 + 15 = 19
measurement_status_errors      = 0 on supported retained rows
```

Every count is emitted with `O`. A supported zero is numeric only after the
call succeeds. Workload SQL totals must remain control/candidate equal and may
not absorb any measurement-only PRAGMA or status call.

To avoid recreating the v1 exact-Q failure with repeated field prose, the row
uses this compact, schema-fixed accounting block:

```text
instrumentation.c      = O
instrumentation.sql    = [before_queries, before_rows,
                          start_queries, start_rows,
                          after_queries, after_rows,
                          total_queries, total_rows]
instrumentation.status = [reset_calls, before_reads,
                          predispatch_reads, postreturn_reads,
                          total_calls, errors]
```

The existing top-level `statement_cache_acquisitions`, `sql_query_calls`,
`sql_execute_calls`, `sql_rows_returned`, and `sql_rows_changed` are explicitly
the workload namespace. They are not duplicated under `instrumentation`.

The private status wrapper must:

- use the live caller-thread-owned `Connection` handle only;
- keep the output pointers valid for the complete FFI call;
- check the SQLite return code;
- reject negative or out-of-range current values through checked conversion;
- expose reset/read/error observations without turning a post-COMMIT
  observation failure into an unqualified operation failure; and
- remain the only direct `sqlite3_db_status` call site.

### Prospective F1-v2 storage amendment

This amendment replaces only v1's invalid exact-APFS-allocation equality
rule. It does not change any already observed v1 classification.

Hard exact storage gates:

- logical main database bytes are control/candidate equal;
- deterministic apparent main DB, journal, and authority-sidecar bytes are
  control/candidate equal;
- post-row journal apparent and allocated bytes are exactly zero;
- no new serialized table, column, index, metadata, sidecar, or endpoint
  exists; and
- every pair begins from byte-identical database/authority/expectation copies.

Protected allocated-byte gates are evaluated separately for main DB, journal,
authority sidecar, and total allocated-store delta. For each nonzero control
metric:

```text
paired_allocated_overhead_i_percent
  = 100 * (candidate_i - control_i) / control_i
```

Each allocated endpoint passes only when:

1. paired-median overhead is at most 5%;
2. at least four of five measured pairs are at most 5%;
3. all five pair deltas and percentages are reported;
4. no unexplained positive candidate growth occurs in all five pairs; and
5. no residual journal or new serialized metadata exists.

When the control value is zero, the candidate must also be exactly zero; no
percentage is invented. This prospective rule reflects APFS allocation
granularity while retaining exact logical/apparent and no-residue gates.

### Protected overhead, Q, and decision rules

For durable wall, total CPU, RSS, peak footprint, and allocated-store delta:

```text
paired_overhead_i_percent = 100 * (candidate_i - control_i) / control_i
```

Each metric passes only when its arm-median overhead and paired-median overhead
are at most 5%, at least four of five measured pairs are at most 5%, and all
pair values are retained. Negative movement is overhead/noise evidence only,
never a speed-improvement claim.

Exact Q is not loosened. The first repair passes only when:

- arm-median Q overhead is at most 5%;
- paired-median Q overhead is at most 5%;
- at least four of five paired Q results are at most 5%;
- every charged component and `q_report_output_bytes` is reported; and
- every exit returns to exact `q_current=0`.

If compact codes still fail, F1-v2 stops **FAIL / REVISE**. No post-observation
amendment or second candidate is authorized in this task.

Hard gates remain exact source/CDC/object/root/transition/closure,
reconstruction/ranges, work/SQL/BLOB counters, timer equations, failure
provenance, one transaction, one COMMIT dispatch/return, committed publication,
fresh reconciliation, terminal Q zero, copied-base custody, and no
post-COMMIT relabeling.

### Focused and release validation contract

Before the release build, run exact focused tests for:

- stable-code dictionary fidelity and unknown-code rejection;
- full-row compact output with exact Q component/high-water/terminal equations;
- real-path Q overlap, cleanup, 1-GiB cap, and overflow;
- exact workload versus measurement-only SQL/status accounting;
- status reset, read, unsupported operation, SQLite error, and checked-range
  conversion;
- dispatch/return/reconciliation and timer equations;
- requested/prior/different/ambiguous outcomes;
- unsupported status codes and no invented zero;
- one COMMIT; and
- committed publication not relabeled by later observation/report failure.

Then run:

```text
cargo test --workspace --offline --all-targets
cargo clippy --workspace --offline --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check HEAD
git status --short --branch
```

After those gates, copy the exact F0 control
`7c395935457f99acfd3b02e08cdba976b971c61a407aeb65c243f2f26ddaf1a2`
into the new v2 root and build the F1-v2 release candidate exactly once from
the frozen validated source.

### Fresh F1-v2 evidence contract

Use only the exact retained 104,857,600-byte K64/F64 full-create fixture,
fixture SHA-256
`63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4`,
and manifest SHA-256
`8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca`.

Run one uncounted AB warmup pair followed by exactly five adjacent measured
pairs in order `AB / BA / AB / BA / AB`. Prepare each pair once, physically
copy byte-identical database/authority/expectations to both arms, and preserve
all raw JSONL, external observations, commands, source/diff/executable hashes,
environment, pair-base/arm hashes, timer/counter equations, Q components,
SQL/BLOB counters, W/D limitations, and logical/apparent/allocated endpoints.
No optional extension, row deletion, or selective rerun is authorized.

Generate a primary summary and a separately implemented independent
recomputation from raw JSONL/preflight only. They must agree on every input
row, identity/work gate, median, paired overhead, pair count, and disposition.

Because executable bytes change, run only the smallest permanent release
M4.5 proof: one uncounted C0/C1 warmup pair and one measured adjacent C0/C1
pair on physical copies of the frozen v4 exact-XOR base. It must reproduce the
permanent identities, `16,334/16,418` C0 versus `10,976/11,060` C1
acquisition/query counters, C1 `123/8` edge counters, eleven writes,
110,745/7,382 canonical bytes, one transaction/COMMIT, Q `2,222,803`, terminal
zero, and no C1 complete-closure replay.

### Close and stop rule

F1-v2 is **PASS / retain; F2 eligible** only if every prospective correctness,
custody, compact-Q, wall/CPU/RSS/peak/store, allocated-endpoint, timer,
measurement/workload-SQL separation, one-COMMIT, publication, unsupported-code,
M4.5 regression, manifest, and final read-only audit gate passes.

Any failure leaves F1 **FAIL / REVISE** and F2 ineligible. Even on PASS, stop
before F2, do not select/promote a profile, do not integrate production, do
not claim Phase 4 complete, and do not commit.

## F1-v2 measured result — measured gates PASS; protocol audit FAIL / REVISE

### Implementation and validation

The repair changes only F1 observation representation/accounting:

- repeated row classifications/reasons use the frozen status codes above;
- exact-Q equation `Q1` is defined once above rather than owned as repeated
  prose;
- workload SQL remains in the established top-level counters, while the
  candidate observation block reports
  `sql=[2,2,1,1,2,2,5,5]` and `status=[4,5,5,5,19,0]`; and
- the only `sqlite3_db_status` call remains one private checked wrapper on the
  live synchronous caller-thread connection.

No dependency, schema, format, serialized metadata, write-shape, identity,
transaction, COMMIT, durability, worker, VFS, cache, retry, pool, or async
change was made. The lifecycle and complexity contracts therefore require no
edit.

Validation passed:

- 35/35 benchmark-focused tests, including compact-code fidelity, exact row/Q
  accounting, real-path capacity cleanup/overflow, measurement/workload SQL
  separation, SQLite status reset/read/error/range handling, COMMIT dispatch
  versus return, requested/prior/different/ambiguous reconciliation, one
  COMMIT, and no post-COMMIT relabeling;
- 100/100 workspace all-target tests;
- workspace/all-target Clippy with `-D warnings`;
- rustfmt check; and
- tracked whitespace/diff and status checks.

Frozen release custody:

| Artifact | SHA-256 |
|---|---|
| sealed F0 control source | `0a078b25216fdc4da83722807dd8e921b523f99f074c86e5480a38e2a9ea2061` |
| sealed F0 control executable | `7c395935457f99acfd3b02e08cdba976b971c61a407aeb65c243f2f26ddaf1a2` |
| F1-v2 candidate source | `9a4fff15668726e2dc2fdd84258e368dbcc992ba4bf39658f3f97cc996655a64` |
| F1-v2 source-only diff | `441baa71b8c75740a1ed134b759aeeb6c9e22c3f5f06d1ea02984624a6775a2e` |
| F1-v2 one-time release executable | `732171041ea25684399d308af1d4682bb9fc58b2a3c79e16080b39d0cb32b805` |
| frozen prospective preregistration | `b25caf05198d882d84b4fbae295969f8d4266a91cd712bda6e10b2c93246b645` |

### Fresh overhead campaign

The immutable v2 root is
`target/wp4m-f1-commit-io-k64-20260819-v2`. It used the exact frozen
104,857,600-byte K64/F64 fixture and executed one uncounted AB warmup pair,
then five adjacent measured pairs in order BA/AB/BA/AB/BA (the complete six
pair sequence, including warmup, is AB/BA/AB/BA/AB/BA). Every arm began from
the pair's byte-identical database, authority, and expectations copies.

All identity, closure, range, workload, transaction, COMMIT, timer, terminal-Q,
and logical/apparent storage gates were exact. In all six candidate rows:

```text
workload statement-cache acquisitions = 16,236
workload SQL queries / executes        = 10,953 / 5,379
workload rows returned / changed       = 16,153 / 5,373
measurement SQL                         = [2,2,1,1,2,2,5,5]
measurement status                      = [4,5,5,5,19,0]
transactions / COMMIT dispatch / return = 1 / 1 / 1
reconciliation calls                    = 0
terminal Q                              = 0
```

Protected measured overhead:

| Metric | F0 median | F1-v2 median | Arm-median overhead | Paired median | Pairs <=5% | Result |
|---|---:|---:|---:|---:|---:|---|
| durable wall | 926.418916 ms | 922.015333 ms | -0.475334% | +0.040155% | 5/5 | PASS |
| total CPU | 1.590 s | 1.620 s | +1.886792% | +2.515723% | 5/5 | PASS |
| maximum RSS | 93,650,944 | 93,601,792 | -0.052484% | -0.035032% | 5/5 | PASS |
| peak footprint | 92,488,136 | 92,389,808 | -0.106314% | -0.070947% | 5/5 | PASS |
| allocated-store delta | 117,878,784 | 117,993,472 | +0.097293% | +0.205203% | 5/5 | PASS |
| exact Q | 35,603 | 37,302 | +4.772070% | +4.772070% | 5/5 | PASS |

These are observability-overhead results, not throughput-improvement claims.
The durable paired values were `+0.354565, +0.242637, +0.040155, -0.596340,
-1.571914%`; the exact-Q paired values were `+4.766318, +4.775013,
+4.772070, +4.775013, +4.769261%`.

### Exact-Q repair breakdown

The full-create median exact-Q decomposition is:

| Row | Non-report live capacity, derived as Q - report | Charged report output | Exact Q high-water |
|---|---:|---:|---:|
| sealed F0 control | 14,486 | 21,117 | 35,603 |
| preserved F1-v1 | 14,486 | 23,760 | 38,246 (`+7.423532%`, FAIL) |
| fresh F1-v2 | 14,486 | 22,816 | 37,302 (`+4.772070%`, PASS) |

The repair removed 944 simultaneously live owned report bytes from F1-v1
(`-3.973064%` report output, `-2.468232%` total Q) without moving bytes outside
Q. All separately emitted CDC overlap components remained exact zero for this
full-create path, and every exit returned to `q_current=0`. The prospective
5% ceiling is 37,383.15 bytes, leaving 81 bytes of measured headroom; this
narrow margin is a limitation, not a reason to weaken the gate.

### Storage and observation matrix

Logical/apparent main DB bytes were exactly 109,268,992 in every arm;
journal apparent/allocated bytes were exactly zero; and the authority sidecar
was exactly 32 apparent / 4,096 allocated bytes. Read-only reopen audit found
one identical schema hash
`636f4c4f5a5940eb64c7c865ee94286a51f5265d83ae888556ed7e27f5084c62`
across all 12 arms, no schema-changing source diff, no residual journal/WAL/SHM,
and no unexpected serialized endpoint.

| Allocated metric | Control median | Candidate median | Paired median | Pair percentages | Result |
|---|---:|---:|---:|---|---|
| main DB | 117,899,264 | 118,013,952 | +0.205167% | +0.246665, +0.205167, +0.243546, -0.367749, -0.381375% | PASS |
| journal | 0 | 0 | NotApplicable (exact zero) | zero/zero in 5/5 | PASS |
| authority sidecar | 4,096 | 4,096 | 0% | 0% in 5/5 | PASS |
| total store | 117,903,360 | 118,018,048 | +0.205160% | +0.246656, +0.205160, +0.243538, -0.367736, -0.381362% | PASS |
| store delta | 117,878,784 | 117,993,472 | +0.205203% | +0.246708, +0.205203, +0.243588, -0.367813, -0.381441% | PASS |

The mixed signs rule out unexplained positive candidate growth in all five
pairs. APFS allocation remains an observed endpoint property, not a physical
I/O-byte substitute.

| Observation | Classification | Result / source |
|---|---|---|
| COMMIT dispatch, return/acknowledgement, reconciliation | Observed | caller-thread timers/counters; candidate median COMMIT 129.248916 ms, dispatch-to-return 128.985291 ms, reconciliation 0/5 |
| SQLite page-cache current/snapshot maximum | Observed | `sqlite3_db_status`; median maximum 87,049,984 bytes |
| cache hits / misses / dirty writes / spills | Observed | 71,129 / 6,698 / 26,676 / 6,676 |
| pager write bytes | Derived | 26,676 pages x 4,096 bytes = 109,264,896; not physical I/O |
| logical/apparent/allocated endpoints | Observed | SQLite PRAGMAs plus explicit filesystem snapshots |
| sampled journal allocation maximum | Derived lower bound | 20,480 bytes |
| user/system CPU, RSS, footprint, block operations | Observed externally | `/usr/bin/time -l`; measured candidate block operations were zero but physical bytes remain unavailable |
| true SQLite cache high-water/current dirty set | Unavailable | API contract / no current dirty-set API (`U_CACHE_HWM`, `U_DIRTY_CUR`) |
| main/journal read/write calls and bytes | Unavailable | prohibited VFS and unavailable privileged trace (`U_VFS_IO`) |
| sync calls and sync wall | Unavailable | prohibited VFS and unavailable privileged trace (`U_VFS_SYNC`) |
| true journal peak / temp-file peak | Unavailable | DELETE-journal sampling gap / no complete temp filename-and-peak API (`U_JRN_PEAK`, `U_TMP_PEAK`) |
| byte-level host physical I/O | Unavailable | not inferred from allocation, Q, RSS, wall, or zero block-operation observations (`U_PHYS_BYTES`) |

### Permanent M4.5 regression and evidence custody

The smallest required release regression passed one warmup plus one measured
C0/C1 pair. The measured durable edit was 439.551291 ms C0 versus 8.668667 ms
C1 (`-98.027837%` direction only). It reproduced Q 2,222,803 and terminal
zero, the permanent identities/work, one transaction/COMMIT, C0
16,334/16,418 versus C1 10,976/11,060 acquisition/query counters, and the C1
123/8 equal/different edge counters.

Primary and separately implemented recomputation agree with no violation.
Principal evidence hashes are:

| Evidence | SHA-256 |
|---|---|
| overhead raw JSONL | `cceb5077bfacbcd883f54db8596108a2e5746e7002696b73a87c879b381e72b2` |
| overhead primary summary | `fb82aa5251a4211d661f791f87958297abe8e15fba80ec71d2fb03675d827a16` |
| overhead preflight | `a635a2a45e9f31d8535fe5664c6ccd0da0eed00ee99a6967d3a1f5a04287c1e7` |
| overhead commands | `6d7affe037269ab6d55385032804a02d9e5c9413f60d2cd2a0fccd8ba6f59fb9` |
| external resource observations | `55544f50e8e940daff07a6a54a6db55a7c50415f73f80439131dcbba914fc907` |
| storage/schema audit | `29ab6ba968ff194c11a747a95fe612abd124979ff1756941e218072be0e35fe6` |
| independent recomputation | `7f804f835181b0b8874eca787e32bba45d1aa2fca47abaa61df5e8e49bb5a709` |
| M4.5 raw JSONL | `4b0b98d6fcb8cc91d1ca4fe2e9e886f1a6b2fa95ae9ee662b60552d360f1e8b3` |
| M4.5 summary | `918d3b97a39c3389dc919f31c6a6fa31e3d01477ec01db9235374a79999cb976` |

The preserved `first-pass-*-error` files record analyzer/check mistakes only:
candidate-only runtime fields were initially compared to absent sealed F0 row
keys, the storage source search initially expected spaced PRAGMA spelling, and
the first final-audit pass expanded four abbreviated v1 hashes incorrectly.
The authoritative v1 complete list still independently verifies 159/159.
None of these corrections changed a measurement, threshold, gate, or raw row.

### Final disposition

**FAIL / REVISE; preserve F1-v2; F2 ineligible.** All numerical, correctness,
validation, custody, SQL-accounting, exact-Q, storage, one-COMMIT, and M4.5
results above pass, but the final protocol audit found one hard preregistration
failure: after the required AB warmup, the prospective contract fixed the five
measured pair order as `AB / BA / AB / BA / AB`; the retained raw campaign is
`BA / AB / BA / AB / BA`. Each pair is adjacent and the design is balanced to
the maximum possible degree for five pairs, but it is not the exact frozen
order. Passing measurements cannot cure a prospective execution mismatch.

No threshold or sequence is amended after observation, and no row, summary,
or v1 artifact is rewritten. The v2 evidence is retained as informative but
non-acceptance evidence. The authorized campaign is exhausted, so there is no
selective or whole-campaign rerun in this task. Do not retain the candidate as
an accepted F1 state, revert v1/v2 evidence, start F2, select/promote a profile,
integrate production, claim Phase 4 complete, or commit.

## F1-v3 prospective campaign authorization — before v3 artifacts or timing

### Authorization, custody, and one change

- Date: 2026-08-19.
- Version: `wp4m-f1-commit-io-k64-20260819-v3`.
- Authorization: one fresh campaign may repair only F1-v2's measured-pair
  orchestration error. F1-v1 and F1-v2 remain immutable historical
  **FAIL / REVISE** evidence and are not relabeled.
- Branch / HEAD / tree remain `codex/empty-worktree` /
  `a6aa42e53effc29246b0e1838e266f518d51dc18` /
  `f4c186790fe9a70a9768a7240987fef7967cd15f`.
- Frozen pre-v3 source / report / ledger SHA-256:
  `9a4fff15668726e2dc2fdd84258e368dbcc992ba4bf39658f3f97cc996655a64` /
  `f731dfa31aff48814589359f4baab9eae1fbd749abbb53cbbc0257da98f30307` /
  `147794a2247937751064c3b11f4b5cc6e6059eba46eebed4c69edec22fd51c18`.
- Frozen pre-v3 status-plus-tracked-diff SHA-256:
  `415e875fa2df8653e9d908c3d1c944bed99deaf1cc3d7a9f83398cdb29aecf75`.
- F0 control executable:
  `7c395935457f99acfd3b02e08cdba976b971c61a407aeb65c243f2f26ddaf1a2`.
- F1 candidate executable:
  `732171041ea25684399d308af1d4682bb9fc58b2a3c79e16080b39d0cb32b805`.
- Candidate source:
  `9a4fff15668726e2dc2fdd84258e368dbcc992ba4bf39658f3f97cc996655a64`.
- Preserved F1-v1 manifest / complete hashes:
  `16173cb9b5d4d5b2b1638eac5c31f7c77df2e4ec45afe83592a127d3ecaa3e15` /
  `26f5e67b22c8b1b36418596a0cc407c61937c9dbf32863abc9bd1039d4ea7fa7`.
- Preserved F1-v2 manifest / complete / terminal hashes:
  `5de33d9483ce28160ed5349f812aa8affd98d89bad739645b6fb3b478a50ec16` /
  `a6ef490db01b55ea299a692602d882457db6b6587e6b41ddcc27c6f8c428c111` /
  `22d1cd611f59447e2a29468a2debad381cd8b6c29bd9c38e973f6925d5353433`.
- F0 manifest remains
  `0c5711abfdad902c0db986590d09fff64ea46baf907ecb4e50dbb71ff67200e0`.

The sole changed variable is campaign orchestration. The benchmark source,
candidate row encoding, exact-Q accounting, binaries, fixture, schema,
format, metadata, algorithm, write shape, transaction/COMMIT count, and
durability configuration are frozen. No release rebuild, static/test rerun,
or M4.5 rerun is authorized unless a read-only custody check fails.

### Exact schedule and dry-run gate

The complete planned sequence is immutable:

```text
pair0 warmup  AB
pair1 measured AB
pair2 measured BA
pair3 measured AB
pair4 measured BA
pair5 measured AB
```

Equivalently, the six-pair vector must be exactly:

```text
[AB, AB, BA, AB, BA, AB]
```

The minimal runner condition is:

```text
pair == 0 or odd measured pair => AB
even nonzero measured pair     => BA
```

Before preparing a fixture, copying a base, invoking either executable, or
creating any timing/raw output, the runner must construct the six-pair vector,
compare it to `[AB,AB,BA,AB,BA,AB]`, print and preserve the dry-run result,
and exit nonzero on any difference. The schedule assertion is the only new
logic; no framework or benchmark change is permitted.

### Reused gates and fresh evidence

All F1-v2 prospective semantic and measurement gates above are reused without
change. In particular, v3 requires:

- exactly 2 warmup rows plus 10 measured rows, adjacent within each pair and
  in the exact six-pair vector above;
- exact fixture/manifest and copied database/authority/expectation custody;
- exact source/CDC/object/root/transition/closure/range/work/SQL/BLOB identity;
- one writer transaction, one COMMIT dispatch/return, exact timer equations,
  acknowledged publication, no reconciliation on acknowledged COMMIT, and
  terminal Q zero;
- workload SQL exact and separate instrumentation vectors
  `sql=[2,2,1,1,2,2,5,5]` and `status=[4,5,5,5,19,0]`;
- exact-Q arm and paired medians at most 5%, at least 4/5 pairs at most 5%,
  with every Q component/output byte retained;
- durable wall, CPU, RSS, peak footprint, and allocated-store delta under the
  unchanged arm-median/paired-median/4-of-5 rule;
- exact logical/apparent main DB, journal, and authority-sidecar endpoints;
- allocated main/journal/sidecar/store rules unchanged from v2, no unexplained
  positive growth in all five pairs, no residual journal/WAL/SHM, and no new
  serialized metadata or endpoint;
- a primary summary plus separately implemented recomputation agreeing on
  every input, pair, statistic, gate, and disposition; and
- a final read-only protocol/custody audit and complete versioned manifest.

Use only the frozen 104,857,600-byte K64/F64 fixture, SHA-256
`63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4`,
and fixture manifest SHA-256
`8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca`.
Each pair is prepared once and physically copied byte-identically to A and B.
No row deletion, selective rerun, second campaign, or post-observation
amendment is authorized.

The passing v2 validation evidence (35/35 focused, 100/100 workspace, Clippy,
fmt, diff) and passing release M4.5 regression are hash-referenced rather than
rerun because source and executable bytes are frozen. Any custody mismatch
invalidates this reuse and stops v3 before timing.

### V3 close and stop rule

F1-v3 is **PASS / retain; F2 eligible** only if every reused and fresh gate,
including the exact schedule, passes. Any failure retains F1 **FAIL / REVISE**
and F2 ineligible. In either case stop before F2, do not select/promote a
profile, do not integrate production, do not claim Phase 4 complete, and do
not commit.

## F1-v3 final result — PASS / retain; F2 eligible

### Exact campaign execution

The one authorized fresh campaign ran exactly once. Its dry-run assertion was
written before pair preparation or executable invocation:

```text
expected=AB,AB,BA,AB,BA,AB
actual=AB,AB,BA,AB,BA,AB
status=PASS
```

Raw JSONL and preflight independently reproduce the same complete sequence:

```text
pair0 warmup   A then B
pair1 measured A then B
pair2 measured B then A
pair3 measured A then B
pair4 measured B then A
pair5 measured A then B
```

There are exactly 12 rows: 2 warmup and 10 measured. Every pair is adjacent,
every row's `pair_sequence` and `warmup` classification is exact, and every
database/authority/expectation arm copy hashes identically to its pair base.
No row was deleted or rerun.

### Frozen implementation and reused proof

Campaign orchestration was the only change. Benchmark source remained
`9a4fff15668726e2dc2fdd84258e368dbcc992ba4bf39658f3f97cc996655a64`;
the sealed control and unchanged candidate executables remained
`7c395935457f99acfd3b02e08cdba976b971c61a407aeb65c243f2f26ddaf1a2`
and `732171041ea25684399d308af1d4682bb9fc58b2a3c79e16080b39d0cb32b805`.
No build, benchmark-source edit, static/test rerun, or M4.5 rerun occurred.

V3 hash-references the unchanged passing F1-v2 evidence:

- validation/build record
  `aacb2c18c00d0dc69875704dfebfe69874377b23face7f91b3aa746c6c54cd7a`
  (35/35 focused, 100/100 workspace, Clippy, fmt, diff);
- M4.5 raw
  `4b0b98d6fcb8cc91d1ca4fe2e9e886f1a6b2fa95ae9ee662b60552d360f1e8b3`;
  and
- M4.5 PASS summary
  `918d3b97a39c3389dc919f31c6a6fa31e3d01477ec01db9235374a79999cb976`
  (measured C0/C1 439.551291 / 8.668667 ms, Q 2,222,803, terminal
  zero, permanent work/identity/one-COMMIT counters).

F1-v1 remains preserved at 159/159 complete hashes. F1-v2 remains preserved
at 166/166 complete hashes with its historical sequence-only failure. The v2
terminal-hash artifact itself is unchanged; its external live-report entry is
historically expected to differ after this authorized v3 report append.

### Correctness, accounting, and exact Q

Primary and independent analyses report no violation. All six candidate rows
retain exact identities, closure, ranges, work, SQLite/BLOB counters, one
transaction/COMMIT dispatch/return, acknowledged publication, zero
reconciliation calls, exact timer equations, and terminal `q_current=0`.

```text
workload statement-cache acquisitions = 16,236
workload SQL queries / executes        = 10,953 / 5,379
workload rows returned / changed       = 16,153 / 5,373
measurement SQL                         = [2,2,1,1,2,2,5,5]
measurement status                      = [4,5,5,5,19,0]
transactions / COMMIT dispatch / return = 1 / 1 / 1
```

Exact-Q measured results are deterministic in all five pairs:

| Metric | Control | Candidate | Arm median | Paired median | Pairs <=5% | Result |
|---|---:|---:|---:|---:|---:|---|
| Q high-water | 35,603 | 37,302 | +4.772070% | +4.772070% | 5/5 | PASS |
| charged report output | 21,117 | 22,816 | exact charged component | exact | 5/5 | PASS |
| non-report live capacity (`Q - report`) | 14,486 | 14,486 | 0% | 0% | 5/5 | PASS |

The unchanged candidate remains 81 bytes below the prospective 5% Q ceiling;
all full-create CDC-overlap components are exact zero and no byte is moved
outside Q.

### Protected overhead

| Metric | Control median | Candidate median | Arm-median overhead | Paired median | Pair percentages | Result |
|---|---:|---:|---:|---:|---|---|
| durable wall | 929.257167 ms | 932.013041 ms | +0.296567% | -0.311203% | -0.311203, +0.778325, -0.949083, -0.510798, +0.123010% | PASS, 5/5 |
| total CPU | 1.610 s | 1.650 s | +2.484472% | +2.484472% | +2.484472, +3.125000, +1.242236, +1.242236, +3.125000% | PASS, 5/5 |
| maximum RSS | 93,782,016 | 93,732,864 | -0.052411% | +0.017479% | -0.192173, +0.052411, +0.262881, +0.017479, -0.139714% | PASS, 5/5 |
| peak footprint | 92,619,208 | 92,537,288 | -0.088448% | -0.017699% | -0.229965, +0.017690, +0.248516, -0.017699, -0.176834% | PASS, 5/5 |
| allocated-store delta | 117,899,264 | 117,813,248 | -0.072957% | -0.031177% | -0.260561, +0.480685, -0.509426, +0.188094, -0.031177% | PASS, 5/5 |

These quantify observability overhead only and are not a throughput-improvement
claim.

### Storage and observability

Logical/apparent main DB bytes are exactly 109,268,992 in every arm;
journal apparent/allocated endpoints are exactly zero; and the authority
sidecar is exactly 32 apparent / 4,096 allocated bytes. Read-only storage audit
again finds the single schema hash
`636f4c4f5a5940eb64c7c865ee94286a51f5265d83ae888556ed7e27f5084c62`,
no schema-changing source line, no residual journal/WAL/SHM, and no unexpected
serialized endpoint.

| Allocated endpoint | Control median | Candidate median | Paired median | Pair percentages | Result |
|---|---:|---:|---:|---|---|
| main DB | 117,919,744 | 117,833,728 | -0.031172% | -0.260516, +0.480602, -0.509338, +0.188062, -0.031172% | PASS |
| journal | 0 | 0 | NotApplicable, exact zero | zero/zero 5/5 | PASS |
| authority sidecar | 4,096 | 4,096 | 0% | 0% 5/5 | PASS |
| total store | 117,923,840 | 117,837,824 | -0.031171% | -0.260507, +0.480585, -0.509320, +0.188055, -0.031171% | PASS |
| store delta | 117,899,264 | 117,813,248 | -0.031177% | -0.260561, +0.480685, -0.509426, +0.188094, -0.031177% | PASS |

The v2 observability matrix and limitations remain exact because the executable
is unchanged: COMMIT dispatch/return/reconciliation, supported SQLite pager
counters, endpoint snapshots, and external CPU/RSS/block operations are
Observed; pager bytes and sampled journal maximum are Derived; true cache
high-water/current dirty set, VFS main/journal calls and bytes, sync calls/wall,
true journal/temp peak, and byte-level host physical I/O remain Unavailable
with the frozen reasons. Zero external block-operation observations are not
relabeled as zero physical I/O.

### Evidence hashes and disposition

| V3 evidence | SHA-256 |
|---|---|
| preregistration | `06a16acf53cce8a90287386ac7b77cb921458e59476fa247d73d815dab4ed11a` |
| runner | `11699461435ee614f310b852980d26c67de8c056fe4c9b7d47b6ae611cd5ba66` |
| dry-run schedule | `1c76b72fbc17336b57222e37d5ec83b75e90b6d039f204bab48561e0e0e797fb` |
| overhead raw JSONL | `dfa78b82fd2cdd27b76ce2708a3411579a09a4b1bd11bbf3e39030e7fc1afd44` |
| overhead preflight | `568d06a86d0d69ab71528fecb9b8e639dd3437101786349726bab89a6b068971` |
| overhead commands | `2fab21402fc65395918a6ce54a0b2c91328f93fdbe88501429cd6240af8c5d83` |
| external resource observations | `7aa44586927ed493d23688a1901c436197e265ceb828660aced22cb6b9bf891a` |
| primary summary | `f36cc5c565e61ff48513588127a3c04c4472ea3592de59086adf1e0e20ecffee` |
| independent recomputation | `6e9ba24e4b12260b20b3a8d2893d2ddd009649985a08f971b212fe829222e301` |
| storage/schema audit | `a5b21640bff45598a04f73000a4f5a02c27a001fbaf9dde72475f4357ec41de2` |

**Disposition: F1 PASS; retain the unchanged observability implementation and
the correctly ordered v3 evidence. F2 is eligible but not started.** F1-v1 and
F1-v2 remain historical FAIL/REVISE evidence. No profile is selected or
promoted, no production integration or Phase 4 completion is claimed, and no
commit is created.
