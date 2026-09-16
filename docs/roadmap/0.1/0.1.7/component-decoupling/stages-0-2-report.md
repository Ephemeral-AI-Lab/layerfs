# Stages 0–2 implementation report

> **Status:** Implementation report for [#166](https://github.com/Ephemeral-AI-Lab/layerfs/issues/166)
> (Stages 0–1) and [#167](https://github.com/Ephemeral-AI-Lab/layerfs/issues/167)
> (Stage 2). Parent [#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165)
> and Stages 3–7 stay open.

Read with the [handoff](stages-0-2-handoff.md), the component designs it lists and
the package READMEs:
[`layerfs-content`](../../../../../core/crates/layerfs-content/README.md) (C1) and
[`layerfs-storage`](../../../../../core/crates/layerfs-storage/README.md) (C2).

## 1. What was built

```text
stable complete-file input
        |
C1  layerfs-content:
      canonical identity + envelope framing        object/
      frozen GEAR chunker + bounded scanner        file/cdc/
      extent-tree build/codec/reads                file/mapping/
      empty / whole-file / chunked dispatch        file/content.rs
        |
bounded finalized-object handoff (FinalizedObject -> FinalizedConsumer)
        |
C2  layerfs-storage:
      exact CAS reuse + collision rejection        cas/membership.rs
      pinned FULL codec + three pack framings      encoding/, pack/
      one mutation owner, bounded shared SQL       cas/owner.rs, sqlite/
      retained-pack ceiling reads                  cas/read.rs
        |
acknowledged storage -> close/reopen -> authenticated readback
```

The three required modes are independently runnable and were actually run:

| Mode | Real operation | Where |
| --- | --- | --- |
| C1 only | stable input → real constructor → bounded non-persisting consumer → root + length | `--mode c1` |
| C2 only | supplied bounded canonical fixture → real CAS save → transaction acknowledgement → independent authenticated read | `--mode c2` |
| Integrated | real C1 → bounded C2 handoff → final storage completion → authenticated logical readback | `--mode pipeline` |

## 2. Frozen profile, capacities and format limits

Frozen and validated before any work; unsupported values fail explicitly and are
never clamped:

| Item | Accepted value |
| --- | --- |
| Construction cutoff `T` | `131072` bytes, exclusive (the only accepted value) |
| Whole-file / chunk delta depth | `8` / `4` recorded; DELTA is **not** implemented |
| Empty file | file state over a defined empty mapping page |
| `0 < length < T` | one `WHOLE_FILE` object, value `LFS5SML\0` + version `1` + raw |
| `length >= T` | `FILE_STATE` (v3 mapping grammar) → extent tree → `CHUNK` objects |
| Largest whole-file object | 131071 raw bytes, 131094 canonical bytes |
| Largest chunk | 32768 raw bytes, 32789 canonical bytes |
| Envelope ceiling per canonical object | 16 MiB (role-independent guard; **not** a file or chunk limit) |
| Per-field ceiling | 8 MiB |
| Largest object any role produces | 131,094 B whole-file · 32,789 B chunk · 8,192 B mapping page |
| Maximum file size | none declared; bounded by `u64` and by cost |
| Format profile | `1`; schema `application_id = 1279677261`, `user_version = 1` — **superseded: `user_version = 2`, see §8** |
| Encoding | **FULL only** (may be compressed); no DELTA, no pooling |

Physical limits: pack `<= 256 KiB`; group body `<= 64 KiB` (one record per group
in the compact whole-file lane); `<= 8191` records per group; `<= 256` groups per
pack; lookup page `128` identifiers; pending batch `512` objects / `512 KiB`;
write transaction `8191` rows / `4 MiB - 1` canonical bytes.

## 3. Actual file tree and estimates versus actual

`45` production files, **5293** production LOC
(C1 2322, C2 2971, of which candidate runtime SQL 44).
The handoff's combined recommendation was 4241–7479 production LOC across 46
planned source files, so the implementation lands **below** the recommended range
in total; per-file verdicts are recorded below with a reason for every departure.
Physical file ceilings (999; 200 for `lib.rs`/`mod.rs`) all hold.

Planned file not created: `core/crates/layerfs-storage/src/pack/read.rs`. Record
extraction and reconstruction are one operation and merged into
`src/encoding/decode.rs` rather than duplicated. No file was split to evade a
limit and no empty placeholder exists.

| directory | recommended | actual |
| --- | ---: | ---: |
| `core/crates/layerfs-content/src/` (recursive) | 1408–2479 | 2322 |
| `core/crates/layerfs-content/src/file/` | 1010–1740 | 1761 |
| `core/crates/layerfs-content/src/file/cdc/` | 124–218 | 499 |
| `core/crates/layerfs-content/src/file/mapping/` | 678–1156 | 946 |
| `core/crates/layerfs-content/src/object/` | 281–514 | 324 |
| `core/crates/layerfs-storage/sql/` | 80–140 | 44 |
| `core/crates/layerfs-storage/src/` (recursive) | 2753–4860 | 2971 |
| `core/crates/layerfs-storage/src/cas/` | 918–1618 | 931 |
| `core/crates/layerfs-storage/src/encoding/` | 426–762 | 591 |
| `core/crates/layerfs-storage/src/pack/` | 596–1044 | 649 |
| `core/crates/layerfs-storage/src/sqlite/` | 678–1176 | 504 |

### Per-file comparison

Produced with the repository counter
(`python3 tools/production_loc.py --files`), the same classification used for the
totals. "physical" is the independent 999/200-ceiling measure; it is never the
production LOC.

| path | recommended production LOC range | actual production LOC | physical lines | within/below/above | explanation |
| --- | ---: | ---: | ---: | --- | --- |
| `core/crates/layerfs-content/src/error.rs` | 35–65 | 116 | 168 | above | Above range. Every variant names exactly one check and the `Display` text is part of the failure evidence; collapsing variants would lose which check failed. |
| `core/crates/layerfs-content/src/file/cdc/gear.rs` | 120–210 | 494 | 538 | above | Above range because the frozen 256-entry gear table is shipped data, not logic. The scanner itself is close to the estimate. |
| `core/crates/layerfs-content/src/file/cdc/mod.rs` | 4–8 | 5 | 10 | within |  |
| `core/crates/layerfs-content/src/file/content.rs` | 90–150 | 195 | 249 | above | Above range. Whole-file framing, the empty/small/chunked dispatch, the bounded threshold probe and root classification belong to one responsibility. |
| `core/crates/layerfs-content/src/file/mapping/build.rs` | 220–380 | 236 | 267 | within |  |
| `core/crates/layerfs-content/src/file/mapping/codec.rs` | 180–300 | 303 | 334 | above | Marginally above range. Three grammars plus the frozen mapping-profile identity; the profile identity cannot move without splitting one frozen value. |
| `core/crates/layerfs-content/src/file/mapping/mod.rs` | 8–16 | 15 | 20 | within |  |
| `core/crates/layerfs-content/src/file/mapping/read.rs` | 180–300 | 210 | 241 | within |  |
| `core/crates/layerfs-content/src/file/mapping/types.rs` | 90–160 | 182 | 248 | above | Above range. Field accessors and the checked page-invariant validator share the type they validate. |
| `core/crates/layerfs-content/src/file/mod.rs` | 8–16 | 10 | 16 | within |  |
| `core/crates/layerfs-content/src/file/read.rs` | 110–200 | 111 | 127 | within |  |
| `core/crates/layerfs-content/src/lib.rs` | 12–30 | 16 | 33 | within |  |
| `core/crates/layerfs-content/src/object/access.rs` | 60–110 | 18 | 36 | below | Below range. One grouped provider method and one point-read convenience; there is no per-algorithm trait. |
| `core/crates/layerfs-content/src/object/codec.rs` | 110–200 | 95 | 126 | below | Below range. The envelope is genuinely small; nothing was merged in and no behaviour is hidden elsewhere. |
| `core/crates/layerfs-content/src/object/id.rs` | 45–80 | 89 | 119 | above |  |
| `core/crates/layerfs-content/src/object/mod.rs` | 6–14 | 11 | 17 | within |  |
| `core/crates/layerfs-content/src/object/output.rs` | 60–110 | 111 | 170 | above | Above range. Role codes, the finalized-object type and the discarding consumer; the consumer carries real bounded accounting, not a test stub. |
| `core/crates/layerfs-content/src/policy.rs` | 70–130 | 105 | 165 | within | Within range. Typed policy, the accepted-value validation and the derived capacities. |
| `core/crates/layerfs-storage/sql/schema.sql` | 80–140 | 44 | 56 | below | Below range. Four tables and two indexes need less DDL than the estimate; column count was nineteen — **superseded: twenty, see §8** |
| `core/crates/layerfs-storage/src/cas/batch.rs` | 110–190 | 58 | 87 | below | Below range. The bounded buffer is small by design; the limits live in `policy.rs`. |
| `core/crates/layerfs-storage/src/cas/dependencies.rs` | 100–180 | 62 | 88 | below | Below range. One availability set and one bounded validation. |
| `core/crates/layerfs-storage/src/cas/finish.rs` | 100–180 | 16 | 25 | below | Below range. One terminal disposition; deliberately not merged into `store.rs` so the single failure boundary is visible. |
| `core/crates/layerfs-storage/src/cas/membership.rs` | 120–210 | 42 | 59 | below | Below range. One exact-comparison decision. |
| `core/crates/layerfs-storage/src/cas/mod.rs` | 8–18 | 11 | 16 | within |  |
| `core/crates/layerfs-storage/src/cas/owner.rs` | 90–160 | 322 | 391 | above | Above range. One cohesive owner: cursors, per-lane placement state, pending groups, the open transaction and the terminal flag. Splitting it would spread one invariant across files. |
| `core/crates/layerfs-storage/src/cas/read.rs` | 140–240 | 65 | 83 | below | Below range. One grouped wave under one captured ceiling. |
| `core/crates/layerfs-storage/src/cas/save.rs` | 180–320 | 40 | 56 | below | Below range. The wave loop is short because membership, dependencies and placement are separate modules. |
| `core/crates/layerfs-storage/src/cas/store.rs` | 70–120 | 315 | 407 | above | Above range. The public `Store`, `SaveOperation` and `SaveHandoff` live together because they are one public contract; splitting the surface would move state that must stay private. |
| `core/crates/layerfs-storage/src/encoding/codec.rs` | 120–220 | 367 | 461 | above | Above range. Two bounded workspaces and two parameter sequences, each with its own safety argument. Merging lanes would not reduce it. |
| `core/crates/layerfs-storage/src/encoding/decode.rs` | 160–280 | 103 | 115 | below | Below range. This file absorbed the planned `pack/read.rs`: reconstruction and record extraction are one operation and were merged rather than duplicated. |
| `core/crates/layerfs-storage/src/encoding/full.rs` | 140–250 | 112 | 132 | below | Below range. Three small FULL encoders. |
| `core/crates/layerfs-storage/src/encoding/mod.rs` | 6–12 | 9 | 15 | within |  |
| `core/crates/layerfs-storage/src/error.rs` | 45–90 | 93 | 135 | above | Above range. The unknown-persistence-outcome and cleanup-failure boundaries are explicit variants; a smaller enum would lose that distinction. |
| `core/crates/layerfs-storage/src/lib.rs` | 15–30 | 11 | 30 | below |  |
| `core/crates/layerfs-storage/src/pack/assemble.rs` | 140–240 | 195 | 218 | within | Within range. |
| `core/crates/layerfs-storage/src/pack/layout.rs` | 170–300 | 321 | 392 | above | Above range. Three grammars, the exact fit arithmetic and the directory/entry validation share one parser. |
| `core/crates/layerfs-storage/src/pack/mod.rs` | 6–14 | 10 | 15 | within |  |
| `core/crates/layerfs-storage/src/pack/placement.rs` | 110–190 | 123 | 159 | within | Within range. |
| `core/crates/layerfs-storage/src/policy.rs` | 75–140 | 148 | 219 | above | Above range. Persisted policy, its accepted range, the schema identity and all derived capacities; every derived bound is declared in one place. |
| `core/crates/layerfs-storage/src/sqlite/cleanup.rs` | 150–260 | 58 | 77 | below | Below range. One indexed page loop and one pack deletion. |
| `core/crates/layerfs-storage/src/sqlite/connection.rs` | 90–150 | 50 | 72 | below | Below range. The pragma profile plus two small error mappers. |
| `core/crates/layerfs-storage/src/sqlite/lookup.rs` | 120–210 | 118 | 150 | below | Marginally below range. Three paged queries share the page builder and the row decoder. |
| `core/crates/layerfs-storage/src/sqlite/mod.rs` | 8–16 | 8 | 13 | within |  |
| `core/crates/layerfs-storage/src/sqlite/schema.rs` | 130–220 | 187 | 212 | within | Within range. Exact creation and validation of four tables, two indexes and the policy row. |
| `core/crates/layerfs-storage/src/sqlite/write.rs` | 180–320 | 83 | 117 | below | Below range. The statement bindings and the three transaction transitions. |

## 3a. Production LOC comparison for this commit

```text
Production LOC: 69208 -> 74501 (delta +5293)
  core      732 -> 6025   (layerfs-content 2322, layerfs-storage 2971, telemetry 732)
  reference 68476 -> 68476 (unchanged)
```

Method: `python3 tools/production_loc.py` (the counter version committed with
this change) run against both snapshots materialized from the exact trees — the
first-parent tree `2242e867b6685f33e2dd52f62eeece153664839f` and the committed
tree `6f30812efbe9ba86c749a48935890d11133930c4`. Nonblank, non-comment first-party
product implementation including candidate runtime SQL; tests, examples,
fixtures, tooling, docs, manifests and generated artifacts excluded. Reference and
core subtotals are reported separately because both products coexist during
migration.

The delta is entirely new candidate implementation. No reference code was
retired, moved or duplicated into the candidate: C1/C2 algorithms were ported and
simplified, and no path dependency, source include or runtime fallback into
`crates/` exists.

## 4. Commands and results

All commands below were run from `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`.

```sh
python3 core/tools/check_product_boundary.py          # PASS: 52 files scanned
python3 -m unittest discover -s core/tools -p 'test_*.py'          # PASS: 5 tests
python3 -m unittest discover -s tools -p 'test_production_loc.py'  # PASS: 13 tests
cargo +1.96.0 fmt --manifest-path core/Cargo.toml --all --check    # PASS
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --tests   # PASS: 45 tests
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --tests   # PASS: 37 tests
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked                  # PASS
cargo +1.96.0 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings  # PASS
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked --example timer_nested        # PASS
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked --example timer_composition   # PASS
tools/preflight.sh                                    # PASS: all steps; one soft budget note
```

`tools/preflight.sh` was run with the new counter tests and the three
`measure_components` modes wired in. Every step passed. The only non-passing line
is the documented soft budget: the reference workspace fast suite took 220 s
against its advisory 120 s warm-suite ceiling. That is a development-loop budget,
not a correctness gate, and the suite itself passed. No timeout was raised, no
test was dropped and no input was warmed to obtain this result. LayerFS has no
CI, so "preflight passed" is the strongest available claim and no CI-green claim
is made.

Real-mode smoke runs (fresh outputs, 16 KiB explicit fixture):

```sh
run="$(mktemp -d /tmp/layerfs-stage02.XXXXXX)"
python3 -c 'from pathlib import Path; import sys; Path(sys.argv[1]).write_bytes(bytes(range(256)) * 64)' "$run/input.bin"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_components -- --mode c1 --input "$run/input.bin" --timings "$run/c1.json"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_components -- --mode c2 --input "$run/input.bin" --store "$run/c2.sqlite" --timings "$run/c2.json"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_components -- --mode pipeline --input "$run/input.bin" --store "$run/pipeline.sqlite" --timings "$run/pipeline.json"
```

Observed root for the fixture in all three modes:
`3438ec94e9d7eddba104ff44e95b0c21d513f4066d3b0bcfa73152b47ca4c208`, logical
length `16384`. The pipeline mode verified byte-for-byte readback. These values
are **exploratory** single-sample wiring evidence, not a performance campaign and
not a speedup claim; the release qualification belongs to the later
qualification issue.

## 5. Correctness, ownership and single-attempt evidence

| Claim | Evidence |
| --- | --- |
| Canonical bytes/IDs match frozen profile fixtures | `object_identity.rs`: domain probe, small canonical hex, chunk identity, empty leaf, empty file state, CDC and mapping profile ids, all computed independently from the frozen formulas |
| Same constructor feeds both paths | `core_pipeline.rs::integrated_and_independent_runs_produce_the_same_roots` — identical root from integrated and C1-only runs, and the C2-only run stores the same supplied objects |
| Exact reuse, not ID trust | `cas_reuse.rs`: corruption of one stored pack byte is rejected as a collision; a truncated stored length is rejected without reading the record |
| Repeated identities | `cas_reuse.rs`: within one batch one row is inserted and the remaining occurrences are exact reuse; across batches likewise |
| Stable locators | `pack_locator.rs`: an existing locator is byte-identical after a later save starts a new pack; appends keep `record_number` ordinals |
| Backpressure and bounded ownership | `memory_bounds.rs` and `core_pipeline.rs` assert pending objects/bytes and the retained pack tail after every accept |
| No temporary payload file | `memory_bounds.rs` asserts the Store directory holds exactly the database file after a supported 131071-byte singleton save |
| One attempt, one cleanup boundary | `persistence_failure.rs`: unavailable ownership, missing dependency, a late input failure after an early acknowledged commit, an externally imposed constraint failure, and a removed Store |
| No retry/fallback/fsync | `grep -rn "fsync\|fdatasync\|sync_all\|sync_data" core/crates/*/src` returns nothing; no retry loop, busy handler or alternate path exists in the product source |

## 6. Honest gaps and limitations

| Item | Disposition |
| --- | --- |
| Lost `COMMIT` acknowledgement / unknown persistence outcome | **NOT INDUCED.** Reachable only through an engine or transport failure; inducing it would need a fault-injection switch, which product `src/` forbids. The product returns `StorageError::UnknownOutcome`, quarantines the save and never resends, polls or deletes. Recorded as a coverage gap, not a pass. |
| Cleanup-failure path (`CleanupFailed`) | Reachable and exercised only through an externally broken table; the ordinary definite-failure path is covered by the early-commit test. |
| Larger cutoffs (256 KiB, 1 MiB) | **NOT IMPLEMENTED.** Rejected as unsupported. A capacity-aware pack/record contract and a matching reader are required first; the 256 KiB compact pack ceiling cannot express a larger incompressible record. |
| DELTA chains, arbitrary edits, multi-edit finality | Later scope. `base_object_id` exists and is always `NULL`; no DELTA is produced or claimed. |
| Metadata pooling / `metadata_value_groups` | The table is shipped and empty. That is not a pooling implementation and is not claimed as one. |
| Grouped node acquisition on read | Range reads navigate the tree with one-ID batches (the provider contract's allowed point read) and batch only payload acquisition. Grouped node acquisition is a later change. |
| Memory evidence | Declared limits and live-ownership accounting only. Heap and RSS are **not** claimed: this slice has no profiler attribution. |
| Performance | One exploratory sample per mode. No matched comparator, no cache contract, no speedup claim, no registered performance campaign. |
| Filesystem trees, attributes, Workspace, FUSE, daemon, cloud | Not started; their issues stay open. |

## 7. Remaining Stage 3+ work

Filesystem tree and attribute construction, known-edit and multi-edit finality,
DELTA selection with chain bounds, metadata value pooling and pooled leaf
encoding, grouped node reads, larger accepted cutoffs with a capacity-aware pack
grammar, remote/daemon adapters under the same operation contract, and the
release qualification campaign for C1/C2. None of these is claimed here.

## 8. Amendment: publication watermark (post-review)

The review of Stages 1–2 (`stages-1-2-review-20260916T185553Z.md`, findings F1–F3)
showed that the visibility argument in §2 and §5 did not hold. Bounded write
transactions release the eager write lock between commits, so a save that
committed a pack, lost its lock or failed later, and then released ownership left
that pack **readable by unrelated readers** — the exact state the "one deliberate
deviation" claimed to prevent.

The fix is a durable publication watermark on the existing `store_policy` row:

| Item | Value |
| --- | --- |
| Schema identity | `application_id = 1279677261`, `user_version = 2` (v1 rejected, not migrated) |
| Column count | twenty: the nineteen above plus `store_policy.retained_pack_ceiling INTEGER NOT NULL CHECK (>= 0)` |
| Watermark meaning | highest `pack_id` belonging to a *completed* save; `0` when no save has completed |
| Advance point | only inside a save's final transaction, never in an early bounded commit |
| Ordinary read | clamps to the watermark; above it the read fails with `VisibilityCeiling` |
| Open check | `retained_pack_ceiling <= MAX(pack_id)`, otherwise `Integrity("publication watermark is ahead of storage")` |
| Acquire check | watermark must equal `MAX(pack_id)`, otherwise `UninspectedState { ceiling, highest_pack_id }` |
| Cleanup | deletes only rows above the failing save's baseline and never moves the watermark |

`sql/schema.sql` consequently grows from 44 to 55 physical lines and the
`store_policy` DDL from five to six columns. New external target
`crates/layerfs-storage/tests/visibility.rs` (5 tests) proves: an unrelated reader
cannot see an open save's early-committed packs and sees them after
acknowledgement; a definite failure after an early commit neither publishes nor
moves the watermark; a store with an unacknowledged baseline refuses to start a
new save; a watermark ahead of storage is rejected at open; and the watermark
survives reopen while still hiding a later open save.

Two further defects were reproduced by an independent harness outside this
repository while this evidence was being built. They are **not** fixed by the
watermark, they are recorded here rather than silently, and no part of this
report or the READMEs claims them as passing:

| Finding | State at this commit | Reproduction |
| --- | --- | --- |
| A read inside the save that accepted an object fails with `ObjectMissing` while that object's write group is still open, although `accept` acknowledged it | Open; outcome in §9 | `SaveOperation::read_batch` on a small-record workload (CDC-sized chunks leave a lane's last group partial) |
| A repeated canonical identity whose occurrences fall on opposite sides of a preparation wave fails the save with `UNIQUE constraint failed: objects.object_id` | Open; outcome in §9 | 2 MiB of one repeated byte: 66 objects, 3 distinct, 63 repeated occurrences; also at 4 MiB |

The second is the deduplication case the store exists for, so it is treated as
severity-high rather than as an edge case.

### 8a. Production LOC comparison for this amendment

Method: `python3 tools/production_loc.py --root <snapshot>` (counter
`09568dbd352a103ff097067a39060754d8c57330e9429d14a205475a45a80d59`), before =
commit `38d47cfd3` tree, after = the amendment tree.

| Scope | Before | After | Delta |
| --- | --- | --- | --- |
| `core` (C1 + C2 + telemetry) | 6039 | 6110 | +71 |
| `core/crates/layerfs-storage` | 2971 | 3042 | +71 |
| `core/crates/layerfs-content` | 2336 | 2336 | 0 |
| `core/crates/layerfs-telemetry` | 732 | 732 | 0 |
| reference `crates/` | 68476 | 68476 | 0 |
| combined | 74515 | 74586 | +71 |

The growth is the watermark itself: schema DDL, the two watermark accessors, the
three checks above, the `UninspectedState` variant, the `store.rs` clamp, and the
`finish_inner` advance. No test, documentation or example line is counted.
