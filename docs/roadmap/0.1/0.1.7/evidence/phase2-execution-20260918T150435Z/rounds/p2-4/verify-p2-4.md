# verify-p2-4 — one decompression per distinct group

> **author-verified** (single-agent Phase 2: no second reviewer, no subagent).
> Round: [`receipt.md`](receipt.md). Tree: `3ce5f409e`, arm [`after/`](after/);
> before arm [`../p2-7/after/`](../p2-7/after/) on `b2abb6455`.

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 3ce5f409e \| tar -x -C /tmp/verify-p2-4` | 0 | clean P2-4 tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples` | 0 | examples built |
| R3 | `measure_edits --mode pipeline --case chunked --threshold-bytes 131072 --output /tmp/p2-4` | 0 | `readback group decodes: 1` (was 2), `readback bytes: 262144`, `readback connection opens: 1` |
| R4 | `cargo test -p layerfs-storage --test group_decodes` | 0 | `2 passed; 0 failed` |
| R5 | `cargo test -p layerfs-storage --test visibility` | 0 | `9 passed; 0 failed` |
| R6 | `python3 compare_arms.py rounds/p2-7/after rounds/p2-4/after` | 1 (by design) | `steps compared: 37, differing: 4` — D21–D24, each only by the decode count |
| R7 | `python3 pack_bytes_census.py rounds/p2-7/after rounds/p2-4/after` | 0 | 12 stores, identical |

## 2. Falsification answers

**2a — does the new test fail on the parent tree?** Yes.
`a_decode_is_charged_once_per_distinct_group` compiles there (it uses V6's public
surface) and fails:

```text
failures:
    a_decode_is_charged_once_per_distinct_group
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1 filtered out
```

The visibility case cannot exist on the parent tree at all: `GroupCache` is new, so
it does not compile there. Both answers are in the receipt.

**2b — did the counter move in the predicted direction and magnitude?**
Predicted 2 → 1 (one decode per distinct group); measured 2 → 1 on all four
pipeline readbacks, with every other counter on those rows and all 33 other steps
bit-identical, and 12 stores byte-identical. The denominator is V6's own census of
the same rows, not a number chosen here.

**2c — parity green and unchanged?** 35/35 sealed-oracle tests. `git diff
b2abb6455..3ce5f409e -- '*tests*'` changes `group_decodes.rs` (V6's own file, which
the plan assigns to both items, with its expectation moved to the post-cache
reading) and adds one case to `visibility.rs`; no parity expectation moved.

**2d — single-variable?** `git show --stat 3ce5f409e`: `policy.rs` (the bound),
`encoding/decode.rs` (the cache and its consult), `encoding/delta/read.rs` (the
resolver's ceiling-before-cache check and the cache it carries), `cas/read.rs`
(the session's ownership), `cas/store.rs`/`cas/owner.rs`/`delta/select.rs` (the
call sites), the two test files and the two architecture papers. One variable:
whether a decoded group body is reused within an operation.

**2e — the item's named risk (ceiling before cache)?** Probed by the new
visibility case, which decodes under one ceiling and demands under a lower one with
the same cache; the refusal is `VisibilityCeiling`, and the cache is demonstrably
non-empty at that moment (`retained_bytes() > 0`).

**2f — is elapsed a gate anywhere here?** No; the gate is `group_decodes` and the
unchanged counters.

## 3. UNVERIFIED

* **The byte bound is not stressed.** No frozen row crosses
  `DECODED_GROUP_CACHE_BYTES` (512 KiB), so the wholesale release path is exercised
  by neither the receipts nor a test; it is ported from the pooled cache's
  discipline and documented, but not measured here.
* **The same-save read path gets no reuse.** `MutationOwner::read_batch` and
  `resolve_location` build a cache per call, so a same-save read of several records
  in one group still decompresses per call. That path's group reuse is not what the
  item claimed (the gate is the read operation), and no frozen row reaches it.
* **The cache is not shared with the save path's dependency reads**
  (`delta/select.rs`), which also build one per trial; sharing it would be a
  different item (P2-5 touches the pooled reader's reuse) and is not claimed.
