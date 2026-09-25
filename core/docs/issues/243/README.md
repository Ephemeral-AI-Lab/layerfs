# #243 generic Workspace shell package refresh

> **Status:** Phases 0–1 implemented; Phase 2 baseline pending.

Tracking: [#243](https://github.com/Ephemeral-AI-Lab/layerfs/issues/243), a
sub-issue of [#232](https://github.com/Ephemeral-AI-Lab/layerfs/issues/232).
This folder prepares the user-facing Workspace shell test and rollout plan.
It contains no new product implementation, registered performance result, or
release-admission claim.

- [Test Workspace and commands](TEST_WORKSPACE_AND_COMMANDS.md) defines the
  versioned package fixtures, exact ordinary shell operations, and independent
  correctness oracle.
- [Implementation plan](IMPLEMENTATION_PLAN.md) defines the source, benchmark,
  verification, and decision checkpoints for the generic POSIX/FUSE path.
- [Frozen Phase 1 contract](PHASE1_CONTRACT.md) registers the ordinary-shell
  scenario, prepared inputs, route, oracle, limits, and custody rules.

The public SDK method is `WorkspaceApi::exec(command)` and currently launches
`/bin/sh -c` in the mounted Workspace. Its command uses normal filesystem
operations. The [#232 route correction](../232/SHELL_ROUTE_CORRECTION.md)
separates this workflow from the historical cooperating-tool ioctl results;
none of those receipts is a generic-shell PASS. The
[benchmark rules](../../../../docs/general/benchmark_rules.md) require a new
committed scenario contract and exact source/cache custody before live
collection. The #232 completion gate remains open.

Phase 0 removed the Linux FUSE range-ioctl dispatch and staging worker. The
former STATE request now returns `ENOTTY`; the fixed Status range-counter slots
remain for historical reader compatibility and cannot be incremented by FUSE.
The benchmark-only edit tool and its registries remain archival opt-in material,
outside the new generic-shell image. The new mounted negative-capability test
requires Linux FUSE and is pending its live Phase 2 run. Core tests, Clippy,
formatting, and boundary checks passed on the source retirement checkpoint.
