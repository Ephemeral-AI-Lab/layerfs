# Benchmark hosting

## Scoped v0.1.6 replacement experiment

For the owner-directed `v016-local-snapshot-experiment-v1` only, follow
`docs/roadmap/0.1/0.1.6/sandbox-local-snapshot-spec-and-plan.md` ahead of conflicting
legacy hosting/sampling text below or in the general guide/quick start. The
candidate may own mutable Workspace metadata, snapshot generations and temporary
payload backing in the sandbox. SQLite, the benchmark/SDK coordinator, canonical
construction and publication remain on the macOS host. Use one Commit compute
worker and one performance sample per case/arm; no n3/repeated-sample campaign.
This exception does not change unrelated benchmark or release contracts.

## Existing profiles

- SQLite, the SDK/benchmark coordinator, canonical construction/Commit publication, and physical spool backing must run on the macOS host.
- The approved #49 rewrite may place the live Workspace operation core with the Linux daemon/FUSE runtime. This is execution-side filesystem state, not a container-side SQLite Store, benchmark coordinator, or canonical publication service.
- Docker runs only the Linux daemon, FUSE, and workload helper. Never run or restore Docker-owned SQLite, prepared Store images, or a container-side benchmark coordinator.
- Migrate unsupported families to host execution; never add a Docker fallback or use a historical revision to bypass this prohibition.
- Historical Docker results remain unchanged and apply only to their recorded topology.
- Use the current fs-bench-pro family entrypoints and follow `docs/general/benchmark_rules.md` and `fs-bench-pro/QUICKSTART.md`.
