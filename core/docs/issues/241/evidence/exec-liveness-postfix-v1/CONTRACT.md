# #241 baseline Exec after missing-Inspect session correction

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Frozen before the sole public SDK attempt at source `dde88f1145143bd11c3908209330d0a8ff7abe51`.

Use a fresh validated writable copy of the closed 1 MiB master, one Branch,
one Sandbox and one mounted Workspace. Call public SDK Exec exactly once with
`printf baseline > .position-baseline`, then unmount and confirm Sandbox
deletion and absence. The [plan](plan.json) pins the release binary, daemon
image, master/copy hashes, command, telemetry identity, cache state and fresh
output paths. No range edit, Commit, retry or registered position is included.

Require typed Exec success, daemon missing Inspect refusals followed by
ReserveInodes **without** a new Service TCP/Noise/Hello span, the host acceptor
shutdown summary, Docker state and cleanup. Missing observations classify the
diagnostic `INCOMPLETE`. Cache remains uncontrolled; this is a functional route
check with no latency admission. It cannot establish that Docker networking
will never stall on another connection.
