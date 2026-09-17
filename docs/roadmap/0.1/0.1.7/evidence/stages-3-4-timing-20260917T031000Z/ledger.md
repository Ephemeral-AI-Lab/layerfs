# Stages 3–4 timing and counter receipts

Round label `stages-3-4-timing-20260917T031000Z`; collection started
2026-09-16T23:03:58Z on the host clock (the label and the clock disagree by the
host's timezone; both are recorded so nothing is inferred from the name).

Commit `dfd54fd8e8ee5f633f9ff80de2967c4c25b6f2c2`, subject
"tools: print the pooled policy and readback identities before collection".
Tool identities (sha256) are in `tool-identities.txt`; the declarations this round
follows are in `../../component-decoupling/stages-3-4-measurement-addendum.md`
(commit `c255dcfaf`), committed before the first receipt here existed.

Profile: debug (`cargo build`, no `--release`). One sample per case per arm. Fresh
`--output` per run, timing trees written with `create_new`. Cache state: the fixture
is built by the same process immediately before the timed scope and the Store is
created inside the run, so no warm-cache credit is claimed and the numbers are
exploratory. This is a wiring and correctness demonstration, not a benchmark and not
a release qualification; no v0.1.6 timing comparison is claimed.

## E1 pooled-metadata lane

Accepted policy row: cutoff 131,072 B, depths whole-file 8 / chunk 4 / metadata 8.

| arm | leaves | new values | reused values | full / delta leaves | trials | work-exceeded | retained entries | groups | wall s |
| --- | ------ | ---------- | ------------- | ------------------- | ------ | ------------- | ---------------- | ------ | ------ |
| e1a-pooled-24 | 24 | 123 | 2277 | 3 / 21 | 21 | 2 | 123 | 24 | 0.208 |
| e1b-pooled-128 | 128 | 227 | 12573 | 16 / 112 | 112 | 15 | 227 | 128 | 0.834 |
| e1c-pooled-512 | 512 | 611 | 50589 | 64 / 448 | 448 | 63 | 611 | 512 | 3.155 |

Readback in every pooled arm verified the first and the deepest leaf byte-for-byte
and printed both identities, which the equivalence arm compares.

The three arms show the same structure at three sizes: values are pooled (512 leaves
carry 51,200 rows in 611 distinct values, 98.8% reused), the physical lane stores
seven of eight leaves as a COPY/INSERT delta, and the chain restarts exactly where
the canonical chain budget is reached - `full leaves = work-exceeded + 1` in all
three arms (3 = 2 + 1, 16 = 15 + 1, 64 = 63 + 1), and 512 / 8 = 64 matches a
restart every eight records of 8,144 canonical bytes against the 65,536-byte budget.
The retained window holds one entry per distinct value (123, 227, 611) and is far
below its 131,072-entry bound, so no arm crossed the window; the window boundary,
the wholesale reset and the cold-start replay are covered by
`metadata_window.rs` instead.

Store files: 49,152 B for 24 leaves, 114,688 B for 128, 323,584 B for 512. The same
512 leaves would need 512 x 8,144 = 4,169,728 canonical bytes unframed, so the
pooled lane plus delta framing is what the physical size reflects.

## E2 edit lanes

15 runs (`--mode c1|c2|pipeline` x `--case small|chunked|small-to-large|large-to-small|batch`),
threshold 131,072 B. Every run exited 0, verified its readback and wrote its timing
tree. Wall times: 0.047 s
to 0.103 s.

Cross-lane agreement is the point of this table rather than the times: for the
`small` case the C1-only run and the integrated pipeline produce the same edited
root `8ddfe36cd5449f2b720590cb05da552290acaa5546ed065881c832703c650798`, the C2-only
lane reads back the same 65,559 canonical bytes, and the pipeline verifies its
readback byte-for-byte against an independent model. C1-only times no storage; C2-only
times no file construction; the integrated scope is edit-to-acknowledgement with the
base saved before the scope, and `verify.readback` is separate.

## E3 observability equivalence

The same bodies were run with `--timing off`. Every product line - policy, counters,
readback verification, both readback identities, retained footprint, catalogue rows,
result roots - is identical between the pairs; only the timing-tree lines, the
`timing:` line and the run's own output path differ:

- `e1a-pooled-24` (on) vs `e3a-pooled-24-off` (off): identical.
- `e3b-pipeline-chunked-on` (on) vs `e3c-pipeline-chunked-off` (off): identical.

## Budgets

Per-command wall time is measured in each `stdout.log` (`wall_seconds:`). The longest
arm is E1c at 3.155 s, inside the 15 s per-command budget; no
declared exception (25 s) was used and no arm was tuned to fit. No arm was skipped,
and no receipt was deleted or rewritten.

## Allocation ledger (as declared)

Product-reported live capacity from the E1 receipts: the pooled-value ordered set
retains 123 / 227 / 611 entries and 2,952 / 5,448 / 14,664 charged bytes
(`Store::pool_index_entries`, `Store::pool_index_bytes`). The externally accounted C1
live state is not re-measured here; it is cited from the committed external tests
`edit_bounds`, `edit_localized` and `memory_bounds` at the same commit. No lifetime
cgroup figure is used as a phase number.

## Not run

- No v0.1.6 timing comparison: the two products share no public edit surface to time,
  so the only reference claim is the sealed result oracle (see
  `stages-3-4-verification.md`).
- No release-profile arm: the addendum declares the debug profile for this round.
- No warm/cold contrast: the addendum declares no cache-state contrast, and a cold
  claim would need the cold contract's invalidation and residency check, which the
  in-process fixture cannot provide.

## Annotation added 2026-09-17 (W8.7) - the `e1c-pooled-512` row is telemetry-clipped

The `e1c-pooled-512` row above is quoted from a receipt whose timing tree is
incomplete: `stdout.log` prints `pooled.save 3.105s [incomplete]`, the tree carries
`"incomplete": true` on its root and on one `storage.accept`, and `timings:` reports
that detail was clipped rather than zero, leaving 57.5 % of that scope
unattributed. The receipt and its tree are retained exactly as produced and are not
re-labelled; this note is appended so that no reader of the row mistakes the
3.155 s wall time for a fully attributed measurement. The clipping is disclosed in
`stages-3-4-verification.md` §4 and was recorded as E-D7 by the review's own
evidence audit. Re-collection was not attempted: the arm is a wiring and
correctness demonstration, not performance evidence, and a single-sample re-run
would not change what it may be claimed for.
