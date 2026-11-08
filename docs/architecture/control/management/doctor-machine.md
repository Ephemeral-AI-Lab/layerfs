# Machine diagnostics

[Doctor](doctor.md) · [Workers](../routing/workers.md)

`doctor machine <name>` observes one saved machine. It does not initialize
or migrate the home, refresh records, create leases, start machines, deploy
bundles, execute guest commands, or probe sandbox daemons.

The target family is implemented in
`control/crates/layerfs-control/src/doctor/machine_target.rs`. The shared
Doctor dispatcher owns report shaping and the overall deadline.

| Check | Observation |
| --- | --- |
| `machine.record` | Name, saved identity, known platform, numeric settings and intent decoding |
| `machine.operation` | Live, unresolved or stale lifecycle intent; no clearing |
| `machine.provider` | Supported host/profile, required provider tools, observed tool version and runtime state |
| `machine.state` | Saved state versus observed provider state; never refreshed |
| `machine.worker.endpoint` | Saved loopback address, nonzero port and nonempty secret |
| `machine.worker.auth` | Worker authenticates and returns `ready: true` |
| `machine.worker.compatibility` | Protocol and applicable sealed-bundle identity |
| `machine.worker.ping` | Existing read-only ping returns worker kind, positive PID and this build's package version |
| `machine.capacity` | Available bytes on the worker's fixed data-root filesystem |

Native macOS/Linux uses the platform's local prerequisite checks and has no
worker or separate guest data disk. Unsupported platforms remain unsupported.
A live or unresolved lifecycle intent blocks provider and worker probes.
Stopped, absent and failed runtimes are distinguished; stopped runtimes are
left stopped and their worker checks are skipped. A running provider whose
worker fails stays reported as a running provider, rather than being relabeled
stopped. Authentication/compatibility failures block dependent checks.

Provider observation uses `provider_within` through the existing sealed platform
adapter. Unlike ordinary status, it does not combine provider state with worker
reachability or execute guest `df`. Docker inspects the expected runtime name
and checks its `layerfs.role=machine-worker` identity; foreign containers and
transitional states are not reported healthy. A failed provider command is not
proof that the runtime is absent.

The diagnostic worker connection uses the existing frame codec on one socket.
Every connect/read/write is bounded by one absolute deadline; a peer sending
bytes slowly cannot extend it. Hello and ping frames are capped at `MAX_HELLO`.
Only an authenticated compatible worker receives `worker.ping`, with
`args.capacity=true`. The worker returns nullable `free_disk` from `statvfs`
on its own fixed root; it never accepts a caller-selected path. Ordinary ping
requests do not perform this optional capacity observation. Missing capacity
remains unknown rather than being replaced by host space or a fixed threshold.
Capacity is a point-in-time observation, not reserved space or a product limit.

Each provider observation is capped at ten seconds and the worker socket at
five seconds, both shortened by the remaining overall budget. Tool commands
run in a private process group and are killed/reaped when their deadline
expires. Pipe-holding descendants are ended before collectors are joined.
Workers receive no lifecycle operation or retry. A timed-out or unobservable
check makes coverage incomplete; earlier confirmed failures are retained.

Provider stand-ins and worker fault peers verify the observational boundary and
failure handling; they do not qualify live Docker, Lima or Apple Container
installations. Native host checks and filesystem available-byte observations
use the actual host. Targeted diagnostic fixtures create no Store and execute
neither Disposable nor Durable persistence.

Saved runtime state is restricted to the producer's `running`, `stopped` and
`failed` values. Malformed state is reported without echoing its payload.
Operation evidence exposes typed standing/uncertainty and only recognized
machine operation names/phases and well-formed operation IDs. Arbitrary saved
operation fields, timestamps, arguments and nested values are not copied into
diagnostic output. An operation's standing is observed once for the report and
its conservative no-probe boundary.
