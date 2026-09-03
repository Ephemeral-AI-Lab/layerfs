# LayerFS 0.1.2 limitations

> **Status:** Draft limitations for the withdrawn `v0.1.2` release candidate.

LayerFS remains a Developer Preview for local evaluation, integration,
benchmark reproduction, and design research—not production storage.

- Keep an independent copy of important data. Live-process acknowledgement is
  not a crash- or power-loss durability guarantee.
- Keep the SQLite Store outside every imported or projected tree.
- One Client binds one local Store; cross-host synchronization, replication,
  repair, and multi-authority writes are outside the product surface.
- Workspaces are ephemeral. End never commits implicitly.
- Managed FUSE requires a compatible Linux container runtime, `/dev/fuse`, and
  `CAP_SYS_ADMIN`; it is not a complete hostile-code security boundary.
- A newly created FUSE file uses direct I/O until its create handle closes.
  Same-handle and concurrent-handle I/O is coherent, but mmap on that create
  handle returns `ENODEV`; close and reopen the file before mapping it.
- The SDK, CLI, daemon, FUSE helper, and Store format must come from a
  compatible release identity.
- Benchmark results are fixture- and host-specific evidence, not universal
  latency or throughput guarantees.
- Owner-side batched range edits must target one Workspace and one regular file.
- The retained SQLite Store misses the 600 MB primary-control footprint goal at
  662,831,104 median bytes; physical packs are deferred to issue #18.
- Terminal edit-family and Store-verification gates are workload-specific
  evidence. Same-count A/A is aggregate repeatability evidence, while the final
  count-changing result is a directional baseline/candidate tolerated-pass and
  makes no improvement claim for rows above the 1.05 target.

See the [versioned limitations](../../docs/versioned/0.1.2/limitations.md)
and [verification record](verification.md).
