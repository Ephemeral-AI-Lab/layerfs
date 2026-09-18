# P2-4 receipt — one decompression per distinct group

> **Status:** Item receipt. Written once, from [`after/`](after/) (product commit
> `3ce5f409e`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../p2-7/after/`](../p2-7/after/), collected on `b2abb6455`, this item's parent.
> **Terminal state: landed with a correction** — the cache landed and the gate
> moved exactly as V6 anchored it; the plan's *scope* for the cache (a wave) would
> have moved nothing, and the receipt records why the operation is the owner.

## 1. The item

`decode_canonical`'s ordinary arm decompressed the group body for every record
served out of it, so `k` records sharing one group cost `k` decompressions
(register T0-9; V6 measured 2 decodes for 1 distinct group on the pipeline
readbacks). The port is `PoolReader`'s discipline:

* `encoding::GroupCache` — decoded ordinary-lane bodies keyed by `(pack, group)`;
* owner: the **read operation** (the pooled `ReadSession`, which P1-2 introduced);
  bound: `DECODED_GROUP_CACHE_BYTES` = 512 KiB, a declared policy constant
  documented in the pooled cache's style (owner, bound, multiplicity, lifetime,
  release); live multiplicity: one copy per distinct group; lifetime: the
  operation; release: the whole cache when the next body would cross the bound,
  and with the session;
* a hit charges no `group_decodes`: the counter reports decompressions, not
  records served;
* the consult is in `decode_canonical`'s ordinary arm — where the decompression
  happens — and the cached body still passes the group-body length check.

## 2. The correction — why the cache belongs to the operation

The plan's shape table puts "a bounded decoded-group cache" in `encoding/decode.rs`
and `encoding/delta/read.rs`; the first implementation put it where the resolver
gets its other wave state (beside the pack cache) and **the gate did not move**:

```text
measure_edits --mode pipeline --case chunked      (cache per wave)
readback group decodes: 2                          ← unchanged
```

The reason is measurable rather than theoretical: the two records of D21's edited
file (`ExtentLeaf` and `FileState`, one group) are reached in **two different
waves**, and the provider reports one connection for the whole readback
(`readback connection opens: 1`), so the operation — not the wave — is the scope
whose reuse is real. The cache moved to `ReadSession`, and the counter did move
(§3). The wave-scoped attempt is not in the tree.

This is also what makes the plan's ordering rule bite: the ceiling is re-read for
every wave (`CONTRACT.md` §2.4's pooling rule), while the cache outlives a wave,
so a body decoded under an older ceiling could answer for a location the current
ceiling hides. §4 pins that.

## 3. The gate — decodes → distinct groups

| Row | counter | before | after | predicted |
| --- | --- | ---: | ---: | --- |
| **D21** `edits.pipeline.chunked` | `readback group decodes` | 2 | **1** | one per distinct group ✔ |
| **D22** `edits.pipeline.small-to-large` | `readback group decodes` | 2 | **1** | ✔ |
| **D23** `edits.pipeline.large-to-small` | `readback group decodes` | 2 | **1** | ✔ |
| **D24** `edits.pipeline.batch` | `readback group decodes` | 2 | **1** | ✔ |
| D21–D24 | every other counter (bytes, roots, connection opens, pages, objects) | — | **identical** | no other counter moves ✔ |
| the other 33 measurement steps | every counter | — | **bit-identical** | ✔ |

```sh
python3 compare_arms.py rounds/p2-7/after rounds/p2-4/after
# steps compared: 37, differing: 4 -> D21-D24, each only by `readback group decodes: 2 -> 1`
python3 pack_bytes_census.py rounds/p2-7/after rounds/p2-4/after
# 12 stores, identical (pack rows, pack bytes, object rows, pack-body sha256)
```

The distinct-group denominators are V6's census of the same rows (receipt §2
there): the edited file's `ExtentLeaf` and `FileState` share one
`(pack_id, group_number)`, so the expectation was exactly **1**, and 1 is what the
rows now read.

## 4. Ceiling before cache, pinned

`Resolver::decode_at` checks `location.pack_id > self.ceiling` and returns
`VisibilityCeiling` **before** the cache is consulted. `tests/visibility.rs` gains
`an_ordinary_group_above_the_ceiling_is_refused_before_the_decoded_cache`:

1. resolve the root under an unbounded ceiling with a cache — the group is decoded
   and retained (`cache.retained_bytes() > 0`);
2. resolve the same location, with the same cache, under a ceiling one below its
   pack;
3. require `VisibilityCeiling { pack_id, ceiling }`, **not** a served body.

The cached body is therefore never a way around a refusal. The case cannot exist on
the parent tree (the cache type did not exist), and it is the plan's named risk
probed directly rather than argued.

## 5. The item's tests

* `tests/group_decodes.rs` (V6's file, which the plan assigns to both items):
  `a_decode_is_charged_once_per_distinct_group` now pins
  `group_decodes == distinct groups < ordinary records`, and **fails on the parent
  tree** (`test result: FAILED`, the parent reads one decode per record);
  `the_decode_charge_is_per_read_and_not_per_store` still pins that a fresh
  provider over the same store charges its own decodes — the cache is the
  operation's, not the Store's;
* `tests/visibility.rs` (new case, §4).

## 6. The eight checks (exit codes)

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 | PASS, 120 files |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 | OK |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 | OK |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 | clean |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 | **466 passed, 0 failed** (465 + the visibility case) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 | clean |
| 7 | `python3 tools/production_loc.py --files` | 0 | 19,410 core |
| 8 | `git diff --check` | 0 | clean |

Logs: [`after/checks/`](after/checks/). Parity 35/35 green and unchanged.

## 7. Production LOC

**19,346 → 19,410 (delta +64).** `layerfs-storage` 6,285 → 6,349. The plan's
estimate was +50..100.

## 8. Architecture documents (same commit)

`core/docs/architecture/05-storage.md` §6.11 (read waves): the session now carries
one `GroupCache`, with its owner, bound, release and lifetime, and the ordering
rule (ceiling before cache) stated. `10-counters.md`: the `group_decodes` paragraph
records what the cache made the counter read (distinct groups; 2 → 1 on the
readbacks) and why it is not a visibility shortcut. `policy.rs` documents the new
constant in the pooled cache's style.

## 9. Clean-tree reproduction

```sh
git archive 3ce5f409e | tar -x -C /tmp/verify-p2-4
cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples
…/measure_edits --mode pipeline --case chunked --threshold-bytes 131072 --output /tmp/p2-4
# readback bytes: 262144 … readback group decodes: 1   ← was 2
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test group_decodes
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test visibility
```

Falsification answers: [`verify-p2-4.md`](verify-p2-4.md).
