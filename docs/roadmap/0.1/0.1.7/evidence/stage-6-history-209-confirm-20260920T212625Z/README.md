# #209 — confirmation window: the kept treatment reproduces on `commit_ns`

> Status: Research; **diagnostic evidence, not release admission**. Round of
> 2026-09-21. **No product line changed**: this window re-measures the treatment that is
> kept (`704580673`) in a fresh window, on a tree that is byte-identical to it over
> `core/crates` and `crates`. Admission `INELIGIBLE`, every budget class `NOT_RUN`.

## Why a confirmation window exists

The kept change is *a step commits the policy state it changed, not the policy state it
re-asserted*. It was measured once, on 2026-09-20, and the machine's level moved by more
than the effect between that window and the next (`commit_ns` 1.81 s → 2.49 s for identical
work; stride10 16.5 s → 22.5 s). A single window's contrast is therefore not enough to say
the change works. This window re-runs it.

**No source was edited to run this.** The two binaries are the already-archived ones:

- `…/stage-6-history-209-commit-20260920T194819Z/binary-archive/lever-pair`, sha256
  `106f181b31e70808cc224c8e00a6dbf9e2b1b232002818eb7025dd0dfe5cd36d` — the round's matched
  instrument, arm selected by `LAYERFS_STORAGE_POLICY_REASSERT`;
- `binary-archive/shipped-treatment` here, sha256
  `3a6c1c20397663c4fc2e0d89a3c2453ba26b39b1dbca0a1b7a5a5bce448a84ad` — which **the current
  tree rebuilds to byte for byte**, so the shipped default is the treatment and nothing
  else.

`collect.py` and `with_locks.py` here are byte-identical to the retained ones (`diff -q`
clean) and the harness source seal is `04bcfab573ab…`, as in every #209 row, so no harness
change can invalidate the pair. [`PREDECLARATION.md`](PREDECLARATION.md) was written before
the rows were sampled.

## The arm convention — the one thing to get right

`LAYERFS_STORAGE_POLICY_REASSERT=0` makes `reassert_policy_every_step()` return **false**,
which **is the treatment** (write the watermark only when it moved). **Unset is the
control** — the per-step re-assertion the treatment removed. This is the opposite of the
next round's lever, where `=0` *restored* the old behaviour, and reading it backwards is
easy: this window's first pass did, and was caught because the shipped row then landed
inside the "control" band. The tables below are checked against each receipt's
`extra_environment`.

## The five rows

| row | arm | operation | `commit_ns` | per append | `resolve_ns` | `filesystem` |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `confirm-1a` | control | 17.778 s | 2.058 s | 44.94 µs | 4.632 s | 4.620 s |
| `confirm-4a` | control | 17.706 s | 2.037 s | 44.48 µs | 4.642 s | 4.609 s |
| `confirm-2b` | treatment | 17.794 s | 1.952 s | 42.62 µs | 4.710 s | 4.716 s |
| `confirm-3b` | treatment | 17.772 s | 1.951 s | 42.61 µs | 4.708 s | 4.734 s |
| `shipped-confirm` | treatment (shipped binary) | **17.550 s** | **1.932 s** | **42.18 µs** | 4.665 s | 4.638 s |
| | **control mean** | 17.742 s | **2.047 s** | 44.71 µs | 4.637 s | 4.615 s |
| | **treatment mean** | 17.706 s | **1.945 s** | 42.52 µs | 4.694 s | 4.696 s |
| | **Δ (treatment − control)** | **−0.036 s (−0.20 %)** | **−0.103 s (−5.01 %)** | −2.19 µs | +0.057 s | +0.081 s |

**The arms do not overlap in `commit_ns`** (control 2.037–2.058 s against treatment
1.951–1.952 s), the shipped binary's own row is below both treatment-lever rows, and the
Store is byte-identical in all five rows
(`7ea2fe6ccf13bc5aee7ba59bddef2d60d61d50d6b90329ee1488b1f096228358`) with `commits` 48,446
everywhere.

## What this confirms, and what it does not

| contrast | kept round's window | this window |
| --- | ---: | ---: |
| `commit_ns`, treatment − control | **−0.111 s (−5.90 %)** | **−0.103 s (−5.01 %)** |
| stride10 operation, treatment − control | −0.237 s (−1.41 %) | −0.036 s (−0.20 %) |
| arms separate in `commit_ns` without overlap | yes (3 vs 4 rows) | yes (2 vs 3 rows) |

- **The bucket effect reproduces**, in sign and in size, in a window whose absolute level is
  8 % higher on the operation and 5 % higher on `commit_ns` than the round that kept it.
  That is what the treatment claimed: ~2.2 µs of a 44.7 µs per-append commit price.
- **The operation-level effect does not reproduce as a resolvable quantity.** It was
  −0.237 s in the keeping window and −0.036 s here, and the second window's own drift
  (first row to last: −0.228 s) is larger than either. **The change is kept for what it
  provably removes — 48,191 of 48,446 watermark statements and 45,794 ceiling statements,
  with a byte-identical Store — not for a claimed wall-clock number.**
- **The shipped binary is the treatment**: the tree rebuilds to the same sha256 that was
  measured, and its row is the fastest of the five on both terms. It is a confirmation, not
  an effect size — it is a different binary from the lever instrument.

## Custody

One sample per row, fresh `--output` each, both global flocks held for every resource
command, quiet preflight per `collect.py` (idle 74.9 % before the window, 78.4 % before the
shipped row); no deferrals were written. Two binaries are necessarily involved and both are
archived with their sha256; the ABBA contrast is from **one** binary. Every row is a
diagnostic: admission `INELIGIBLE`, every budget class `NOT_RUN`. Reproduce with
[`analyze.py`](analyze.py).
