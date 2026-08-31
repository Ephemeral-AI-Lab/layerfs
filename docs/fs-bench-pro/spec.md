# fs-bench-pro current contract

The sole normative benchmark architecture is
[`../v2-replacement/spec.md`](../v2-replacement/spec.md), sections 17–19.

The current harness is [`../../benchmark/fs-bench-pro`](../../benchmark/fs-bench-pro).
It measures one local `LayerStackStore` through public SDK operations and the
exact T0–T4 lifecycle defined by the replacement specification. The benchmark,
public SDK, single Store, Workspace runtime/spool, and FUSE `ProxyHost` run
natively on macOS. Every execution starts a fresh process in a real FUSE
Workspace served by one already prepared Linux container. That container owns
only the accepted control daemon, fresh helpers/processes, workload binary, and
fixture: it has no Store/runtime/result/binary/fixture host bind. Its daemon port
is published only on host `127.0.0.1` and every control stream is capability
authenticated. Store/image/container preparation is outside the timer;
Workspace Create, execution/output completion, Commit, and End are inside their
reported boundaries.

The required fields are:

```text
workspace_create_ns
execution_ns
commit_api_ns
layerstack_visible_ns
workspace_end_ns
complete_lifecycle_ns
```

The harness separately reports the inner 32 MiB write interval without
substituting it for `execution_ns`. It verifies Store visibility through public
`Client::query` after recording T4, so proof work changes none of the six timed
equations.

Raw JSONL, source seal, Git state, host/container custody, and the generated
Markdown report are retained under `benchmark-results/fs-bench-pro/runs`.
