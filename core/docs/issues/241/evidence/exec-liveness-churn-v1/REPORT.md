# #241 missing-path Service connection churn result

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The sole [frozen plan](plan.json) ran at source `9a377c85f37522325e0a3def8f9df26499f7c017` with the image sealed by the plan. One public SDK Workspace issued distinct baseline creates, stopping at the first failure. The [attempt](attempt.json), [raw receipt/logs](raw/) and [SHA inventory](RAW-SHA256.txt) retain every issued result; no position selection or performance sample was rerun.

Creates `000`–`126` completed with exit 0. Create `127` returned a known shell exit 1 after 13 ms with `No space left on device`, the Workspace's finite inode budget for this diagnostic. Thus the declared 128-create diagnostic is **127 PASS / 1 FAIL**, with cleanup PASS. Docker showed a running, non-OOM daemon. The host acceptor recorded **256 accepted, 256 admitted, 256 reaped, peak live 2, zero capacity drops, capacity 4, terminal error none**. Across those connections, TCP spans were 0.558–1.384 ms and Noise spans 0.689–2.462 ms; all 256 checked Hello spans were present.

This single sequence shows that substantial missing-path reconnect churn did not itself produce a five-second stall or fill the four host slots under this topology. Its final failure is a diagnostic workload capacity limit, not the v3 `Unknown,true` mechanism. Cache was uncontrolled; no latency admission follows from these timings.
