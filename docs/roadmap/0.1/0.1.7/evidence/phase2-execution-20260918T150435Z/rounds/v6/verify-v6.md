# verify-v6 — the ordinary-lane group-decode counter

> **author-verified** (single-agent Phase 2: no second reviewer, no subagent).
> Round: [`receipt.md`](receipt.md). Tree: `3e7b3db80`, arm [`after/`](after/);
> before arm [`../v5/after/`](../v5/after/) on `464807178`.

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 3e7b3db80 \| tar -x -C /tmp/verify-v6` | 0 | clean V6 tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples` | 0 | examples built |
| R3 | `measure_edits --mode pipeline --case chunked --threshold-bytes 131072 --output /tmp/v6-clean` | 0 | `readback group decodes: 2`, `readback connection opens: 1`, `save: inserted 3 … packs 2` — identical to `after/logs/D21` |
| R4 | `cargo test --test group_decodes` in the archive | 0 | `2 passed; 0 failed` |
| R5 | `python3 compare_arms.py rounds/v5/after rounds/v6/after` | 1 (by design) | `steps compared: 35, differing: 10` — D15–D24, each differing only by the added line |
| R6 | the store census over `after/output/D21…D24/…/store.sqlite` | 0 | each edited file's `ExtentLeaf` and `FileState` share one `(pack_id, group_number)` ✔ |

## 2. Falsification answers

**2a — does the new test fail on the parent tree?** Yes, by not compiling: the
parent has no `StoreProvider::group_decodes`. Copied into the `464807178` archive:

```text
error[E0599]: no method named `group_decodes` found for struct `StoreProvider`
  --> crates/layerfs-storage/tests/group_decodes.rs:59:18   (and :64, :84)
```

**2b — did the counter move in the predicted direction and magnitude?** The
instrument's prediction is that the defect is visible: **2 decodes for 1 distinct
group** on four frozen rows (D21–D24), 0 where the readback never touches an
ordinary group (D15–D20). Nothing else moved: 25 of 35 steps are bit-identical,
and the 10 that differ carry only the added line. The magnitude is stated with its
denominator rather than as "decodes is nonzero": the distinct-group figure comes
from the store's own `objects` table, not from the counter being checked.

**2c — parity green and unchanged?** 35/35. `git diff 464807178..3e7b3db80 --
'*tests*'` adds `group_decodes.rs` and changes nothing else.

**2d — single-variable?** `git show --stat 3e7b3db80`: `encoding/decode.rs` (the
charge), `encoding/delta/read.rs` (the chain field and accumulation),
`cas/read.rs` (the wave total), `cas/store.rs` + `cas/provider.rs` (the public
surface), `examples/measure_edits.rs` (two prints), the new test, and
`10-counters.md`. One variable: the decode counter. No behaviour, format or bound
change.

**2e — the item's named risk (would the counter count the wrong thing)?** The
charge is at the decompression, so only an actual group-body decompression counts:
a raw (uncompressed) ordinary group charges nothing, and a cache in front of the
call (P2-4) cannot be charged for a body it served - which is what makes the
counter usable as P2-4's gate rather than merely correlated with it. The
per-wave/per-operation split is probed by the second test.

**2f — is elapsed a gate anywhere here?** No; the gate is the decode count and the
distinct-group census.

## 3. UNVERIFIED

* **The distinct-group column covers the edited file only by inspection.** The
  census lists every object in the store; that the readback traverses the edited
  file's records (pack 4 for D21, pack 3 for D22, pack 4 for D23/D24) is read from
  the printed `save:` line and the root the vehicle reads back, not from a
  per-record trace. The claim does not depend on it: even counting every ordinary
  group in the store, D21/D23/D24 hold 2 groups against 2 decodes for the edited
  file's 1 group, and the test asserts the strict inequality on a constructed
  fixture where the group membership is read from the table.
* **`measure_components.rs` does not print the new counter.** The storage example
  that would show it per wave is not part of the frozen set; the `edits` rows carry
  the operation-level figure through `StoreProvider`.
* **No test pins that a raw (uncompressed) ordinary group charges zero.** The
  statement is read from the code path (`GroupCodec::Raw` has no charge); no
  fixture in the frozen set produces a raw ordinary group.
