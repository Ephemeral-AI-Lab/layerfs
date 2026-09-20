# Adversarial review of this campaign

> Status: research note. No subagents were launched; this is the coordinator's own
> adversarial pass, and it is written to be read against the report rather than
> instead of it.

## What was checked adversarially, and what it found

1. **Is the attribution measuring what it claims?** Instrument v1 charged the whole
   canonical row-resolution loop to one `rebuild_ns` field, which made it a
   container and produced a meaningless 4.689-second "rebuild". The defect was
   named, the instrument was split, and the v1 sample was kept but **excluded** from
   the matched pair. Instrument v2's disjoint set closes against the provider's own
   elapsed time to within 14,593 ns, which is why the split is believed.

2. **Is the treatment's mechanism real?** The prediction was arithmetic: one
   statement per distinct covering group. Measured: 65,337 on stride10 and 368,074
   on stride3, exactly the prediction. A treatment whose counts land on the
   prediction is not a coincidental timing win.

3. **Is the win a measurement artifact?** Both arms carry identical
   instrumentation; the operation timer is the harness's own; the instrumented
   provider interval, the catalogue counts and the pack counters are unchanged in
   kind between arms; and the Store interval moves in the same direction for the
   same reason (the save resolves pooled leaves through the same function).

4. **Could the gain be a cache effect rather than removed work?** The removed work
   is SQL statement executions, counted directly (278,927 → 65,337). Nothing was
   moved outside the operation, no input was pre-touched, no cache was enlarged,
   and the two arms read the same corpus with the same declared cache state.

5. **Is equivalence real, or just sampled?** Stronger than sampling: the saved
   Stores are **byte-identical** between arms in both selections, and equal to the
   retained L40 level-1 candidate Stores. Every read is also re-identified against
   the locator that named it, so a wrong value would have been an integrity error,
   not a quiet difference. The verification read-back remains a declared sample and
   is not described as exhaustive.

6. **Independent re-derivation.** `rederive.py` recomputes every headline figure,
   every binary hash and every custody hash from the raw receipts without importing
   `analyze.py`, `write_results.py` or `compare_stores.py`. Its first run **failed**
   and found a real convention error: the derived JSON summed operation and regions
   over states 2..N while `phases-perf.operation_ns` and the retained L40 table
   include the first state. The derived files are corrected, the superseded ones are
   retained and labelled in [SUPERSEDED-ANALYSIS.md](SUPERSEDED-ANALYSIS.md), and
   the error is recorded here rather than quietly fixed. Its second run has an empty
   failure list. A second finding from the same pass: the provider residual had been
   reported as 1,339,076,683 ns when the disjoint set actually leaves
   **1,305,940,532 ns**; the report now states the closing figure.

## Disagreements and rejected directions, preserved

- **The pre-registered candidate was not selected.** Priority A named a
  selected-group BLOB range read. The attribution does not reject it, but it does
  not select it either: 43.6% of the provider is pack acquisition and the existing
  pack cache already serves 67% of 436,068 demands, so the range route's removable
  share is not separable from the BLOB opens it adds without its own screen. That
  screen is still **NOT_RUN**.
- **Priority B is a real target and is still not implemented.** An
  operation-scoped pooled pack cache would remove roughly 2.1–3.9 seconds of pack
  acquisition, but it changes the lifetime of a cache whose window spans Store
  writes, so it needs an append-invalidation contract and corruption tests. It
  stays a **proposal**.
- **Priority C has no isolated measured effect.** The 9.87-second Store interval is
  not evidence that staging or copying costs that much; compression, indexing and
  transaction work are inside it and were not separated. It stays a **proposal**.
- **The first treatment shape was retained as a negative result, not deleted.** It
  won more on stride10 and lost on stride3; both samples and its identity are kept.
- **This campaign does not explain the historical gap.** The unmatched v0.1.6
  comparison is untouched, and nothing here is subtracted from it.

## Limits a reader should carry forward

Single samples do not establish repeatability. Admission is INELIGIBLE and every O3
pinned-counter row is INCOMPLETE. The diagnostic 120/240-second caps are not
ordinary 15/25-second admission rows. The 7,817 ns per catalogue statement is
measured but not decomposed into engine time and wrapper time, so the treatment's
residual 3,315,711,491 ns on stride3 is explained by statement count and not by
per-statement cost. The stride10 provider's 1,305,940,532 ns remainder is unnamed.
No new corruption, ceiling, cancellation or adversarial test was added, because the
change adds no new failure mode — but it also adds no new coverage.
