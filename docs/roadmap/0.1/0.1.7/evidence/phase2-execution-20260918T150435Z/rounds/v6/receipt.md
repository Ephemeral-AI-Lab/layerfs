# V6 receipt — the ordinary-lane group-decode counter

> **Status:** Item receipt. Written once, from [`after/`](after/) (product commit
> `3e7b3db80`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../v5/after/`](../v5/after/), collected on `464807178`, this item's parent.
> **Terminal state: landed** — the counter exists, is charged where the
> decompression happens, is live on the frozen set, and the gate
> "decodes > distinct groups (before)" is measured on a frozen row.

## 1. The item

`P2-4`'s stated gate was "pack/decompress counters", and the counter did not
exist: `ReadCounters` (`cas/read.rs`) carries `objects`, `packs_read`, `pages`,
`ceiling`, `edges`, `max_depth`, `canonical_bytes`. The register's finding
(report-D O5) is that the ordinary resolver decompresses the same zstd group body
once per record served out of it - `k` records in one group cost `k` full
decompressions - and `packs_read` (one per pack per wave) and `objects` (one per
returned object) are both blind to it.

The instrument:

* `ChainCounters.group_decodes` - charged in `encoding/decode.rs`'s
  `GroupCodec::Zstandard` arm, **where the decompression happens**. Not at a
  caller reading the header: a caller would count a decompression it did not
  perform, and a cache placed in front of this call (`P2-4`) must not be charged
  for a body it served.
* `ReadCounters.group_decodes` - the wave's total, accumulated through the same
  `totals` path as `edges`/`max_depth`.
* `StoreReadCounters.group_decodes` and `StoreProvider::group_decodes()` - the
  per-wave figure and the operation's sum, the shape V3 gave connection opens.
* `measure_edits` prints `readback group decodes` beside
  `readback connection opens` in both readback paths.

Only the ordinary lane's *group body* is counted: every other lane decompresses a
per-record frame or copies a raw body, and the pooled reader owns its own cache
and its own charge.

## 2. The gate — decodes > distinct groups, on the frozen set

25 of the 35 measurement steps are bit-identical to the before arm; the 10 that
differ differ **only** by the added line:

```sh
python3 compare_arms.py rounds/v5/after rounds/v6/after
# steps compared: 35, differing: 10   (D15-D24, each only by `readback group decodes: N`)
```

| Row | `readback group decodes` | distinct ordinary groups **read** | verdict |
| --- | ---: | ---: | --- |
| D15–D20 (`edits.c2.*`) | 0 | 0 | the readback is one whole-file/singleton record: no ordinary group |
| **D21** `edits.pipeline.chunked` | **2** | **1** | decodes > groups ✔ |
| **D22** `edits.pipeline.small-to-large` | **2** | **1** | decodes > groups ✔ |
| **D23** `edits.pipeline.large-to-small` | **2** | **1** | decodes > groups ✔ |
| **D24** `edits.pipeline.batch` | **2** | **1** | decodes > groups ✔ |

The distinct-group column is the engine's own answer, read from the arm's own
store files (the edited file's records are the ones the readback traverses):

```sh
python3 - <<'PY'
import sqlite3, glob
names = {3:'ExtentLeaf',4:'ExtentBranch',5:'FileState',7:'DirectoryLeaf',8:'DirectoryBranch',
         9:'InodeBranch',10:'FilesystemRoot',11:'AttributeLeaf',12:'AttributeBranch',13:'Symlink'}
for path in sorted(glob.glob('after/output/D2*/edits-pipeline-*/store.sqlite')):
    rows = [r for r in sqlite3.connect(path).execute(
        "SELECT object_role, pack_id, group_number FROM objects")
        if r[0] in names]
    print(path, rows)
PY
# D21 …/D21/edits-pipeline-chunked: edited file = pack 4 ->
#   ExtentLeaf pack 4 group 0, FileState pack 4 group 0   => ONE group, TWO decodes
```

D21's edited file is pack 4: its `ExtentLeaf` and its `FileState` share **one**
group (`pack 4, group 0`), and the readback charged **two** decodes for it. D22
(pack 3) and D23/D24 (pack 4) have the same shape. `P2-4`'s claim is therefore
already anchored on the frozen set: the same rows must fall to **1**.

## 3. The item's tests

`core/crates/layerfs-storage/tests/group_decodes.rs` (new; the plan's §5 file,
which `P2-4` extends):

* `a_decode_is_charged_once_per_record_and_not_once_per_group` — on a chunked-file
  fixture, reads the file back through a real `StoreProvider` and asserts
  `group_decodes == ordinary records read`, `records > distinct groups` (read from
  the store's own `objects` table), and byte-identical output;
* `the_decode_charge_is_per_read_and_not_per_store` — a second provider over the
  same Store charges the same again (the charge is the read's, not a lifetime
  total), and one record read is exactly one decode on the per-wave figure.

They fail on the parent tree by not compiling, which is the honest form for an
instrument that adds an accessor:

```text
error[E0599]: no method named `group_decodes` found for struct `StoreProvider`
  --> crates/layerfs-storage/tests/group_decodes.rs:59:18   (and :64, :84)
```

## 4. The eight checks (exit codes)

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 | PASS, 120 files |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 | OK |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 | OK |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 | clean |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 | **460 passed, 0 failed** (458 + the two new) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 | clean |
| 7 | `python3 tools/production_loc.py --files` | 0 | 19,342 core |
| 8 | `git diff --check` | 0 | clean |

Logs: [`after/checks/`](after/checks/). Parity: 35/35 green and unchanged
(`git diff 464807178..3e7b3db80 -- '*tests*'` adds `group_decodes.rs` only).

## 5. Production LOC

**19,318 → 19,342 (delta +24).** `layerfs-storage` 6,235 → 6,259. The plan's
estimate was +15..30. The delta is one field on the chain counters, one charge,
the wave total, the public mapping, the provider sum and the two prints.

## 6. Architecture document (same commit)

`core/docs/architecture/10-counters.md`: `ChainCounters` and `StoreReadCounters`
gain `group_decodes` in their field tables, and the C2 counter section gains the
paragraph that says where it is charged, what it deliberately excludes (every
other lane; a cache hit) and why `packs_read`/`objects` cannot substitute. The pin
note records the addition as #178 **V6**.

## 7. Clean-tree reproduction

```sh
git archive 3e7b3db80 | tar -x -C /tmp/verify-v6
cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples   # exit 0
…/measure_edits --mode pipeline --case chunked --threshold-bytes 131072 --output /tmp/v6-clean
# save: inserted 3 reused 0 packs 2 prefix records 0 full records 3
# readback bytes: 262144
# readback connection opens: 1
# readback group decodes: 2
```

Identical to `after/logs/D21-edits-pipeline-chunked.log` on every counter.
Falsification answers: [`verify-v6.md`](verify-v6.md).
