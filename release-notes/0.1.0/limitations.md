# LayerFS 0.1.0 limitations

> **Status:** Release-candidate limitations for LayerFS 0.1.0.

LayerFS 0.1.0 is a Developer Preview intended for development, integration
testing, benchmark reproduction, and design evaluation. Read these constraints
before using it with valuable data. The complete versioned list is in the
[0.1.0 limitations reference](../../docs/versioned/0.1.0/limitations.md).

## Data safety and durability

- LayerFS does not claim that every acknowledged operation survives sudden
  power loss, kernel failure, storage-controller failure, or forced process
  termination.
- A successful operation means its transaction is committed and readable from
  the live local LayerFS process.
- Keep an independent copy of important or irreplaceable data.
- Do not place the Store file inside a directory imported into a LayerStack or
  exposed as a Workspace projection.
- Automatic backup, remote replication, disaster recovery, and Store repair
  are outside the 0.1.0 product surface.

## Distribution and compatibility

- Rust crates are consumed from this source tree and are not promised as
  published registry packages.
- Availability of prebuilt CLI, daemon, FUSE helper, and runtime-image
  artifacts is authoritative only when recorded with a digest in
  [artifacts.md](artifacts.md).
- The Rust API, CLI grammar, container protocol, and SQLite format are
  Developer Preview interfaces.
- Mixing binaries, helpers, SDK crates, or Store files from different minor
  release lines is unsupported unless explicitly documented.
- The `0.1.x` patch line is intended to preserve the documented Store format,
  canonical identity, daemon compatibility, CLI grammar, and public SDK
  behavior. No compatibility promise is made for `0.2.0` or later.

## Store and process model

- One Client binds one local Store. Applications needing another Store create
  another Client.
- The Store is designed for one live local LayerFS process, not concurrent
  independent writers sharing the SQLite file.
- Cross-host access, remote object fallback, distributed consensus, and
  multi-authority operation are outside the release contract.
- Automatic canonical-object garbage collection and space reclamation are not
  part of the documented public operations.
- Entity rename and destructive LayerStack, Layer, Branch, or Commit deletion
  are not part of the documented public operations.

## Workspace and execution model

- Workspaces are ephemeral and must be ended explicitly.
- Workspace End never commits; callers must invoke Commit before End when they
  want to retain changes.
- Each command runs in a fresh process. A persistent shell process is not part
  of ordinary Exec semantics.
- Output retention is bounded and paged. Callers must handle truncation and
  cursors according to the [SDK](../../docs/versioned/0.1.0/sdk.md) or
  [CLI](../../docs/versioned/0.1.0/cli.md) contract.
- SDK entity queries are cursor-paged. The 0.1.0 CLI query command drains all
  pages into one response and is not the bounded interface for very large
  result sets.
- One CLI context binds one Store and at most one running managed container.
- Interactive terminal behavior is limited to the exact facilities documented
  by the CLI reference; it should not be treated as a general remote terminal
  service.

## Platform and FUSE constraints

- Local Store operations and materialized Workspaces target macOS and Linux.
- Managed-container FUSE requires a Linux container runtime with a functioning
  `/dev/fuse` device and `CAP_SYS_ADMIN`.
- Host-native FUSE availability depends on the target, build features, and
  host FUSE support. Materialized projection is the portable fallback.
- Managed-container operation requires a compatible daemon and FUSE helper
  from the same release identity.
- Container setup requires Docker-compatible lifecycle access. The expected
  image contents, loopback endpoint, capabilities, and resource controls are
  defined in the [container-runtime reference](../../docs/versioned/0.1.0/container-runtime.md).
- Windows is not a supported target for the 0.1.0 Workspace runtime.

## Operational scope

- Names are immutable. IDs remain the authoritative identity for durable
  entities.
- Conflict reconciliation requires explicit typed choices; LayerFS does not
  silently choose a winner for a stale Workspace.
- Commit, diff, query, and output operations are bounded, but required work may
  still scale with the selected content or history.
- Passive Monitor receipts are process-local observability, not a durable
  metrics service or billing ledger.
- Benchmark results describe the sealed environment and acknowledgement policy
  in [benchmark-results.md](benchmark-results.md); they are not latency or
  throughput guarantees for every host, filesystem, container runtime, or
  workload.

For installation and evaluation boundaries, start with the versioned
[quickstart](../../docs/versioned/0.1.0/quickstart.md) and
[product specification](../../docs/versioned/0.1.0/specification.md).
