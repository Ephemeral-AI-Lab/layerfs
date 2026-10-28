Private organization implementation workspace for LayerFS Area P. Upstream `Ephemeral-AI-Lab/layerfs` remains read-only; development lands on `area-p/p1-p15` in this repository.

Parent: https://github.com/Ephemeral-AI-Lab/layerfs-connect/issues/14

## Original Area P issues

- [ ] P1 — Node SDK history calls: https://github.com/Ephemeral-AI-Lab/layerfs-connect/issues/53
- [ ] P2 — JS constructor configuration: https://github.com/Ephemeral-AI-Lab/layerfs/issues/484
- [ ] P3 — CLI history, push and pull: https://github.com/Ephemeral-AI-Lab/layerfs-connect/issues/54 (push/pull blocked by R7 #42)
- [ ] P4 — Python SDK: https://github.com/Ephemeral-AI-Lab/layerfs-connect/issues/55
- [ ] P5 — MCP server: https://github.com/Ephemeral-AI-Lab/layerfs-connect/issues/56
- [ ] P6 — Shared contract tests: https://github.com/Ephemeral-AI-Lab/layerfs-connect/issues/57
- [ ] P7 — Commit author and labels: https://github.com/Ephemeral-AI-Lab/layerfs-connect/issues/58
- [ ] P8 — Read files at Commit or Layer: https://github.com/Ephemeral-AI-Lab/layerfs-connect/issues/59
- [ ] P9 — Event stream: https://github.com/Ephemeral-AI-Lab/layerfs-connect/issues/60
- [ ] P10 — Per-call results API: https://github.com/Ephemeral-AI-Lab/layerfs-connect/issues/61
- [ ] P11 — Store statistics: https://github.com/Ephemeral-AI-Lab/layerfs-connect/issues/62
- [ ] P12 — Store summary: https://github.com/Ephemeral-AI-Lab/layerfs-connect/issues/63
- [ ] P13 — Folder-scoped client: https://github.com/Ephemeral-AI-Lab/layerfs-connect/issues/64
- [ ] P14 — Pagination fixes: https://github.com/Ephemeral-AI-Lab/layerfs-connect/issues/65
- [ ] P15 — Publish `@layerfs/*` to npm: https://github.com/Ephemeral-AI-Lab/layerfs-connect/issues/66

## Execution records

- Design: `docs/superpowers/specs/2026-10-28-area-p-design.md`
- Foundation plan: `docs/superpowers/plans/2026-10-28-area-p-foundation.md`
- Local ultragoal plan: `area-p-20261028`

Real Store/FUSE verification runs on Linux with Disposable/WAL/synchronous=OFF only. R7 push/pull remains an external dependency and will not be simulated.
