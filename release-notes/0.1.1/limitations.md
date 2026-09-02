# LayerFS 0.1.1 limitations

> **Status:** Released limitations for the `v0.1.1` Developer Preview.

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
- The SDK, CLI, daemon, FUSE helper, and Store format must come from a
  compatible release identity.
- Benchmark results are fixture- and host-specific evidence, not universal
  latency or throughput guarantees.
- Terminal namespace and payload gates pass for the retained source-sealed
  campaigns; they are workload-specific evidence, not universal guarantees.

See the [versioned limitations](../../docs/versioned/0.1.1/limitations.md)
and [verification record](verification.md).
