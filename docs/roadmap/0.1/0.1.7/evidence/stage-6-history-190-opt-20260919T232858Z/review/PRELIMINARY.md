# Independent preliminary review

> Status: Research. Read-only review of proposed design, external tests, observer and protocol; no candidate product code or candidate measurement was available at this review point.

The proposed change reuses the existing `lookup_many` API and preserves the immutable base root. Filtering existing live parents before lookup excludes newly allocated and unreachable parents. Checked input order makes sorting/deduplication unnecessary. Holding one bounded batch of base values through directory construction supplies the content root and omitted metadata without retaining an operation-sized record memo.

## Open correctness question

Moving `ReferenceReducer::note_value` into the directory loop interleaves parent values with binding observations. The row value and reference effects are independent, so this does not itself change final semantic counts. It does change pending-row insertion and spill order. `RunStore::spill` reserves live runs plus pending rows and merge output, so identical declared `ordering_bytes` does not prove identical capacity requirements. A differential baseline/candidate test at constrained ordering capacity is needed before asserting unchanged resource acceptance. Existing forced-spill coverage proves successful canonical output and cleanup, not this boundary. Product owner and coordinator have been notified; the concern is retained rather than treated as a demonstrated defect.

Existing validation still runs first and covers input ordering, parent kinds, declared-new identities and topology. Batched acquisition may change which error wins when several immutable objects fail; it must still fail once without publishing a root. The baseline has repeated lookup demand even for identical omitted/supplied metadata: retained baseline structural test shows 23/17/15 versus 18/12/10 object acquisitions at widths 1/3/64. This is acquisition work, not physical reads or elapsed time.

## Measurement review

`ReadWork` forwards the original ordinary or scoped method, unchanged IDs and provider result. Its counters include failed waves, sum requested objects, count successful returned canonical objects/bytes, and measure delegated elapsed time. Disabled observation does not read the clock. These counters do not measure physical disk bytes, SQLite queries, codec CPU, or unique objects. Provider elapsed overlaps filesystem/content spans and must not be added to them.

Coarse filesystem spans leave work outside named children: initial registration, final value overlay in the baseline, zero-count collection and released-descendant work. Keep these as an explicit filesystem residual. Moving final parent-value registration into directories changes that subphase's composition; compare complete filesystem time and complete provider work for the treatment.

The protocol prospectively freezes 120/240-second diagnostic commands, 60-second verification hard budget, one sample per arm/selection, identical instrumentation, explicit environment, source identities, a measurement lock and quiet-machine criteria. It correctly marks uncontrolled Store/OS residency INELIGIBLE for cold/performance admission. Such elapsed differences cannot establish a cold speedup or resolve the historical v0.1.6 comparison. A successful quiet precheck only describes the precheck; post-observations and contemporaneous activity remain relevant.

## Pending independent work

Review final product diff and architecture update; recompute trace and timer totals from retained baseline/candidate raw files; compare selection/root identities, all stated work counters, residuals, bounds, verification outcomes, harness/source hashes and production LOC. No independent benchmark process, test, build, product edit or harness edit was run by this reviewer.
