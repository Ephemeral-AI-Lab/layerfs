# Independent rejection of immediate parent-value insertion

> Status: Research. This finding concerns the first candidate, rejected before performance collection.

The initial implementation passed the first three structural/canonical tests and reduced the five-parent full-batch fixture from 15 to 6 authenticated object acquisitions. Nevertheless it changed which valid bounded operations complete: moving `note_value` earlier increased transient ordering storage requirements.

Independent parsing of the 56-cell baseline quota log and initial candidate log reproduced **eight baseline-accepted cells now refused**:

| Pending rows | Ordering bytes | Batch widths | Baseline | First candidate |
|---:|---:|---|---|---|
| 1 | 864 | 1, 64 | OK | limit 864, actual 960 |
| 1 | 960 | 1, 64 | OK | limit 960, actual 1056 |
| 2 | 960 | 1, 64 | OK | limit 960, actual 1248 |
| 2 | 1152 | 1, 64 | OK | limit 1152, actual 1248 |

Two additional cells (pending 1, bytes 768, both widths) were refused in both versions but changed actual demand from 864 to 960 bytes. All 56 lines can be recovered despite test stdout/stderr interleaving by stripping only the test-harness `test ... ok` token from one quota line; no measurement was rerun. Exact outcomes are retained in `initial-candidate-quota-comparison.json`.

These are real acceptance regressions, not evidence of broken canonical output. The first quota test merely logged allowed refusal classes, so its overall PASS did not establish baseline-equivalent capacity acceptance. The coordinator rejected the first implementation and requested a strengthened regression assertion and a second implementation preserving original value-registration order. No quota was enlarged, no product performance claim was accepted, and the failing candidate artifacts remain available.
