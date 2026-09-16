# Evidence: Stages 1-2 remediation (publication watermark)

Produced while fixing finding F1 of
[`stages-1-2-review-20260916T185553Z.md`](../../../component-decoupling/stages-1-2-review-20260916T185553Z.md).

| File | Contents |
| --- | --- |
| `watermark-differential.txt` | One harness, one 5 MiB workload, two trees: an unrelated reader versus an open save before and after the watermark |
| `harness-watermark.rs` | Copy of that harness (it lives outside the repository, in `/tmp/rev-harness/src/bin/`) |
| `checks.txt` | Test, clippy `-D warnings`, formatting and production-LOC output on the remediated tree |

The report amendment that consumes this evidence is section 8 of
[`stages-0-2-report.md`](../../../component-decoupling/stages-0-2-report.md).
