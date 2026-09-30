# Family7 package SDK r071 —six PASS,1025 failure,retained NOT_RUN

**Status: Dated benchmark checkpoint; diagnostic timings, numeric cache INELIGIBLE.**

Measured clean source `ab32123817dda4bb18be7323d94874e79b71c980`, tree `05462b9daf643d830ba737d691b49aa3935d8cb5`; prospective [eight-case selection](../FAMILY7-SELECTION-20260930.md). [Compact receipts](20260930-workspace-shell-package-sdk-r071-receipts.json) retain identities, exact commands, binary/image/fixture hashes, raw manifest hashes and every row. Prepared masters are independent closed byte copies; sole existing public SDK Init prepared package/large/repeated once, full baseline proof once. Locked release binaries, same daemon and prior image base reused; v2 package inputs added once. One construction worker and original quotas/limits. No production change. No earlier family repeated.

| Exact SDK case | End-to-end ns / bound | SDK Exec / Commit ns | Host Server | Daemon Exec / Commit ns¹ | Full verifier ns /9s | Correctness / command / cleanup | Numeric |
| --- | ---: | ---: | --- | ---: | ---: | --- | --- |
| workspace-shell-package-mixed-refresh-sdk-v2 | 1188465084 / 25000000000 | 174716083 / 155932833 | UNAVAILABLE: complete timer tree clipped/not published | 173017792/154760708 | 37586542 | PASS / PASS / PASS | INELIGIBLE |
| workspace-shell-package-overwrite-4k-sdk-v2 | 896207834 / 15000000000 | 18492959 / 18134292 | UNAVAILABLE: complete timer tree clipped/not published | 17451958/16886042 | 100846375 | PASS / PASS / PASS | INELIGIBLE |
| workspace-shell-package-repeated-one-byte-sdk-v2 | 906347708 / 15000000000 | 36592125 / 18024125 | UNAVAILABLE: complete timer tree clipped/not published | 35516458/17003208 | 38729500 | PASS / PASS / PASS | INELIGIBLE |
| workspace-shell-package-many-128-sdk-v2 | 2632776334 / 15000000000 | 724438541 / 1012325792 | UNAVAILABLE: complete timer tree clipped/not published | 723084209/1011075959 | 108020333 | PASS / PASS / PASS | INELIGIBLE |
| workspace-shell-package-many-129-sdk-v2 | 2526101625 / 15000000000 | 728778958 / 980102542 | UNAVAILABLE: complete timer tree clipped/not published | 726267084/977311834 | 108645417 | PASS / PASS / PASS | INELIGIBLE |
| workspace-shell-package-many-257-sdk-v2 | 5144135334 / 15000000000 | 2108573209 / 2145004208 | UNAVAILABLE: complete timer tree clipped/not published | 2106993542/2143315168 | 188224917 | PASS / PASS / PASS | INELIGIBLE |
| workspace-shell-package-many-1025-sdk-v2 | 19789623791 / 25000000000 | 8806900666 / 5028155750 | UNAVAILABLE: complete timer tree clipped/not published | 8805492379/9814418170 | NOT_RUN | NOT_RUN / PASS / FAIL | INELIGIBLE |
| workspace-shell-package-many-129-live-g2-sdk-v2 | NOT_RUN / N/A | N/A / N/A | UNAVAILABLE: complete timer tree clipped/not published | UNAVAILABLE/UNAVAILABLE | NOT_RUN | NOT_RUN / NOT_RUN / NOT_RUN | INELIGIBLE |

¹ Nested attribution inside the complete command, not additive/exclusive competing arms. Timers with dropped events remain incomplete; counts in compact telemetry are observed-event counts only. Process-shared RSS is not a phase-owned Budget or cgroup peak. The failed SDK Commit interval is time-to-unknown, not successful Commit work.

Independent proofs PASS for all six completed rows: full old/new path kinds, portable modes, every regular-file byte, exact known head/parent and inherited core/index.js identity. Refresh verifies legacy deletion/new generated path; overwrite compares all10MiB plus untouched package; tiny tiers verify all128/129/257 files/listings. Verification is separate, never added to command time.

1025: Exec exits0 at8,806,900,666ns. SDK Commit returns Unknown at5,028,155,750ns; complete19,789,623,791ns<25s, cleanup5,586,193,333ns FAIL with shutdown Busy. Daemon WorkspaceCommit diagnostic reports9,814,418,170ns and success, including observed later HistoryCommand; canonical/local outcome was not acknowledged to SDK and remains unproved in this row. Forced Docker deletion cannot replace graceful close. No retry or promotion. Next retained129 case NOT_RUN because prior cleanup failed.

Source cause: `workspace.rs` supplies600,000ms for Commit; `native/connection.rs::Socket::read` requires wire activity every5,000ms. Daemon control Commit synchronously prepares/saves each dirty inode and emits no progress to its outer SDK session. Existing authenticated `Output::progress` is accepted for Exec/Init, but not WorkspaceCommit. Native service activity on another session cannot advance the SDK control session clock. Preserve both operation and silence deadlines; next concrete fix wires authenticated progress at completed preparation steps, without helper workers, retries, larger quotas/timeouts or broad #256 streaming changes.

Historical failed-command/no-commit method remains visible OWNER-DEFERRED / NOT_RUN on [#276](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5903530127), retaining Family6 r069 FAIL. No dirty-discard behavior added.

Checks before collection: new Python6/6 (one test-only prefix assertion fixed once), shared namespace5/5 and mutations10/10, list/parser and diff check PASS. Production LOC135638→135638(delta0), reference65417/Core70221; exact staged tree matches commit. After the progress fix, verify owning product/transport paths once and collect only failed1025 plus unattempted retained129 at new source. Six passing arms remain reused; no Family2 group. #256 general scale and full-cache numeric qualification remain open.
