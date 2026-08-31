# fs-bench-pro optimization handoff

Use [`../v2-replacement/spec.md`](../v2-replacement/spec.md) as the only
normative architecture. Run `benchmark/fs-bench-pro/run.sh --self-check`, then a
fresh sealed campaign with the native macOS public SDK/Store and one prepared
FUSE-capable daemon container. The container must have no host bind and must
publish only its authenticated daemon port to host `127.0.0.1`.

If a hard target fails, use the six T0–T4 fields, the Store Workspace Commit
receipt, FUSE read/write receipts, and the helper's independently reported
inner-write interval to locate the phase. Change the smallest shared root cause,
rerun the focused tests, and create a new immutable run ID. Never reuse a
Workspace, process, Store, evidence directory, or earlier result to improve a
measurement.
