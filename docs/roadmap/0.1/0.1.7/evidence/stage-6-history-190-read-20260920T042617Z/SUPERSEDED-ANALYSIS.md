# Superseded derived analysis files

> Status: research note; the raw receipts were never modified.

Two conventions appear in this campaign's **derived** JSON. Both are retained; only
one is authoritative.

| File | Convention | Status |
|---|---|---|
| `baseline2-instrumented-stride10.json`, `baseline2-instrumented-stride3.json` | operation and regions summed over states **2..N** | superseded |
| `candidate-stride10.json`, `candidate-stride3.json` | states 2..N | superseded |
| `candidate2-stride10.json`, `candidate2-stride3.json` | states 2..N | superseded |
| `baseline-stride10-analysis.json` | states 1..N | diagnostic, instrument v1 |
| `baseline2-stride10-analysis.json`, `baseline2-stride3-analysis.json` | states 1..N | **authoritative** |
| `candidate-stride10-analysis.json`, `candidate-stride3-analysis.json` | states 1..N | rejected treatment, authoritative for it |
| `candidate2-stride10-analysis.json`, `candidate2-stride3-analysis.json` | states 1..N | **authoritative** |

The correct convention is **all selected states, including the first**. It was
established by checking that `raw/phases-perf.json::operation_ns` equals the sum
over all states exactly in every one of the six performance runs; the retained L40
table is on the same convention. The superseded files excluded state 1, which made
their absolute operation and region figures about 43-48 ms too small and
incomparable with L40.

**Effect on the conclusions: none.** The headline reductions move by
4,543,959 ns (stride10) and 2,234,459 ns (stride3) — under 0.3% of each reduction
and under 5 ms in absolute terms — and both remain far above the one-second
criterion. The instrumented provider interval, the catalogue statement counts, the
catalogue interval, the pack counters, the Store bytes and every verification
figure are identical under both conventions, because state 1 has no pooled reads.

The error was found by `rederive.py`, which recomputes the headline arithmetic from
the raw receipts without importing the analysis scripts. The superseded files are
kept rather than deleted or rewritten.
