# Benchmark hosting

- SQLite, the SDK/coordinator, Workspace processing, and spool must run on the macOS host.
- Docker runs only the Linux daemon, FUSE, and workload helper. Never run or restore Docker-owned SQLite, prepared Store images, or a container-side benchmark coordinator.
- Migrate unsupported families to host execution; never add a Docker fallback or use a historical revision to bypass this prohibition.
- Historical Docker results remain unchanged and apply only to their recorded topology.
- Use the current fs-bench-pro family entrypoints and follow `docs/general/benchmark_rules.md` and `fs-bench-pro/QUICKSTART.md`.
