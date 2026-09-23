# #237 prospective C6: trim only a mostly empty final pooled pack

> **Status: Research; informative and not a product contract.** Frozen
> after the one retained C5 sparse PASS and dense space FAIL, before a C6
> product edit or sample. Do not repeat or relabel C3/C5.

## Evidence and one policy difference

C5 shortened the final pooled row at the end of **every** Save. In the
matched 17-Save release probe it cut allocated Store bytes from C3's
4,608,000 to 483,328 B with complete reopened object readback. In the
separate dense 100k SDK release row it rewrote only a final pooled pack
with **133,675 B used / 128,469 B tail** and raised combined allocated
Store + History from C3's 523,399,168 to 530,206,720 B, above the
frozen +1,000,000-B allowance. The [C5 result](pack-space-c5-result-20260924.md)
retains both readings and their cache limitations.

Keep C5's exact-closed payload packs, cross-flush pooled appends, and
final transaction/ownership checks. Change only the final pooled
shrink decision: issue the bounded SQL rewrite **when the final pooled
pack's declared used length is at most half of the 262,144-B pack limit
(≤131,072 B)**. Above half, retain its ordinary appendable row length
and do no SQL UPDATE. This is a generic occupancy rule, not a scenario
or file-count branch. It avoids rewriting a mostly used row for less than
half a pack of recoverable tail. It leaves the group bytes, IDs, reader
grammar, page size, publication and definite/unknown failure paths
unchanged.

Read-only examination of the retained sparse-history Store's 157 pooled
rows found **155** with used length ≤131,072 B; those rows account for
**36,388,028 B of its 36,581,495-B pooled tail**. That is a source-backed
coverage projection, **not** a C6 history-space result. All 17 rows in
the committed sparse probe are below half, while the retained dense
C3/C5 final pooled row is above half. SQLite free-page reuse and physical
allocation remain measured outcomes, not inferred passes.

## Frozen one-shot checks

Update the external pooled test to cover both branches: 17 successive
sparse Saves whose finalized v12 BLOB lengths equal declared used with
reopened exact readback and bounded closed file size, and one
more-than-half-full pooled pack that retains its 256-KiB row and reads
back every accepted object. Update physical-writing architecture in
the product commit. Run the owning Core tests, examples, formatting,
boundary checks and warning-denying product Clippy; retain the known
shared benchmark-example `clippy::format_collect` miss until both
release probe arms are sealed, then fix that example without changing
their historical receipts.

Take **one** C6 run of the unchanged, committed 17-Save release example
at fresh output
`benchmark-results/issue237-sparse-c6-17save-20260924-01`, using the
same `/usr/bin/time -l` command form and 4-KiB default policy as the
retained C3/C5 sparse probes. Full reopened 17-object byte/ID
verification must pass. Against the retained C3 control, C6 allocated
Store must fall by **at least 3,000,000 B**; report pages, freelist,
all pack lengths/used, command time, public Save timers and RSS. Then
take **one** Core SDK release 100k call at fresh output
`benchmark-results/fs-bench-pro/sdk-100k-packspace-c6-20260924-01`,
same SHAKE manifest, runner seal and zero-resident payload-page check
as C3/C5. Its separate full 101,001-path oracle must pass and combined
Store + History allocation must be at most **1,000,000 B above C3's
523,399,168 B**. Keep the 15-s complete-command and under-10-s verifier
expectations visible; report any miss without resampling.

The small sparse probe remains a mechanism diagnostic. It cannot close
the full #229 `history-stride10` or `history-stride1` gate: their
unchanged command budgets and complete-path verifier are unresolved.
Dense metadata cache residency also remains unqualified.
