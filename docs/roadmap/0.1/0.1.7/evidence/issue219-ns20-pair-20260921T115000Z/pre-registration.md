# Pre-registration — #219 round 20, the matched pair that the first row is not

Written **before** either arm of the pair is built or run.

## Why a pair, and what it is for

Round 20's first row (`benchmark-results/issue219/ns20-P1-packlimit-20260921T112200Z`, clean at
`c296194a1`) moved its declared figure by **-197,218,166 ns**, and the movement does not decompose the
way its pre-registration predicted. Two measured reasons, both read off that row and its control:

1. **the row's formula subtracts a term that grew.** `pipeline.operation_work_ns` is
   `accept_span_ns - finish_drop_ns` (`src/ops/pipeline.rs:1260-1263`), and `finish_drop_ns` is the
   save's owner being taken apart, 99.9 % of it `release_connection_ns` — closing the save's
   connection. That term went **81,532,666 -> 230,468,083 (+148,935,417)** while the inclusive
   `accept_span_ns` went **1,208,264,083 -> 1,159,981,334 (-48,282,749)**. Three quarters of the
   headline is work that crossed the row's own declared boundary, inside the same closure.
2. **the pair is not matched.** Pure-CPU work the change cannot reach also fell: `construct_ns`
   (untimed, before any Store exists) **440,144,955 -> 321,098,127 (-27.0 %)**, `construct_noise_ns`
   -25.7 %, and the in-region C1 tree build `span_build_ns` **89,652,833 -> 68,377,625 (-23.7 %)**,
   which builds a tree in memory and writes no pack.

So neither the 197.22 ms nor the 48.28 ms can be attributed to `PACK_LIMIT` from that row, and this
pair exists to settle it: **the same case, the same machine, the same session, two binaries that
differ only by the one commit under test, run back to back.**

## The arms

| arm | source | binary |
| --- | --- | --- |
| **A, control** | `b7a0ab0a2` — the handoff commit, whose product source is identical to the round-19 row's `e3a46d74b` | rebuilt from that tree |
| **B, treatment** | `codex/219-ns10000` at `88accb7d0`, product source `c296194a1` | rebuilt from that tree |

Both are built **in this worktree**, so the two builds share one `target/` and one dependency cache:
the difference between the executables is the one commit and its rebuild, not a dependency change.
Both trees are **clean checkouts at their own commit**, so each receipt's `source_commit` names the
source its binary was built from and `source_dirty` is false for both. One sample per arm, one run per
arm, `--verify full`, fresh `--out`, back to back, control first.

## The registered prediction

This is a **diagnostic pair**, not a new arm: it re-measures an existing treatment to give it a matched
control. Nothing here changes a product line, and no figure from it replaces the arm's gate sample.

**Registered:** the two rows reproduce the qualitative split that the first row showed —

| quantity | arm A predicted | arm B predicted |
| --- | ---: | ---: |
| `packs_created` | 1,270 | 295 |
| `space.pack_bodies_bytes` | 332,922,880 | 309,329,920 |
| `space.page_count` | 81,987 | 76,162 |
| `accept_span_ns` | ~1.21 s | **lower than arm A** |
| `finish_drop_ns` | ~81 ms | **materially higher than arm A** |
| `operation_work_ns` | ~1.13 s | **lower than arm A** |

and that the *pack-free* work in the two arms is now **equal within 5 %** — `construct_ns`,
`construct_noise_ns`, `span_build_ns` and `preparation_ns`. That last line is the check on the
confound: if the two arms agree on work `PACK_LIMIT` cannot touch, the pair is matched and the
remaining difference is the lever.

**Refuted** if any of: the pack-free work differs by more than 5 % between the arms (the pair is not
matched and this run says nothing); arm B's `finish_drop_ns` does not rise materially above arm A's (the
boundary reading was a property of the earlier session, not of the change); `packs_created` and
`pack_bodies_bytes` are not 295 and 309,329,920 in arm B (the shape finding does not reproduce); or
either arm fails a gate.

## What it will and will not settle

It settles whether the -149 ms boundary shift is the change's or the session's, and it gives the first
matched estimate of the lever's real size on the inclusive closure. It does **not** make the lever a
1 s claim: the inclusive closure is the honest boundary, and whatever the pair says about it is what
gets reported.
