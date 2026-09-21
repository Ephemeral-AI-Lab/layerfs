# Pre-registration — #219 round 2: the remainder, after the write path was fixed

Written 2026-09-21T06:37Z, **before** any run of this round. Control arm for everything below is
**T1c** (`ns19-T1c-final-20260921T061849Z`, commit `a5c54df16`): the same worktree, the same
instrument, `operation_ns` 2776.3 ms, `accept_span_ns` 2772.5 ms, 13/13 gates, 14/14 pinned
counters. Its numbers are quoted from that receipt, not re-measured.

## The correction this round starts from

Round 1 predicted that batching a wave's seals into one transaction would dedupe page writes ~30x
and save 400-600 ms. **That prediction is wrong and is withdrawn here, before the run that would
have tested it.** The error: it assumed the appends sharing a transaction re-dirty the *same*
pages. They do not. A pack is append-only and grows, so append *k* writes bodies at offsets append
*k-1* never touched; the only page a run of appends shares is the control page (one of ~2 WholeFile
/ ~11 Native pages per append). Measured on T1c: 682.5 ms `commit_ns` over 17,378 commits is
39.3 us/commit, and the page work in it is ~4.9 body pages plus one control page per append, which
batching can dedupe by at most ~8 %.

What batching *can* remove is the **fixed** per-transaction cost, and that is what the round-1
instrument separates: `BEGIN IMMEDIATE` is **209.1 ms over 17,378 calls (12.0 us each)**. A wave is
~43 objects and ~30 seals, so a wave-scoped transaction would issue ~590 `BEGIN IMMEDIATE` instead
of 17,378.

## D2 — the finish-span probe (DIAGNOSTIC, not a treatment)

`pipeline.span_finish_ns` is **257.5 ms** on T1c while `finish_inner` is **1.2 ms** and the final
`flush_batch` drain is bounded above by `flush_batch_ns - accept_plumbing_ns` = **0.8 ms**. So
~255 ms of the finish span is in neither the save's own finish nor its drain, and nothing measured
says where it is. D2 charges the three candidates by name: the drain, the drop of the save's owner
(connection, codec workspaces, retained pack tails, the candidate and pool index clones), and the
residue of the call itself.

- Predicted: the owner drop is >= 200 ms of the 257.5 ms.
- Refuted if: the three named parts total < 150 ms of the 257.5 ms, or if the row stops being PASS.
- Diagnostic. It moves no pinned counter and no root digest, and it is reported as a diagnostic.

## T4 — two query-shape fixes (THE TREATMENT of this round)

One difference: **the two per-row/per-wave query shapes in the save's read path stop being
row-shaped.** Two parts, both format-neutral and both inside one transaction's semantics:

1. `validate_candidates` issues **one `SELECT` per row** (25,245 for 16,802 seals) where
   `lookup::candidates` already pages a list of identifiers. It becomes one paged call per seal.
   The predicate, the comparison order per identity and the failure are unchanged.
2. `lookup::candidates`/`present` build **variable-width SQL** (`IN (?1,...,?k)`) per page, so the
   prepared-statement cache holds one entry per page width and thrashes across the 587 waves.
   Pages become a fixed width with a sentinel identifier that cannot match, so the statement text
   is constant.

Expected movement in the instrument's own units:

| instrument | T1c | expected after T4 |
| --- | ---: | ---: |
| `diag_collision_query_ns` | 81.4 ms | <= 60 ms (25,245 queries -> 16,802) |
| `diag_validate_ns` | 123.8 ms | <= 105 ms |
| `diag_wave_ns` | 114.8 ms | <= 60 ms (587 x 2 prepares become cache hits) |
| `operation_ns` | 2776.3 ms | 2650-2700 ms |
| CPU | 2533.8 ms | a comparable fall |

Must NOT move: `digest:filesystem_root` `1d6fba29…` and all 14 pinned counters, of which
`pipeline.presence_queries` (398), `pipeline.commits` (17378) and `pipeline.inserted` (25245) are
the ones a query-shape change could plausibly disturb. **If `presence_queries` moves at all, the
change is a behavior change and is reported as one.**

Refuted if: any pinned counter or the root digest moves; or `operation_ns` moves by less than the
sum of the two measured query terms; or the row is not PASS.

The arm carries D2 as well. D2 is three `Instant` pairs on the finish path - below this
instrument's resolution, and declared rather than assumed.

## Method

One sample per case per arm, fresh `--out`, never an existing path, single thread, no re-run to
make a number look right. The control is T1c, measured on the same tree with the same instrument.
Nothing is retuned, relabelled or promoted from round 1.
