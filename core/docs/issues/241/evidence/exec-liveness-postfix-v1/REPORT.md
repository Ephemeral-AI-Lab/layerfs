# #241 first mounted check after native missing-Inspect retention

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The sole [frozen plan](plan.json) ran at source `dde88f1145143bd11c3908209330d0a8ff7abe51` with immutable daemon image `sha256:4ee70e1f9aa77774b4928ef526e9fb9105e635cbee66fd6aa1d029570146801c`. The public SDK baseline Exec returned exit 0 in 19 ms from a fresh validated 1 MiB master copy. Docker showed a running, non-OOM daemon; unmount, Sandbox deletion and final absence passed. The [attempt](attempt.json), [raw logs](raw/) and [SHA inventory](RAW-SHA256.txt) retain the complete run. Cache was uncontrolled.

The host acceptor summary is present: 3 accepted, 3 admitted, 3 reaped, peak live 2, zero drops, capacity 4, no terminal acceptor error. Both missing-path Inspects returned known refusals, but the following `HistoryCommand::ReserveInodes` still shows a new `daemon.service_connect` with TCP, Noise and Hello children. The [contract](CONTRACT.md) required no new connection there, so the **route criterion FAILS despite typed Exec success**. No registered position or release latency case was sampled.

Source tracing found the remaining reset in `layerfs-daemon/src/run.rs`: its outer delivery closure discarded the transport after every error, even though the native client/server and inner transport had retained the synchronized missing-Inspect refusal. This attempt is unchanged and does not qualify the correction in source `0da912016`.
