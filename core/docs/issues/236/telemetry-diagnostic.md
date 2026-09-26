# Agent SDK functional telemetry diagnostic

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

This is a one-run exploratory diagnostic for the public
`sandbox.create` → `workspace.mount` → `workspace.exec` →
`workspace.commit` → `workspace.unmount` route. It is not a registered
fs-bench-pro family, a comparison arm, a release gate, or evidence for #232's
direct SDK range-edit cases. No latency, CPU, or memory value is a PASS.

## Declared one-run boundaries

- Fresh host Store/history and the one-file `note=base` import, Branch fork,
  binaries, image, and listener are setup outside the route. The first
  container has no prior Workspace or edit.
- The measured sequence contains one call to each public method above, in that
  order. Exec runs `printf first > note` through the mounted FUSE directory;
  Commit is explicit. Each monotonic operation timer begins immediately before
  the SDK call and ends on its return. SDK telemetry records these five calls.
- A sixth LayerFS timing window encloses the five-call route, including its
  telemetry handoffs and minimal SDK orchestration. The sum of the five
  operation-local times is reported separately. Correctness checks, reopened
  readback, historical conflict, restart checks, cleanup, and report generation
  follow outside the route timer.
- The SDK host process and host Service share one process. Its 10 ms sampled
  CPU and RSS windows are process-shared and cannot assign exclusive CPU to
  an SDK call. The Linux daemon records authenticated control operations and
  process-shared 10 ms CPU/RSS windows in its own clock domain.
- All reported time, CPU and RSS values come from `layerfs-telemetry` LFT1
  operation records. A process window needs two valid samples for CPU delta;
  shorter operations can report CPU unavailable. Sampled maximum RSS is not an
  exact phase peak. The daemon monitor does not count shell Exec child CPU/RSS,
  and there is no Docker cgroup total or `memory.peak` claim in this diagnostic.
- Source and OS cache state is uncontrolled. No cold or warm cache claim or
  numeric performance PASS is possible. The row is
  `admission_eligible=false` and `INELIGIBLE` for performance admission even
  when functional assertions pass. One diagnostic run is retained; failures
  are not overwritten or resampled at the same source identity.

The fresh evidence directory keeps the command log, host and daemon LFT1 raw
events, source/binary/image identities, derived report, and a SHA-256 manifest.
`core/tools/sdk_telemetry_report.py`
derives the report solely from those retained files. Historical receipts stay
unchanged.
