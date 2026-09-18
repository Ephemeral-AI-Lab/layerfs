# Stage 6 round-3 handoff — 2026-09-19

> **Status:** Active implementation routing. This is the executable assignment for
> the successor Stage 6 agent, written at the end of round 2. It **supersedes the
> routing** in [`stage-6-continuation-handoff-20260919.md`](stage-6-continuation-handoff-20260919.md)
> — that document's rules still bind **except where §4 below amends them** — and it
> supersedes nothing else. Rounds 1 and 2 are the historical record and are not
> edited.
>
> **Issue:** [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) — **open**.
> Parent [#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165). Next child is
> [#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172) (Stage 7) — **not yours**.
>
> **How you work:** one agent, working alone. **No subagents. No codex.** Everything
> happens in your session.
>
> **Read first, in this order:** [`AGENTS.md`](../../../../../AGENTS.md) §1-§4,
> [`core/AGENTS.md`](../../../../../core/AGENTS.md),
> [`docs/general/benchmark_rules.md`](../../../../general/benchmark_rules.md), the
> frozen specification under
> [`core/docs/benchmark/fs-bench-pro-storage-content/`](../../../../../core/docs/benchmark/fs-bench-pro-storage-content/CONTRACT.md)
> (seven files), the harness
> [`README.md`](../../../../../core/benchmark/fs-bench-pro-storage-content/README.md),
> and round-2 evidence:
> [`evidence/stage-6-round2-20260919T000000Z/README.md`](../evidence/stage-6-round2-20260919T000000Z/README.md).

## 1. Where the tree actually is

```text
HEAD                     55dda8ba4  the one product fix, and the round-2 records
                         f5d54e47d  prove the update read-back
                         6a1e95a02  the fixture builder, the O4 oracle, five C1 drivers
                         91d7c5d90  the WP-A test suite
                         150ded0c3  the round-1 continuation handoff
production LOC           84921 (core) / 65417 (reference) / 84920 -> 84921 this stage
```

Start by reproducing, not by reading code:

```sh
H=core/benchmark/fs-bench-pro-storage-content
cargo +1.85.1 build --release --manifest-path $H/Cargo.toml --locked
cargo +1.85.1 test --locked --manifest-path $H/Cargo.toml          # 79 Rust tests
python3 -m unittest discover -s $H/shared -p 'test_*.py'           # 102 Python tests
python3 $H/runner.py self-check
python3 $H/runner.py perf --lane smoke --out /tmp/smoke
python3 $H/runner.py report --run /tmp/smoke
```

## 2. What round 2 measured

`runner.py perf --lane full` — 220 cases in **326.787 s**:

| Class | Rows | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: | ---: |
| Admission (the frozen 217) | 217 | **167** | 3 | 47 |
| Diagnostic (`component.primitives`) | 3 | 0 | 0 | 3 |
| **Total registered** | **220** | **167** | **3** | **50** |

Round 1 was 122 / 3 / 95. `runner.py verify` re-derived all 220 statuses with **0
disagreements**, sealed call-graph **PASS** over 120 product files, runtime
tripwires **PASS** over 138 stores.

**The 47 admission rows that are not `PASS`, by cause.** Four independent causes,
not one — round 1 reported them as a single "12 product errors" bucket.

| Rows | Cause |
| ---: | --- |
| 20 | `c2.reuse.workspace` 14, `c2.pool.cold-warm` 2, `pipeline.*` 4 — no driver |
| 8 | `UNIQUE constraint failed: objects.object_id` — diagnosed in §5, **not fixed** |
| 8 | `tiny-unlink` / `tiny-bulk-delete` — an update reads back what it emits |
| 4 | `dedup-cdc-{overwrite,delete,scattered,common-body}-500` — budget overrun |
| 4 | `namespace-{10000,100000}[-text-v1]` — walk ceiling at the declared size |
| 2 | `CapacityExceeded pack.assembled_length` — 262147 and 262151 vs 262144 |
| 1 | `InvalidRecord("mapping coverage")` — `overwrite-tail-4k-500m` |

**13 of 21 registry groups have drivers** — 185 of 217 admission cases. 20 admission
rows remain `driver-unimplemented`.

## 3. The one thing that changed the rules

**Owner ruling, round 2 (2026-09-19): product source *bug fixes* are permitted;
*new features* are not.** This **supersedes** the continuation handoff §6 bullet
*"No product source change… Every finding is reported, not repaired."* Everything
else in that list still binds.

What this means in practice:

- You **may** change `core/crates/*/src` to make already-declared behaviour work.
  Every such commit reports `Production LOC: <before> -> <after> (delta <signed>)`
  per `AGENTS.md` §4, and must pass
  `cargo +1.85.1 test/clippy/fmt --manifest-path core/Cargo.toml --locked` plus
  `core/tools/check_product_boundary.py`.
- You **may not** add capability. The test for a fix: *does the product's own code
  or the frozen spec already state the behaviour that is failing?* If yes it is a
  bug; if no it is a feature and is out of scope.
- The decisive precedent for the §5 group: `cas/save.rs` states in its own comment
  that a wave may carry the same identity several times, that the first occurrence
  decides the row, and that *"inserting a second row is a hard constraint failure,
  and a store that fails on repeated content fails on exactly the workload it
  exists for."* The product documents the behaviour that is failing, so that is a
  bug, not a feature.
- **Any product change invalidates the product seal.** Rounds 1 and 2 receipts stop
  being comparable to runs made after it. Re-run into a **new** directory; never
  into an existing one.

## 4. The work, in the order that unblocks the most

### WP-1 — the mid-wave seal fix. **8 rows. Do this first.**

`UNIQUE constraint failed: objects.object_id` on
`dedup-cross-file-identical-{10,100,500}`, `dedup-cdc-insert-{10,100,500}`,
`dedup-cdc-delete-{10,100}`.

**The diagnosis, already done — do not re-derive it.**

- `owner.offer` has exactly **one** call site: `core/crates/layerfs-storage/src/cas/save.rs:84`.
- `objects` has exactly **one** writer: `core/crates/layerfs-storage/src/cas/placement.rs:156` (`write::insert_objects`).
- Therefore a duplicate row requires `offer` to be reached for an identity that already has one.
- `flush_batch` (`cas/save.rs:22`) takes `by_id` **once**, at wave start, from
  `lookup::locations`.
- A later object in the same wave hits the `None if owner.pending_member(..)` branch
  (`cas/save.rs:73`), which calls `owner.seal_pending` (`cas/placement.rs:84`). That
  seals the lane's **whole** group: rows are written for every member and
  `std::mem::take` clears `groups[index]`.
- For every **other** identity from that group, later in the same wave, `by_id` is
  now stale (miss) and `pending_member` is false (cleared) → falls through to
  `offer` → second row → SQLite refuses.
- The wave-local `prepared` map cannot help: it dedupes repeated *identities*, and
  these are different ones.

**Supporting evidence.** The failure is exactly the multi-member rows.
`dedup-cdc-insert-1` — one member, so no repeats — fails for the unrelated capacity
reason in WP-3 instead. A previous attempt to fix this by recording all decided
identities in `prepared` did **nothing** and was reverted; do not retry it.

**The shape of the fix.** The sealed member set must become visible to the rest of
the wave. `seal_pending` currently returns `()` and has a single caller
(`cas/save.rs:73`), so having it report the ids it sealed, and having `flush_batch`
consult that set alongside `pending_member`, is small and contained. Whatever you
choose, the invariant to establish is: *an identity whose row was written earlier in
this wave must not reach `offer` again in this wave.*

### WP-2 — the `CapacityExceeded` boundary. **2 rows. Likely the cheapest.**

`pack.assembled_length` limit `262144`; actual `262147`
(`store-footprint-large-object-500m`) and `262151` (`dedup-cdc-insert-1`). Three and
seven bytes over. This is a boundary bug in pack assembly or in the limit's
derivation — the same family of "off by a framing constant" that
`PACK_LIMIT`/`GROUP_TARGET` arithmetic invites. Read
`core/crates/layerfs-storage/src/pack/assemble.rs` and the constant's definition
before assuming which side is wrong; a limit that no write path can reach and no
read path can return is the failure mode the spec warns about.

### WP-3 — `InvalidRecord("mapping coverage")`. **1 row.**

`overwrite-tail-4k-500m`. Not investigated. Note it is a **different family** from
the other two `overwrite-tail` rows that pass, so compare it against
`overwrite-tail-4k-{1m,10m,100m}` rather than starting from scratch.

### WP-4 — WP-C, the three `FAIL` rows. **Harness fixture defect; no owner input needed.**

`overwrite-fixed-64k-chunk-count-decrease-{10m,100m,500m}` do not move the extent
count (536→536, 5417→5417, 26972→26972). 64 KiB of zeros chunks to exactly two
extents where the noise it replaced also occupied two; the 1 MiB tier *does* move
(53→52). Fix the **replacement content or the base's local structure**, never the
gate, the limit or the cardinality, and re-run into a **new** output directory.
`crate::families::c1_cdc::{START, LEN, ROTATIONS}` are frozen; the base content is not.

### WP-5 — WP-D, the four budget rows. **No owner input needed.**

`dedup-cdc-{overwrite,delete,scattered,common-body}-500` measure 27.98–29.63 s
against the 25 s declared exception. The oracle replay and the read-back sit
**inside** the child process, hence inside the complete command.
`benchmark_rules.md` §6 asks for setup, performance, verification and cleanup to be
separate phases. Moving the replay and read-back into a second, **unmeasured**
`verify`-mode invocation of the same case fits the rows without shrinking the
workload or enlarging a timeout — the only permitted way to make them fit. The
alternative (a longer declared exception) is an owner decision, not yours. **Do not
shrink a payload tier.**

### WP-6 — WP-B second half: 20 admission rows.

`c2.reuse.workspace` (14), `c2.pool.cold-warm` (2), `pipeline.*` (4). These are C2 /
`Store` rows, so they need the `Store` path, not the filesystem fixture builder.
`c2.reuse.workspace` is the same declared reuse equation as `c2.reuse.cross-file`,
which already has a working driver in `src/ops/c2.rs::reuse` — start there.
`pipeline.*` needs C1 construction plus C2 save acknowledgement in one region
(`PipelineOp::{EditsSmall, EditsChunked, EditsLargeToSmall, FilesystemBuild}`).

### WP-7 — the four walk-ceiling tiers.

`namespace-{10000,100000}[-text-v1]` are refused by the walk ceiling at their
declared size. **The route to them is proven, so this is engineering, not research:**
a single `build_filesystem` is accepted at exactly 4,096 bindings and refused at
4,097, but `check_parent_aliases` (`filesystem/validate.rs`) only walks when a batch
rebinds an **existing directory or symlink** — regular files are skipped before
`by_parent` is populated. So growth by *new* files and *new* directories never
charges the whole-tree walk. A probe grew a real tree to 100,000 files in 620 ms by
successive updates (build 4,096, then add in batches of ≤4,096 new bindings, never
restating an existing directory binding). Build the batched-growth driver and
declare the multi-operation structure in the row's notes; do **not** shrink the tier.

### WP-8 — WP-E, the process-kill arm.

The one WP-9 arm round 1 did not implement, because no child mode pauses inside a
save. A declared wait state in the **harness** child is ordinary harness work: it
touches no product source, no hook, no feature flag and no fault-injection surface
in `core/crates/*/src`. Implement it, record the outcome **including a failure**, and
keep the other two arms green: **W1** (hand-edited `store_policy.retained_pack_ceiling`
makes `Store::open` refuse while the unperturbed control opens) and **W3**
(`lifecycle-begin-save` gates `OwnershipUnavailable` on a second `begin_save`), both
already satisfied.

### WP-9 — closure.

Only after WP-1..WP-8: a fresh full lane into a **new** directory, `verify` with a new
tag, `report`, `calibrate`, a new dated evidence directory, and a fresh matrix. Then
re-check the nine #171 acceptance checkboxes. Round 1 left **1, 2, 4 and 5 partial**;
**#171 stays open until every one holds.**

## 5. The one ruling still outstanding

A filesystem **build** runs to completion against a non-retaining consumer and an
empty provider — measured. A filesystem **update** does not: it demands an object it
emitted earlier in the same operation, so a `DiscardingConsumer` cannot serve it.

The eight `tiny-unlink` / `tiny-bulk-delete` rows close `NOT_RUN` with the reason
attributed to the driver, not the product. The fix is small — use the
`SharedStore`/`SharedReader` overlay already in
`src/workload/providers.rs` inside `measure_update` — but it puts a retaining
consumer inside a timed phase, which the binding rules forbid. **Ask the owner
whether a retaining measured phase is admissible for an update-shaped row**, and
record the answer either way. The overlay itself is already written and proven: it
is what makes the unmeasured replay of those same inputs succeed.

## 6. Traps, so you do not pay for them again

Round 1's traps all still apply; these are the round-2 additions and corrections.

1. **`pack_cache` and `pool_reader` are two different caches.** `write_pack` released
   only the pooled one. If you add another pack-derived cache, invalidate it on write
   or you will re-create `Integrity("group ordinal")`.
2. **A build and an update differ in whether they read back.** A build does not; an
   update does. Any driver that measures an update needs an overlay reader.
3. **`TreeStore` is not enough for an update chain.** Use
   `SharedStore`/`SharedReader` (in-flight objects first, then the base) or
   `PairProvider`. Passing the caller's *base* as the overlay's first side looks like
   it works and does not — the two failures are indistinguishable until you notice
   both sides are the same store.
4. **`cargo test` in the harness needs a dedicated process for the heap window.**
   The counting allocator is process-global and the test harness's own scheduler
   thread frees memory mid-window, which understated a 4,194,304-byte pattern as
   4,194,156. `tests/instruments_selfcheck.rs` re-executes the test binary for it.
5. **Do not rustfmt the harness wholesale.** 21 round-1 files are not rustfmt-clean;
   format only the files you touch.
6. **`FilesystemResources` defaults matter.** `maximum_pending_records` is 4,096, so a
   fixture of at most that many bindings resolves in memory. Above it the operation
   needs a caller-supplied `FileBacking` or it fails `ResourceUnavailable { what:
   "ordering backing" }` — which is *not* the walk ceiling and looks like one.
7. **A `FilesystemInput` build states its own bindings**, so `check_build_reachability`
   charges every one of them. That is the 4,096 ceiling. An **update** over a base of
   4,096 is accepted (the charge is `> 4,096` that refuses).
8. `--case` overrides the lane selection entirely; `DECLARED_EXCEPTIONS` in
   `runner.py` is a hand-maintained ID list and must stay in sync with the case IDs.
9. The page size here is **16,384 bytes**. Read it; never hardcode it.
10. This shell's working directory is not stable across invocations. Use absolute
    paths; `timeout(1)` is not installed.

## 7. How to check round 2's claims rather than believe them

```sh
H=core/benchmark/fs-bench-pro-storage-content
python3 $H/runner.py self-check
python3 $H/runner.py perf --lane full --out /tmp/your-own-run
python3 $H/runner.py verify --run /tmp/your-own-run
python3 $H/runner.py report --run /tmp/your-own-run
python3 core/tools/check_product_boundary.py
python3 tools/production_loc.py
cargo +1.85.1 test --locked --manifest-path $H/Cargo.toml
python3 -m unittest discover -s $H/shared -p 'test_*.py'
git log --format='%H %s' -5
```

Every receipt carries the identity it ran under, including the harness binary's
SHA-256. A rebuilt binary invalidates the run it produced, which is why round 2's
full lane (`55dda8ba4`, harness sha `77196b68…`) and any run you make after a product
change are separate directories with separate `run.json` files.

## 8. Rules that bind you, restated because they are the ones this stage leans on

- **Product bug fixes are allowed; new features are not** (§3). Production LOC delta
  is reported on every commit.
- **A timed phase never retains, and every oracle is a second, unmeasured,
  byte-identical operation.** The two roots must agree; a replay that failed to
  reproduce the measured operation is caught by a gate rather than trusted. (See §5
  for the one place this is under question.)
- **One sample per case per arm. Fresh output. Append-only everything.**
- **Never shrink a workload, relax a limit, inflate a timeout or add a worker to turn
  a number green.** A selection that cannot fit is `NOT_RUN` with its measured wall
  time.
- **`elapsed_ns` never gate-decides.** Counters, heap and disk do.
- **Do not re-open a Stage 5 row. Do not promote a Stage 5 `NOT_RUN` or owner-WAIVED
  row. Do not run `tools/preflight.sh`, restore it, or create an aggregate gate or CI
  replacement.**
