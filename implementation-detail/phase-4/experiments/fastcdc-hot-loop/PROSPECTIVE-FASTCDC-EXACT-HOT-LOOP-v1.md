# Prospective FastCDC exact hot-loop experiment v1

Status: **FROZEN BEFORE THE FIRST MECHANISM-SCREEN ROW**. This document
authorizes one safe-Rust, exact-boundary candidate, one CDC-only kill screen,
and only on a complete screen pass one short adjacent durable A/B. It does not
authorize a second FastCDC candidate or any H09, SQLite, concurrency,
materialization, reopen-authority, production-integration, or commit work.

## Custody and one variable

| Item | Frozen value |
|---|---|
| repository / branch / checkpoint | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` / `codex/empty-worktree` / `daf4cefc1fd7861681de3f94bf042b556cc21ccb` |
| accepted Canonical-v2 benchmark source | `16e9beedd2fe49d6da65f89f53f488cffbfdcfc71f10477e854cd2d37d00e120` |
| accepted Canonical-v2 durable executable | `f3dd4c9420cc7bb7e7390960db9bf6e4a4a44de3d15dc0573002d3172b570280` |
| accepted profile | `94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b` |
| control `cdc/mod.rs` | `82d8463101675e8f0e5632b532a3a96893405adaa09d311fddb25ca322620940` |
| candidate `cdc/mod.rs` | `eed17659d0d8f86793ad6b0ffbd4a6b89470555503aa033dda1d3acb5e417923` |
| exact candidate diff | `facc341de5ffff6d25d1daab5f8219b5784672326e9211d86aebb14997dc7816` |
| CDC screen control executable | `a3a0808fc98148a979dfde9d70030b925a0dbd83acd946c7b089e2e2c8515f0d` |
| CDC screen candidate executable | `6de6085e4eaaf140a59d944876316ad433a450fb78a1435f19f3ff29f920f814` |
| conditional durable candidate executable | `9160fcad455af20aecd04c28b59665d0f414e52aa91a4cb845746b4e2961774f` |
| retained fixture | 104,857,600 bytes; SHA-256 `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4`; BLAKE3 `bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7` |
| target namespace | `target/phase4-fastcdc-exact-hot-loop-20260821-v1` |

The only production change replaces the two per-pair small/large mask
selections with two scanner-local active-mask scalars. They start with the
existing small masks, switch to the existing large masks exactly when
`next_even == TARGET_CHUNK_BYTES`, and reset on the existing emission boundary.
The Gear table, chunk sizes, masks, normalization, rolling update order,
pending-byte behavior, callback, buffer, and every identity remain unchanged.
The candidate adds no dependency, allocation, abstraction, unsafe code,
worker, thread, task, queue, pool, SIMD, or new execution profile.

## Checks completed before timing

The focused commands are frozen as:

```text
cargo test -p layerfs-core --offline cdc::tests
cargo test -p layerfs-core --offline cas::tests::scan_identity_deduplicates_and_preserves_callback_bytes
cargo test -p layerfs-core --offline content::tests::full_replace_is_a_separate_streaming_path
cargo test -p layerfs-core --offline --test canonical_v2_fixture_oracle independent_actual_fixture_oracle_freezes_complete_v2_corpus
cargo fmt --all -- --check
git diff --check
rustfmt --check implementation-detail/phase-4/experiments/fastcdc-hot-loop/fastcdc_exact_screen.rs
```

They passed before release construction. The independent oracle covers the
retained 1/10/100-MiB fixtures and freezes the 100-MiB count at 5,284. The CDC
tests cover frozen small boundaries, empty/short/minimum/maximum edges, five
fragmentation patterns, exact byte totals, no empty or over-maximum chunk,
fixed 32,768-byte scanner capacity, and exact callback-error propagation.

The control scanner was built once from a clean `git archive` of the checkpoint
plus the identical screen harness. The candidate scanner and durable binary
were built together once from an independent archive containing only the
candidate `cdc/mod.rs` and that harness:

```text
cargo build --release --offline -p layerfs-core --example fastcdc_exact_screen
cargo build --release --offline -p layerfs-core --example fastcdc_exact_screen -p layerfs-engine --bin phase4_create_edit_benchmark
```

No additional release build or rebuild is admissible.

## CDC-only mechanism screen

Timer boundary: `Instant` immediately before `FastCdc::scan` through its
successful return. File open, exact-boundary file serialization, acquisition
custody, analysis, and manifest work are outside each row timer but inside the
global screen clock. The callback performs the same bounded work in both arms:
one BLAKE3 reconstruction update plus one fixed-capacity boundary record. This
is scanner-only evidence and never opens SQLite or runs full create.

Schedule, exactly ten invocations:

```text
uncounted warmup AB
measured pair 1 AB
measured pair 2 BA
measured pair 3 AB
measured pair 4 BA
```

The screen global clock starts immediately before warmup A and includes all ten
invocations, acquisition custody, fail-closed analysis, disposition, and the
small manifest. It must be less than 19,000,000,000 ns. A timeout is `REVISE`
evidence and causes final candidate revert; no row is rerun or removed.

Every row reports input/scanned/read bytes, occurrences, callback count, summed
lengths, minimum/maximum length, read calls, scanner/boundary capacities,
scanner wall, `/usr/bin/time -l` user/system CPU and maximum RSS, BLAKE3 of the
reconstructed callback bytes, and the exact boundary transcript. The transcript
is BLAKE3 over ordered `u64be(start) || u64be(end) || u64be(length)` records.
Each row also retains an independently parsed TSV containing every ordinal,
start, end, and length. The exact TSV SHA-256 must match across all ten rows.

Hard parity is exact equality across arms and rows for 104,857,600 consumed,
5,284 callbacks/occurrences, total length 104,857,600, every boundary TSV,
transcript fingerprint, reconstructed source BLAKE3, min/max, read shape,
32,768-byte scanner capacity, and 5,284-record bounded observer capacity.
Callback errors and fragmented readers are protected by the focused tests.
Any mismatch is immediate `PARITY FAIL / REVERT` and forbids durable work.

The position-balanced center is the arithmetic mean of the four measured rows
per arm; equal AB/BA positions make it the arithmetic mean of the two position
strata. A pair favors B only when its B scanner wall is strictly lower. Each
position wins only when the mean of its two B observations is strictly lower
than the mean of its two A observations.

Advance requires all of:

- at least 15.000 ms position-balanced scanner wall saved;
- at least 10% relative scanner improvement;
- at least three of four pairs favor B;
- both execution-position strata favor B;
- position-balanced user and system CPU each no more than 5% above A;
- paired median maximum-RSS ratio no more than 1.05 and at least three of four
  pair ratios no more than 1.05;
- identical fixed scanner allocation/capacity and every parity/custody gate.

If any signal condition fails, retain the complete screen as
`FASTCDC EXACT HOT LOOP NO-GO / REVERT`, revert only the production/test
candidate in `cdc/mod.rs`, verify tracked source equals the checkpoint, and
stop. No second loop variant is admissible.

## Conditional durable A/B

This runs without approval only if the frozen screen analysis says
`advance_to_durable=true`. A and B are respectively the exact accepted durable
binary and the once-built candidate above. Each arm is prepared once outside
row timers; every row receives an independent physical byte copy of its arm's
database, authority, and expectations plus the exact retained fixture.

Schedule, exactly ten invocations:

```text
uncounted warmup AB
measured pair 1 AB
measured pair 2 BA
measured pair 3 AB
measured pair 4 BA
```

The row boundary is the existing `--fast-row ... write ... capture-only`
durable-submit timer. The global benchmark clock is inherited from the start
of the mechanism screen and has an absolute 119,000,000,000-ns ceiling,
including conditional preparation, acquisition, analysis, disposition, and
manifest. A measured row is never rerun, deleted, repaired, or replaced.

Hard gates per row are the exact profile/source/canonical occurrence
commitment, root `93d1b461...c6d1`, transition `2de8d2ce...fd89`, closure
`29233d60...c0c1`, 5,284 occurrences, 5,372/0 created/reused,
105,122,466 canonical bytes, 196,174 mapping bytes, 5,381 SQL calls,
10,748 BLOB writes, one transaction, one successful COMMIT dispatch/return,
`FULL + DELETE`, `temp_store=FILE`, `mmap_size=0`, exact timer equation,
zero graph authentication in `sqlite_commit`, Q high-water no greater than
86,181, terminal Q zero, exact logical/apparent database/store endpoints, and
no journal/WAL/SHM residue. Candidate/control schema, metadata, profile, and
serialized observations must be identical.

The durable position and pair definitions match the screen. Retain requires
at least three of four B wins, both position strata, at least 10.000 ms and 2%
position-balanced durable improvement, and lower position-balanced candidate
mapping wall consistent with the scanner screen. For user CPU, system CPU,
maximum RSS, and allocated store endpoint, the paired median B/A ratio must be
at most 1.05 and at least three of four pair ratios must be at most 1.05, with
no unexplained monotonic growth or residue. Physical I/O, sync-call counts,
instructions, cycles, true cold-cache state, phase-local CPU, and non-scanner
heap allocation remain `Unavailable` because the frozen public/runtime
observers cannot measure them; no wall/RSS/logical-byte proxy is permitted.

## Terminal decision

If the screen and durable campaign pass, run workspace tests, Clippy with
warnings denied, rustfmt, tracked/relevant-untracked whitespace checks,
independent recomputation, and a final read-only custody audit. Only then mark
`FASTCDC EXACT HOT LOOP PASS / RETAIN`, freeze a baseline-successor report and
manifest, and update the Phase-4 roadmap to name the exact candidate executable
as next control. Do not commit.

Any parity, semantic, custody, clock, Q, durability, storage, resource, or
performance failure marks `FASTCDC EXACT HOT LOOP NO-GO / REVERT`, preserves
all evidence, reverts only the candidate `cdc/mod.rs` change, verifies tracked
source at `daf4cef...`, and stops without durable work if the screen failed.
