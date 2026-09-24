# #241 baseline SDK Exec connection diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Frozen before the sole diagnostic attempt at source `f4f82cb221d8354d9229096e631a95dca4dc820b`.

Run `baseline_exec_liveness_diagnostic` once from an independent writable copy
of the closed 1 MiB master. It uses public SDK Project, Branch, Sandbox and
Workspace operations, then exactly `printf baseline > .position-baseline`.
There is no range edit, Commit, retry or position selection. Cache is
uncontrolled; this run makes no latency admission claim. The existing
five-second transport progress rule, 30-second Exec deadline and worker count
remain unchanged.

The [plan](plan.json) pins source, binary, image, master, copy, command,
telemetry identity and fresh output paths. Retain the first typed Exec result,
daemon TCP/Noise/Hello spans, host acceptor admission/drop/summary lines,
Docker state and daemon logs, unmount and Sandbox deletion/absence. If either
the host acceptor summary or daemon connection spans are missing, classify the
diagnostic `INCOMPLETE` even if Exec succeeds. A successful one-shot run alone
does not establish the cause of the historical v3 `Unknown,true`.
