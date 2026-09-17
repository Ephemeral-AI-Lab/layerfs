# Final tip verification (2026-09-17)

A fresh round confirming the state the closeout report describes, run after the last
documentation-only commit. Nothing in this round is a performance claim: the profile
is the test profile, one sample per command, and every command is a correctness,
boundary, formatting or accounting check.

| Command | Exit | Wall |
| --- | --- | --- |
| rustfmt, product workspace | 0 | 0.53 s |
| clippy, all targets, `-D warnings` | 0 | 0.32 s |
| workspace suite, 43 targets / 273 tests | 0 | 97.72 s (declared verification run) |
| product boundary | 0 | 0.09 s |
| core tool suite | 0 | 0.16 s |
| root tool suite (production LOC counter) | 0 | 0.15 s |
| production LOC at the tip | 0 | 5.83 s |
| `git diff --check` | 0 | 0.11 s |

Production LOC at the tip: core **11 058** in 75 files (C1 4 463, C2 5 863,
telemetry 732), reference **65 417** in 193 files by the corrected counter,
combined **76 475**. The per-commit first-parent comparisons are in each commit
message; the report's §2.1 records the review's finding-by-finding closure.

What this round does not prove: any latency, throughput, storage-saving or memory
superiority claim. Those need the matched campaign that the owner decision E1
governs, and the report's rows G13 and G15 stay open until it is answered.
