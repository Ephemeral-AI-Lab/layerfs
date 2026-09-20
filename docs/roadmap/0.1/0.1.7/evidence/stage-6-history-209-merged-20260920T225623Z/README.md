# #209 — stride1 on merged `main`: **97.730 s**

> Status: Research; **diagnostic evidence, not release admission**. Round of
> 2026-09-21, taken **after** PR #206 was merged to `main`. Admission
> `INELIGIBLE`, every budget class `NOT_RUN`.

## What was measured

One stride1 sample on the merged tree, one sample, fresh `--output`, both global
flocks, quiet preflight, `--pre-execute`.

| | |
| --- | ---: |
| **operation** | **97.730 s** |
| states | 157 |
| `commit_ns` | 4.104 s |
| `resolve_ns` | 36.690 s |
| `sql_ns` | 2.543 s |
| `full_ns` | 2.500 s |
| `delta_ns` | 1.044 s |
| `place_ns` | 0.372 s |
| `group_ns` | 0.120 s |
| `content` span | 5.157 s |
| `pack_appends` | 84,493 |
| saved Store | `d45975f25daee9d0…` |
| binary sha256 | `3a6c1c20397663c4fc2e0d89a3c2453ba26b39b1dbca0a1b7a5a5bce448a84ad` |
| merge commit | `2f07f1f37af3e06a92a00880c68882b3c91923ef` |
| window | load 3.99, idle 84.38 %, 2026-09-20T22:56:26Z |

`storage.accept_loop` and `filesystem` are **absent from this row by design**:
stride1 runs the ordinary recording, because the driver refuses per-state phase
nodes above 53 states, so those spans are not recorded as nodes and the state
totals carry their time. The operation is the sum of the 157 state nodes.

## What this row does not establish

- **It is not comparable to the retained stride1 rows.** Those rows (the #190
  pooled-reader round, the #205 save-split round) all carry **`pack_appends`
  84,473 and Store `1635cf7b…`**; this one carries **84,493 and `d45975f2…`**. The
  difference is consistent with the multi-writer model change — the same change
  that moved stride10's constant from `4af37932…` to `7ea2fe6c…`, which the #209
  rounds recorded as expected — but **that has not been verified for stride1**, and
  until it is, no stride1 figure from before the model change may be read as a
  trend against this one.
- **It carries no in-window control.** One sample, one arm, no stride10 companion
  row in the same window, so the 97.730 s has no same-window bar to sit against.
  The protocol's own gate does not predict the machine's level (L62 §8), so this
  number must not be compared with 105.726 s or 146.178 s from earlier windows.
- **The Store hash differs from `collect.py`'s retained `history-stride1`
  constant `1635cf7b…`**, which is the pre-multi-writer campaign constant; that is
  expected for this model and is the same situation the confirmation window
  recorded for stride10.

## Reproduce

```sh
bash run_stride1.sh
```
