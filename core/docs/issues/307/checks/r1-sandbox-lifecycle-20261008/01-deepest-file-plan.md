# R1d owned Sandbox lifecycle and SDK readiness

> Status: implementation plan; input 1b3e90e1fdb53d0af79fb3b5cc88215a96a00cb1. No release or native FUSE acceptance.

Main owns all edits/builds/tests/Docker/proof/cleanup. Existing read-only agents review protocol, lifecycle and standard runtime cancellation. Global Store is explicitly Disposable/WAL/OFF; Durable NOT_RUN disabled by owner. Overlay is separate MEMORY/OFF/EXCLUSIVE. No new worktree, remote write, whole-Sandbox implicit cancellation, host data service or reference retirement.

| Deepest owning source | Responsibility, resource and original custody |
| --- | --- |
| Sandbox backend/docker/http.rs | Reuse one exchange for two-pass JSON metadata and one-pass known-length tar. Fixed request window, original partial transfer/fence, no upload buffering or resend |
| Sandbox backend/docker/{container,container_types,archive,endpoint,listener}.rs | Exact immutable image and borrowed named VM volume; create once with explicit root daemon/NNP/no ordinary command privileges; stopped private tar config and binary, own container-local files; start once; actual listener log event; exact-ID endpoint observation; explicit one-attempt stop/delete preserving borrowed shared Store |
| Sandbox JSON parser | Narrow selected array support with no whole response DOM; deciding duplicates, exact ID/port/volume validation and unknown-field streaming |
| SDK sandbox/{api,lifecycle,types}.rs | Actual SandboxApi composes lower runtime owner, Bridge authentication and existing Control Hello. Private config bytes only archive; explicit command identity. Install borrows same Control without resetting correlation. No FS Ready or drain claim |
| SDK control/installation.rs | Narrow same-owner install seam, preserves original InstallFailure and failed channel state |
| Manifest/lock/guard | Actual SDK -> Sandbox edge using existing graph; Sandbox stays independent of Bridge/daemon/database |
| External tests/examples | Public endpoint framing/identity/partial upload/lost start/readiness/once-only cleanup; bounded waits. Actual owned named VM volume/daemon proof, ordinary nonroot permission observations. Build first; each test explicit <=100s, scoped functional proof narrower |
| Architecture/ledger/results/LOC | Distinguish control startup vs FUSE Ready, exact source pins and failures, exact first-parent/final staged/committed production LOC |

A Docker start acknowledgement and assigned port do not establish listener readiness. The readiness wait consumes actual daemon stdout marker once through ordinary log framing. Authentication connects once after that event. Failed or partial operations retain exact owned identity and never auto-delete/replay/adopt by name. Stop/delete are explicit whole-Sandbox crash teardown until graceful daemon drain is implemented. No process/descriptor/mapping/FS drain is inferred from shell exit or stream EOF.

Published official static containerd2.2.4 client preparation is separate qualification work. No exact process signal is attempted without acknowledged Engine Exec/container identity and positive runtime endpoint/CNI-extension evidence.
