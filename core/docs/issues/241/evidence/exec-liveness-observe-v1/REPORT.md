# #241 baseline Service connection observation

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The sole [frozen one-shot plan](plan.json) ran at source `f4f82cb221d8354d9229096e631a95dca4dc820b` with the immutable image `sha256:d3c955caff865f8752fafd65614a9808185ab03946ba52fe93e3bbf083feb789`. It used an independent writable copy of the closed 1 MiB master and public SDK `printf baseline > .position-baseline`. Cache was uncontrolled. The exact source, image, binary, master, copy and output hashes are in the plan; the [attempt](attempt.json) and [raw files](raw/) are retained with a [SHA inventory](RAW-SHA256.txt).

Exec returned exit 0 in 24 ms of host context wall. Docker reported a running, non-OOM daemon. Unmount, Sandbox deletion and absence passed. The host acceptor shutdown summary is present: **3 accepted, 3 admitted, 3 reaped, peak live 2, zero capacity drops, capacity 4**. Three daemon fresh connections have TCP spans of 0.752–1.142 ms, Noise spans of 0.812–1.027 ms, and checked Hello spans of 0.264–0.315 ms. The missing-path Inspect refusals and subsequent ReserveInodes are visible. Attribution coverage is complete for this attempt.

This is one functional **PASS**, not a latency admission or a historical replay. It does not locate the v3 five-second stall. The separate [three-slot synthetic pressure](../exec-liveness-pressure-v1/REPORT.md) and [128-name churn diagnostic](../exec-liveness-churn-v1/REPORT.md) test specific hypotheses at their own source identities.
