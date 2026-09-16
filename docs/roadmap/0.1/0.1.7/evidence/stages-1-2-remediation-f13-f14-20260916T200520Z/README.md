# Evidence: duplicate identities across waves, and same-save reads of waiting records

Produced while fixing the two defects recorded in section 8 of
[`stages-0-2-report.md`](../../../component-decoupling/stages-0-2-report.md).

| File | Contents |
| --- | --- |
| `f13-f14-differential.txt` | One harness, two trees: the duplicate-identity failure, the same-save read failure, the new tests failing on the pre-fix tree, and the measurement that rejected the retain-first design |
| `harness-repro.rs` | Copy of the F13/F14 harness (it lives outside the repository) |
| `harness-compressible.rs` | Copy of the compressible-workload harness that measured the rejected design |
| `checks.txt` | Test, clippy `-D warnings`, formatting, production-LOC and physical-line output |
