# #241 missing-path Service connection churn diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Frozen before the sole sequence at source `9a377c85f37522325e0a3def8f9df26499f7c017`.

Use one fresh validated 1 MiB master copy, one Branch, one Sandbox and one
mounted Workspace. Issue up to 128 distinct public SDK Exec creates in order:
`printf baseline > .position-baseline-000` through `-127`. Stop at the first
failure and retain each result. Each missing path drives Inspect refusals and
the subsequent ReserveInodes call without touching a registered position or
Edit→Commit case. The [plan](plan.json) pins the exact binary, image, master,
copy, sequence count, telemetry identity and output path.

Record host accepted/admitted/live/reaped/drop summary, daemon TCP/Noise/Hello
spans, typed Exec outcomes, Docker state and cleanup. This is a count-driven
functional diagnostic of reconnect churn, with uncontrolled cache and no
latency admission. It does not replace a frozen position or the one-shot
baseline attempt. Missing host summary or daemon spans makes attribution
`INCOMPLETE`.
