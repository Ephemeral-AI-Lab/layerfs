# V7 receipt — the save connection's cache profile (spill half blocked)

> **Status:** Item receipt. Written once, from [`after/`](after/) (product commit
> `8f0fda297`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../v6/after/`](../v6/after/), collected on `3e7b3db80`, this item's parent.
> **Terminal state: blocked** — the profile half landed and is measured; the
> `SQLITE_DBSTATUS_CACHE_SPILL` half cannot be read from product code under this
> crate's audited-`unsafe` boundary, and the blocker is named in §4 with its
> evidence. Per the handoff's five states, a blocked item is a completed item when
> its receipt names the blocker and the evidence; it is **not** ticked as landed.

## 1. What was asked, and what the item could reach

`P0-2` recorded the limit: `MutationOwner` is private and `SaveOperation` exposed
no connection, so the spilling question could only be answered on a harness-owned
connection replaying the row shape. `P2-1`'s write-path claim is gated on that
observability.

The plan authorised either "a bounded accessor on the save (pragma values +
`SQLITE_DBSTATUS_CACHE_SPILL`)" or "a recorded acquisition profile". The second
form is what landed:

```rust
pub struct SaveConnectionProfile {
    pub page_size: i64,
    pub cache_size: i64,
    pub cache_spill: i64,   // SQLite's threshold encoding: 0 = off
    pub mmap_size: i64,
}

impl SaveOperation {
    pub fn connection_profile(&self) -> StorageResult<SaveConnectionProfile>
}
```

Owner: the save operation. Bound: four integers, fixed size, no allocation, no
retained state. Live multiplicity: read on demand, never stored. Lifetime: the
read call. Release: nothing to release. Every field is read back **from the
connection that performs the save**, never from the profile the store was
configured with, and no product path writes a pragma from it.

## 2. The measurement — the profile is readable, and it is a reading

| Row | before (`v6/after`) | after (`v7/after`) |
| --- | --- | --- |
| D28 `c2.ceiling` | no profile observable | `page_size 4096 cache_size 2000 cache_spill 20000 mmap_size 0` |
| D29 `c2.small` | no profile observable | the same four values |
| X3 (D28's determinism re-run) | — | the same four values |
| every other counter on D28/D29/X3 | — | **bit-identical** |
| the other 33 measurement steps | — | **bit-identical** |

```sh
python3 compare_arms.py rounds/v6/after rounds/v7/after --skip Y1
# steps skipped by name: Y1 (harness row added with the item)
# steps compared: 36, differing: 3 -> D28, D29, X3; each differs only by the added profile line
```

Three of 36 steps differ and each differs only by the added line: the instrument
changed no work. `cache_size 2000` and `cache_spill 20000` are the engine's own
answers for the *declared* profile (no `cache_size`, no `cache_spill` pragma), and
they are exactly what `P2-1` must move to `-32768` and `0` - this receipt is that
item's read-back surface.

The accessor is proven to read a connection rather than a literal by an external
case: an independent connection opened through the product's own open path reports
the same four values (`tests/connection_profile.rs`,
`the_profile_reading_agrees_with_the_store_itself`). Neither case pins a value
`P2-1` will legitimately change.

## 3. The live control for the quantity that is blocked

The spill counter itself is proven live in this round - on a connection the
harness owns, the only place it can be read (`Y1`, labelled diagnostic, never a
gate sample; P0-2's `spillcontrol` re-run):

```sh
…/phase0client c2 20000 diagnostic-small-cache
# c2 rows 20000 … cache_arm diagnostic-small-cache … commits 1
# | pragma cache_size 8 cache_spill 8 page_size 4096 journal_mode memory
# | dbstatus cache_spill ok=true value=15588  ← nonzero: the counter fires
```

20,000 rows in one transaction against a forced 8-page cache spill **15,588**
pages. P0-2's control on the same toolchain reported 1,032 at a different row
width, so the absolute figure is not portable across fixtures; what the round
re-establishes is that the observable is live, in this toolchain, at this
statically-linked SQLite.

## 4. The blocker, with its evidence

The gate asked for the spill counter **on the product's own save connection**. It
cannot be read there:

1. `rusqlite` 0.40.2 exposes no safe binding for `sqlite3_db_status` - the
   vendored source declares no `db_status` symbol at all
   (`grep -rn "db_status" ~/.cargo/registry/src/*/rusqlite-0.40.2/src/` → 0 hits).
2. This crate cannot call the FFI itself: `#![deny(unsafe_code)]`
   (`core/crates/layerfs-storage/src/lib.rs:28`), whose design note at `:20-27`
   states the rule - "`unsafe` is therefore denied crate-wide and allowed on
   exactly one audited module, `encoding::codec` … and the product boundary guard
   rejects `unsafe` anywhere else in this crate".
3. The guard enforces it mechanically:
   `core/tools/check_product_boundary.py:17-19` pins
   `UNSAFE_AUDITED_MODULE = {"layerfs-storage": "src/encoding/codec.rs"}`, and
   adding a second FFI site in `sqlite/connection.rs` fails check 1 with
   "unsafe outside the audited module boundary". The attempt was made and
   reverted; the field is absent rather than faked.

```sh
python3 core/tools/check_product_boundary.py     # exit 0, 120 files - with the FFI readers removed
```

**What would unblock it** (an owner decision, not taken here): a second audited
FFI module for one `sqlite3_db_status` call, or a rusqlite release that exposes
it safely. Until then `P2-1`'s **write-path** half can only be argued from the
harness-side control, and its receipt must say so; the read-path half is
unaffected. Note that `P0-2` already found **no spilling** at 1× or 4× the
transaction ceiling under the current profile, so the write-path half of `P2-1`
was measured-and-undercut before this instrument existed - the blocker removes
the product-side confirmation, not the finding.

## 5. The eight checks (exit codes)

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 | PASS, 120 files |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 | OK |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 | OK |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 | clean |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 | **462 passed, 0 failed** (460 + the two new) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 | clean (the first run caught an `unused_mut` in the new case; fixed, re-run) |
| 7 | `python3 tools/production_loc.py --files` | 0 | 19,361 core |
| 8 | `git diff --check` | 0 | clean |

Logs: [`after/checks/`](after/checks/) (`exits.tsv`). Parity 35/35 green and
unchanged (`git diff 3e7b3db80..8f0fda297 -- '*tests*'` extends
`connection_profile.rs` and touches nothing else).

## 6. Production LOC

**19,342 → 19,361 (delta +19).** `layerfs-storage` 6,259 → 6,278. The delta is one
`Pragma` variant, the profile struct, the accessor and its documentation.

## 7. Architecture document (same commit)

`core/docs/architecture/11-optimization-study.md` §16.6 (the connection-profile
comparison): core's `Pragma` set is stated as four read-only pragmas, the
accessor is named as #178 **V7**, and the study records that the spill counter is
read on a harness-owned connection and never on the product's, with the reason.

## 8. Clean-tree reproduction

```sh
git archive 8f0fda297 | tar -x -C /tmp/verify-v7      # + this round's client/src/main.rs
(cd …/client && cargo +1.85.1 build --release --offline --locked)   # exit 0
…/phase0client c2 8191 default
# … inserted 8191 … commits 31 statements 8191 …
# c2 save-connection profile page_size 4096 cache_size 2000 cache_spill 20000 mmap_size 0
…/phase0client c2 20000 diagnostic-small-cache
# … dbstatus cache_spill ok=true value=15588 …
```

Identical to `after/logs/D28` and `after/logs/Y1`. Falsification answers:
[`verify-v7.md`](verify-v7.md).
