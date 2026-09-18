# P2-0 receipt — the `cas/owner.rs` responsibility split

> **Status:** Item receipt. Written once, from [`after/`](after/) (commit
> `ed5ab5d95`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`before/`](before/), collected on `0a1d74742`, whose product tree is
> `502f2aae1`'s (the only difference between them is documentation).
> **Terminal state: landed with a correction** — the split landed exactly as
> ruling 1 authorised; one line of the item's stated gate ("no test file
> changed") is refuted by a source-locator guard the plan did not see, and the
> deviation is named in §5 rather than hidden.

## 1. The item

`cas/owner.rs` was **962 of the 999 physical lines** the product-file ceiling
allows, and both `P2-2` (the per-row insert loop) and `P2-5` (the per-trial
reader) must be written in it. The plan's §2.1 split, authorised by ruling 1, is
a pure relocation by responsibility:

| file | before | after | responsibility |
| --- | ---: | ---: | --- |
| `cas/owner.rs` | 962 | **138** | the state struct, `OutcomeCounters`, the demand path (`note_reuse`, `read_batch`, `resolve_location`) |
| `cas/pool_lane.rs` | — | **365** | pooled metadata lane: ordinals, value groups, index sync, pooled bases |
| `cas/placement.rs` | — | **179** | framing, lane placement, group sealing, open tails |
| `cas/lifecycle.rs` | — | **232** | acquisition, commit cadence, acknowledgement, terminal disposition |
| `cas/selection.rs` | — | **137** | representation selection and its counters |
| `cas/mod.rs` | 18 | 23 | four `mod` lines, re-export split |

Mechanics: `impl MutationOwner` blocks in sibling files are legal (inherent impls
must share a crate, not a module) and only `lib.rs`/`mod.rs` are barred from
`impl` by the boundary checker. Every field and the two cross-module private
methods (`select_pooled`, `write_pack`) became `pub(super)`, which from
`owner.rs` is `pub(in crate::cas)` — visible to every caller, and not a public-API
change because `mod owner;` is private. `cas/mod.rs` re-exports
`OutcomeCounters` from `owner` and `PoolCounters` from `pool_lane`; the two
`crate::cas::owner::PoolCounters` paths in `store.rs` follow.

**Production LOC: 19,264 → 19,314 (delta +50)**, core files 116 → 120, within the
plan's +25..60 estimate. `python3 tools/production_loc.py --detail` is in
[`after/checks/7-production-loc.log`](after/checks/7-production-loc.log); the
delta is import lines in four new files plus `pub(super)` markers, and this
commit is labelled **relocation**.

## 2. Gate 1 — every counter bit-identical on the frozen set

Both arms ran the whole frozen set with the same driver, one worker, one sample
per case, a fresh `--output`, and a probe client **rebuilt per arm** (its sha256 is
in each arm's `artifacts.txt`). 37 steps per arm: 35 measurement steps plus the
two build steps.

```sh
python3 compare_arms.py rounds/p2-0/before rounds/p2-0/after
# steps compared: 35, differing: 0     (exit 0)
```

The comparator strips every timing field (`elapsed_ns`, `wall_s`,
`wall_seconds`, phase and timing-tree durations) and each arm's own output path,
because `elapsed_ns` is diagnostic-grade (`CONTRACT.md` §2.4). The build steps
`B1`/`B2` are excluded by name: their logs record what cargo compiled, which
legitimately differs between a cold and a warm target directory; the arms'
artifact identity is `artifacts.txt`, not those logs.

**The comparator is proven live by a control**, so "0 differing" is a measurement
and not a silent pass: a copy of the before arm with one counter perturbed
(`D26` `rows_read 25809` → `25810`) is reported as exactly one differing step.

```sh
cp -R rounds/p2-0/before /tmp/p2-0-control && sed -i '' 's/rows_read 25809/rows_read 25810/' \
  /tmp/p2-0-control/logs/D26-order-forced-64.log
python3 compare_arms.py /tmp/p2-0-control rounds/p2-0/after
# steps compared: 35, differing: 1 -> DIFF D26-order-forced-64.log   (exit 1)
```

## 3. Gate 2 — the move is a move (`move_check.py`)

[`move_check.py`](move_check.py) compares every non-blank line of the parent
`cas/owner.rs` with the after tree's five `cas/*.rs`, as a multiset, after
stripping the one marker the split is allowed to add (`pub(super) `):

```sh
python3 move_check.py /tmp/p2-0-before-owner.rs core/crates/layerfs-storage/src/cas
# parent non-blank lines: 923 / after: 1003 (delta +80)
# parent lines lost: 7   (4 rewritten module-doc lines + 3 imports split per file)
# after lines not in parent: 87 -> module doc 39, import 40, impl opening 4, brace 4, OTHER 0
```

Zero code lines are lost and zero are added: every added line classifies as a
module header, an import, an `impl MutationOwner {` opening or its closing brace.
This is the "LOC delta = imports + visibility markers only" gate, measured.

## 4. Gate 3 — the eight checks and the parity set

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 | `PASS: scanned 120 production Rust/SQL files` (116 + 4) |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 | OK |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 | OK |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 | clean |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 | **456 passed, 0 failed** |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 | clean |
| 7 | `python3 tools/production_loc.py --files` | 0 | 19,314 core |
| 8 | `git diff --check` | 0 | clean |

Logs: [`after/checks/`](after/checks/) (`exits.tsv` plus one log per check). The
34-test sealed-oracle parity set is green: `fixture_seal` 2,
`filesystem_reference` 2, `edit_reference` **3** (34 + the third pinned test the
owner accepted at ruling 8), `object_identity` 11, `filesystem_codec` 9,
`filesystem_updates` 6, `filesystem_profile` 2 — **35 passed, 0 failed**.

## 5. The correction — one line of a source-locator guard

P2-0's stated gate is "**no test file changed**", on the argument that a pure move
cannot need one. The plan's §2.1 coupling check did not include
`core/crates/layerfs-storage/tests/visibility.rs`, which contains a
**source-locator guard**:

```rust
fn the_pooled_lane_supplies_the_owners_ceiling_at_every_read_site() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cas/owner.rs"),
    ).expect("owner source");
    let selection = source.find("fn select_pooled(").expect("pooled selection");
    let synchronization = source.find("fn sync_pool_index(").expect("index synchronization");
    let outcomes = source.find("/// Pooled lane outcomes of this operation.").expect("pooled outcomes");
    // both regions: no `i64::MAX`, and `self.ceiling` present
}
```

It reads `src/cas/owner.rs` **by path** and scans the pooled lane's read sites for
an unbounded ceiling. The split moved exactly those three anchors into
`cas/pool_lane.rs`, so the guard failed:

```text
---- the_pooled_lane_supplies_the_owners_ceiling_at_every_read_site stdout ----
panicked at crates/layerfs-storage/tests/visibility.rs:502:54: pooled selection
test result: FAILED. 7 passed; 1 failed
```

The commit changes **one line** — the path the guard reads — and nothing else:

```diff
-        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cas/owner.rs"),
+        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cas/pool_lane.rs"),
```

Why this and not the alternatives: leaving the guard pointing at `owner.rs` would
fail the suite; keeping the pooled-lane methods in `owner.rs` to satisfy the guard
would both disobey ruling 1 and **destroy the guard's coverage**, since the sites
it exists to police (`select_pooled`, `sync_pool_index`, `pool_base`) would then
sit in `pool_lane.rs` unchecked. The three anchors and both assertions are
unchanged, so nothing about the expectation moved — only the file the expectation
is read from. It is reported here as a deviation from the item's stated gate, for
the owner to rule on if the reading should be stricter.

## 6. Clean-tree reproduction

```sh
git archive ed5ab5d95 | tar -x -C /tmp/verify-p2-0
cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples   # exit 0
(cd docs/roadmap/0.1/0.1.7/evidence/phase2-execution-20260918T150435Z/client && cargo +1.85.1 build --release --offline --locked)  # exit 0
…/phase0client order 4000 2000 64
# order pairs 2000 pending 64 spilled 3968 elapsed_ns <t> rows_read 25809 rows_written 25760
# runs_created 124 merges 61 … dir_pages_read 17 ino_pages_read 81 read_waves 5 objects_read 98
…/phase0client c2 8191 default
# inserted 8191 packs_created 33 pack_appends 8169 commits 31 pool_groups 8191
```

Every field matches `after/logs/D26-order-forced-64.log` and
`after/logs/D28-c2-ceiling-default.log`; only `elapsed_ns` differs, as declared.
The full record, with the falsification answers, is
[`verify-p2-0.md`](verify-p2-0.md).

## 7. Architecture documents (same commit)

`core/AGENTS.md` requires the affected documents to move with the code:

* `architecture/README.md` — the C2 file inventory gains the four modules.
* `architecture/10-counters.md` — `PoolCounters`' home is `cas/pool_lane.rs`, and
  the pin note records the move as #178 **P2-0**.
* `architecture/13-physical-writing.md` — the seal decision's home is
  `cas/selection.rs`.
* `architecture/11-optimization-study.md` — the two `cas/owner.rs` citations for
  code that moved (the seal decision, the per-row insert loop) follow it.

## 8. What this leaves for the items that need it

`cas/owner.rs` has 861 lines of headroom; `cas/placement.rs` (P2-2's landing
file) has 820 and `cas/pool_lane.rs` (P2-5's) has 634. The counters the later
items gate on are untouched: every Phase 1 counter reads exactly as it did before
the split.
