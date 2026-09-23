# Agent-facing LayerFS API

> **Status:** Project Init implemented in Core; Workspace and exec remain proposals.

This is the application boundary for agents. `layerfs-sdk::Client::init_project`
is the live host-direct operation. It takes a project name and host-visible
directory and returns the published LayerStack ID, genesis Layer ID, root and
root serial. Workspace and execution methods explicitly return `Unsupported`.

## Proposed layout

```text
core/                                  existing Cargo workspace and lockfile
  crates/layerfs-api/
    core/                               layerfs-api-core: project result and errors
      Cargo.toml
      src/lib.rs
      src/project.rs
    sdk/                                layerfs-sdk: agent-facing client and handles
      Cargo.toml
      src/lib.rs
      src/client.rs
      tests/
    mcp/README.md                       future adapter; no MCP package yet
    cli/README.md                       future adapter; no CLI package yet
```

The first issue adds only the source needed for a working `init_project` and small
Workspace/exec placeholders that explicitly report unsupported operations.
Both Rust packages join the existing `core/Cargo.toml` workspace; there is no
second lockfile or target. Before nested product source lands, extend the
production LOC counter and product-boundary guard to include
`core/crates/layerfs-api/{core,sdk}/src/`. Tests live outside `src/`. The
existing [Core agent rules](../../AGENTS.md) still apply.

`layerfs-api-core` owns the shared project result and error vocabulary.
`layerfs-sdk` owns the ergonomic client; the Service owns source binding.
The host Service continues to own Init/C1/C2/C5 composition; the Linux daemon
continues to own mounted Workspace lifecycle. Future MCP and CLI adapters call
the SDK instead of reimplementing operation or error rules. If the first slice
does not establish a real shared-core responsibility, keep it as a module of
the SDK crate rather than adding a package only to fill the diagram.

## Agent workflow

The intended call sequence is:

```text
project = api.init_project(project_name, host_path)
workspace = project.mount_workspace()         future
result = workspace.exec("cargo test")         future, shell in execution environment
commit = workspace.commit()                   future, explicit
workspace.unmount()                           future, does not commit or close
workspace.close_clean()                       future, refuses retained dirty state
```

The first call takes only the name and path. The backend supplies identities,
scope seed, construction policy and history publication. `Project` is the SDK
name for the existing LayerStack and its genesis root, not a second persistent
entity. It does not imply a Branch, Workspace, mount or process execution.
The future `mount_workspace` backend must create or select a writable Branch
before attach; the SDK need not expose that bookkeeping as an argument.

## Placement and route

```text
host agent/SDK  ── init_project(name, host path) ──> host Service ──> C1/C2/C5
host agent/SDK  ── Workspace control ──────────────> Linux daemon ──> Workspace/FUSE
host agent/SDK  ── exec ────────────────────────────> execution-side daemon
```

The daemon route still reads its startup-bound `LAYERFS_IMPORT_ROOT`. The SDK
calls the host Service directly; its validated, canonical source path is bound
to that one authorized import call without changing the daemon route's root.
`path` must be a directory visible to the host Service. Remote source staging
is separate future work and must preserve the same timed import and verification
boundaries.

The existing headless daemon route remains useful as a delivery diagnostic;
it is not required for host-direct project initialization. Historical
`init_namespace` receipts keep their recorded route and identity. See the
[#231 first-pass report](../benchmark/fs-bench-pro/issue-231/RESULTS-20260923.md)
and [API operation contracts](operations.md). The [#236 functional proof](evidence/issue236/README.md)
and [SDK benchmark report](../benchmark/fs-bench-pro/issue-236-sdk-init/RESULTS-20260923.md)
record the 100/1,000-file source manifests and single raw SDK Init times.
