# Prospective G2 materialization decomposition v1

Status: `FROZEN BEFORE SOURCE CHANGE OR MEASURED ROW`

Date: 2026-08-22

Repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`

Branch: `codex/empty-worktree`

Starting HEAD: `d79f0e0e2582d1bc491410224fec2b6cef7482e9`

## Decision before implementation

G0 and G1 remain complete. G1 retains benchmark-private
`PRAGMA cache_spill=2000;`. G2 selects exactly one variable: bounded scalar
observation of the existing authenticated logical-reconstruction pass. G2 is
a diagnostic, not a performance optimization, native materializer, trust
shortcut, page profile, cache policy, worker, queue, or concurrency feature.

Three independent read-only lanes inspected the current source, G1 and
Canonical-v2 raw evidence, timers/counters, authority contracts, prior Phase-4
research, and SQLite primary documentation. Each ranked G2 decomposition
first. Their cross-review agreed that controlling evidence must perform every
existing operation once, in the existing order, with no skipped or deferred
authentication/hash pass. Counterfactual skip-work rows are excluded.

Current unresolved measured boundaries are:

| Boundary | Retained observation |
|---|---:|
| Durable 100-MiB create, G1 | 308.884052 ms |
| Authenticated reconstruction, warm | 338.775916 ms |
| Authenticated reconstruction, fresh process | 366.356667 ms |
| Returned authenticated 1-MiB range | 2.279209 ms |
| Reopen / visible head | 2.088334 ms |
| First edit after reopen | 154.019083 ms |

The reconstruction result is a logical hashing/counting sink. It is not a
native destination write, controlled-cold evidence, trusted hot read, or
incremental materialization.

## Ranked operation matrix

`=` means no direct mechanism. `?` means a hypothesis requiring a prospective
measurement. Storage columns distinguish canonical/logical bytes from SQLite
apparent and allocated bytes; none is a physical-media observation.

| Rank / direction | Direct expected benefit and measured budget | Create and edits | Materialization, reads, open/reopen | Memory and Q | Storage | Lock / concurrency | Durability, format, migration | Fastest falsifying screen | Disposition now |
|---|---|---|---|---|---|---|---|---|---|
| 1. G2 exact in-path decomposition | Separates the 338.776/366.357-ms authenticated reconstruction parent into acquisition/authentication/commitment/decode/fingerprint families and checked residual | No execution change; create/edit are guards only | Direct evidence for reconstruction; range/reopen protected; does not implement native output | Fixed scalar stack fields and bounded report growth only; terminal Q remains zero | No DB or sidecar write; endpoints and hashes exact | No thread, connection, transaction, cache, or lock-policy change; diagnostic concurrency is not product evidence | FULL+DELETE and one-writer publication untouched; no format/migration change | Adjacent frozen-control versus instrumented warm pass, exact observer bound and timer equation | **Selected one variable** |
| 2. Bounded ordered CDC/canonical pipeline | At most the smaller safely overlapable producer/SQLite lane; standalone current CDC gross wall is 43.786594 ms while the split inside 263.736949-ms G1 mapping is unavailable | Possible full-create/full-rewrite win; small edits likely neutral or worse | Directly `=` for reconstruction/range/open | Adds worker stack, owned chunk slots, bounded queue and aggregate-Q problem; current Q is thread-local | Canonical/logical bytes should be `=` only if exact order is retained | CPU contention, cancellation, panic/error precedence and 1/2/4-store aggregate RSS risks | Must keep SQLite caller-owned, one transaction/COMMIT and exact identities; execution-contract change | 10-MiB depth-one/rendezvous parity with producer/consumer idle time and injected error order | Defer: no isolated overlap budget |
| 3. 16-KiB SQLite page profile | Fewer pager/B-tree/overflow events are plausible; current read-only `dbstat` finds 23,451 overflow pages among 26,660 4-KiB pages, but DB overhead above canonical payload is only 3.878% | Create `?`; small edit write granularity may regress | Reconstruction/sequential read `?`; 4-KiB ranges may regress; open `?` | Unchanged 2,000-page cache/spill values imply roughly a 32-MiB cache class; fixing bytes to 500 pages stacks variables | Canonical/logical `=`; apparent/allocated `?` | Larger cache delays spill in bytes but aggregate RSS rises; lock timing unmeasured | Persistent SQLite physical format; fresh creation or VACUUM/rejection/migration required; page profile is not currently stored/validated | Fresh exact-schema 4K/16K layout replay plus create/edit/reconstruction/range guards | Defer: violates G1 memory class or one-variable rule |
| 4. Intermediate `cache_spill=4096` | Trades mapping spill wall against COMMIT and delays first EXCLUSIVE lock; G1 already beat 20,000 by 19.169 ms total | Large create `?`; current small edits spill zero and are directly `=` | Read/materialize/open directly `=` | Predicts about 8 MiB more page cache before overhead than G1; exact RSS unavailable | Canonical/logical/apparent endpoints expected `=` but must be measured | Only credible upside is later first spill and shorter reader blocking; first-spill/EXCLUSIVE interval is unavailable | Runtime-only; FULL+DELETE and format `=` | One adjacent writer plus independent authenticated old-head reader with explicit timeout and first-spill timestamp | Defer: higher memory has no measured cross-operation benefit |

Primary SQLite contracts used for the matrix:

- <https://www.sqlite.org/pragma.html#pragma_cache_spill>
- <https://www.sqlite.org/lockingv3.html#writing_to_a_database_file>
- <https://www.sqlite.org/atomiccommit.html#cache_spill_prior_to_commit>
- <https://www.sqlite.org/c3ref/c_dbstatus_options.html>
- <https://www.sqlite.org/fileformat.html#cell_payload_overflow_pages>
- <https://www.sqlite.org/pragma.html#pragma_page_size>

These primary sources establish logical pager/locking/file-format behavior.
They do not establish LayerFS wall time, physical I/O, or target-host outcomes.

## Frozen source, binary, fixture, and base custody

| Item | Frozen value |
|---|---|
| Starting benchmark source | `157699e0cd4cb1e3b5ec631cefb7c967ff7433bdeeb10ee1336e70961b402ad2` |
| FastCDC source | `bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6` |
| Exact G1 control executable | `42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55` / 1,372,784 B |
| 100-MiB source | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` / 104,857,600 B |
| Completed G1 base DB | `7db8d50de42b994546789cb67fc7a9b650e2e551dab118e15003e02106b19890` / 109,199,360 B |
| Completed G1 authority | `7855ea6096359925f639b91c8d6b9708cfe0bc0df4a3ffd97a280a8e9a9ded48` / 32 B |
| Completed G1 expectations | `a7489b01445e53aa8a0c5824059b8a6b04f92e15a3b6cf953fbb4c83d6b5e18a` / 1,096 B |
| Profile | `94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b` |
| Expected source fingerprint | `bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7` |
| Expected occurrence commitment | `5bf63ef8adad7bb373be4e968759997b385590d076d75a1488e1958b64a3f8e2` |
| Expected root | `93d1b461b5cbf88e8d122ad4e90a15a6e3029c408f0fe02ef51c561e5a94c6d1` |
| Expected transition | `2de8d2ce6b614373ba8fb8b29d3a3eccd535abfccc2654e7922514e5ca90fd89` |
| Expected closure | `29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1` |

The source, completed base, and all row operands are copied into a new private
namespace. The sealed G1 paths are never opened for mutation. Candidate source,
diff, executable, preregistration, runner, and analyzer hashes are frozen in a
separate append-only `INPUT-BINDINGS-v1.json` before any measured row; this
preregistration is not edited to insert post-implementation hashes.

Fresh result namespace:

`target/phase4-g2-materialization-decomposition-20260822-v1`

Acquisition refuses to start if the namespace or
`target/phase4-g2-materialization-decomposition-20260822-v1.lock` exists.

## Sole implementation variable

Add benchmark-private, environment-gated scalar timing fields around existing
read work. The measured B arm sets `LAYERFS_G2_DECOMPOSE=1`. The A arm runs the
frozen G1 executable. The B binary with the environment absent is used for
unaffected guards.

The instrumentation may add only fixed scalar state and bounded JSON fields.
It must not add a vector, event log, worker, queue, cache, connection, SQL
statement, PRAGMA, database field, sidecar, dependency, skipped operation,
second traversal, or output-sized allocation.

Within the existing `reconstruction_wall_ns` parent, record disjoint wall:

1. `sqlite_blob_acquisition_wall_ns`: statement construction/acquisition,
   query creation, `rows.next`, `get_ref`/`as_blob`, and the existing bounded
   owned copy for mapping objects; callback work is excluded.
2. `canonical_authentication_wall_ns`: the existing complete ObjectId check
   and strict canonical Bytes framing decode. `validate_bytes_identity`
   performs both and is not split into overlapping timers.
3. `mapping_validation_wall_ns`: existing namespace/file mapping decode,
   topology/order/length checks and summaries outside canonical auth.
4. `closure_commitment_wall_ns`: existing ordered closure updates/finalize.
5. `occurrence_commitment_wall_ns`: existing ordered occurrence
   updates/finalize.
6. `source_fingerprint_wall_ns`: existing reconstructed raw-source hash
   updates/finalize. This is a logical verification sink, not native output.
7. `secondary_bytes_decode_wall_ns`: the current callback's second bounded
   `decode_bytes_object` plus raw-length check.

The raw residual is derived, never timed or corrected:

```text
raw_residual = reconstruction_parent - sum(disjoint_direct_timers)
```

It includes delivery accounting, loop/control work, timer overhead, and any
unclassified work. A negative residual is impossible and forces `REVISE`.
No residual or composite may select a later candidate.

The B row records exact timer-region count and the operation Q high-water
before report rendering. The expanded JSON report's exact capacity is charged
normally; report growth is not hidden as unchanged Q.

## Observer bound

The instrumented warmup B row freezes the actual timer-region count before
measured rows. Run five optimized no-work probes with that exact count. Each
probe executes the same `Instant::now`, `elapsed`, checked `u128` add, and
checked timer-count add shape. The maximum complete probe wall is an observer
ceiling only; raw child and parent values are not corrected.

Require:

```text
observer_ceiling <= min(5 ms, 1% of the instrumented warm parent)
```

Timer-region count must be identical across all four measured B rows. The
candidate/control parent ratio must be `<=1.05` in all four pairs and at the
position-balanced center, with both positions protected.

## Frozen schedule and time ceilings

Every child receives a fresh private database/authority/expectations copy.
Base preparation, copying, hashing, modes, and fixture preflight are outside
all row timers. The complete measured campaign is acquired once; no row is
deleted, replaced, or selectively rerun.

Primary warm decomposition and observer-equivalence schedule:

```text
uncounted warmup pair: AB
five observer probes at the B warmup's exact timer count
measured pair 1: AB
measured pair 2: BA
measured pair 3: AB
measured pair 4: BA
```

`A` is exact G1 source/executable with no new fields. `B` is the once-built
instrumented source with `LAYERFS_G2_DECOMPOSE=1`. The operation is
`materialize-warm`; its first reconstruction is the existing untimed primer
and only its second reconstruction is decomposed.

After and only after a valid primary screen, run these fixed adjacent guard
pairs once each:

```text
AB materialize-fresh
AB read-range-1m
AB reopen
AB edit-same, capture-only
```

For the B materialization guard only, decomposition is enabled. For range,
reopen, and edit, the B binary runs with decomposition disabled. These are
parity/regression guards, not a balanced optimization campaign.

The first child through primary analysis must finish within 20 seconds. The
entire measured campaign must finish within 120 seconds. Either ceiling breach
is sealed as `REVISE`; acquisition does not resume.

Concurrency is `NotRun(reason=diagnostic-only scalar observation; no retained
execution, thread, connection, transaction, cache, statement-batching, or
lock-policy variable)`. Instrumentation inside a borrowed-row callback can
lengthen a SHARED-statement lifetime microscopically, so an instrumented
concurrency row would measure observer perturbation, not a later product
candidate. A causal concurrent-load guard is mandatory for the actual later
candidate. Current G1 concurrent-reader behavior remains explicitly
unqualified.

## Exact semantic, work, resource, and storage gates

Every one-pass 100-MiB reconstruction must preserve:

```text
source/output bytes                         104,857,600
CDC occurrences / chunk references                5,284
authenticated objects                              5,371
authenticated canonical bytes                105,122,401
borrowed chunk BLOB reads / bytes          5,284 / 104,926,292
leaf batches / references                       83 / 5,284
read-operation SQL queries / rows                170 / 5,371
fresh row total SQL queries / rows               173 / 5,374
transactions / COMMITs                               0 / 0
database cache writes / spills                        0 / 0
```

Also require exact profile, source fingerprint, occurrence commitment, root,
transition, closure, output length, timer parent, nonnegative residual,
timer-region count, query/row/BLOB work, Q equation and terminal zero,
database/authority/expectations hashes and modes, logical/apparent/allocated
endpoints, and zero journal/WAL/SHM/temp residue.

The warm row's complete process counters contain the untimed primer plus timed
pass. One-pass gates apply to the `read_operation` phase delta; total warm row
counters are not misreported as one pass.

Guard pairs require exact identities/work/storage and B/A wall no worse than:

```text
materialize-fresh: 1.05
read-range-1m: max(1.05 ratio, 200 us absolute allowance)
reopen: max(1.05 ratio, 200 us absolute allowance)
edit-same: 1.05
```

No source, database, journal, authority, expectations, or result file is used
as a physical-I/O proxy. True OS/device cold state, VFS calls/bytes, sync wall,
physical-media bytes, true SQLite cache high-water, current dirty set, lock
duration, and true journal/temp peaks remain unavailable unless directly
observed.

## Mechanism-selection and terminal decision

G2 may select a following candidate only when the same directly timed family:

1. is at least 33 ms in all four measured B rows;
2. is at least 33 ms in both execution positions;
3. has a non-overlapping timer and exact byte/call evidence;
4. is independently removable or replaceable under the frozen authority,
   identity, durability, error, output, memory, and storage contracts; and
5. admits one separately preregisterable implementation variable.

A mandatory 33-ms family is not removable. SQLite acquisition does not become
removable because it is large; canonical authentication does not become
optional because it is large; source fingerprint work does not become native
output; and a residual/composite cannot issue `GO`.

Terminal dispositions:

- `G2 PASS / SELECT <one later candidate>`: every diagnostic/parity gate passes
  and exactly one directly timed removable family passes all selection gates.
- `G2 PASS / INSUFFICIENT_EVIDENCE FOR A CONSTANT-FACTOR CANDIDATE`: the
  diagnostic is exact but no removable family passes; recommend the
  destination-authority/incremental-materialization lane rather than inventing
  a micro-optimization.
- `G2 REVISE`: observer, timer, parity, resource, storage, custody, manifest, or
  time-ceiling failure. Revert only the diagnostic source and retain G1.

G2 does not implement the later candidate. Any later candidate requires a new
one-variable preregistration and its own edit/materialization/read/open/reopen/
concurrency/storage/memory acceptance.

## Commands

Before source change, the exact G1 control is built and copied outside the
workspace, then hashed. After this document is frozen:

```text
cargo test --offline -p layerfs-engine --bin phase4_create_edit_benchmark g2_materialization_decomposition_
cargo fmt --all -- --check
git diff --check
cargo build --offline --release -p layerfs-engine --bin phase4_create_edit_benchmark
```

The candidate release executable is built once. Before acquisition, freeze
source/diff/binary/methodology/fixture/base hashes, prove the result root and
lock absent, prepare all private copies, and dry-run the exact schedule with
zero child invocations.

Only after a valid primary and guard campaign may static closure run:

```text
cargo test --workspace --offline --all-targets
cargo clippy --workspace --offline --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```

No 500-MiB work, page-size change, second spill threshold, worker/pipeline,
schema/profile/identity change, native materializer, G3 implementation, WP5,
Phase 5, push, amend, merge, or rebase is authorized by this preregistration.
