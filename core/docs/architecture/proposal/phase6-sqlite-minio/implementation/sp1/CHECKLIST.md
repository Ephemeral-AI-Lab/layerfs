# SP1 implementation checklist

> Status: Current planning checklist; no release candidate exists.

Owned worktree: `/Users/yifanxu/.codex/worktrees/phase6-sp1/layerfs`.
Owned branch: `codex/phase6-sp1`; initial source `8c926b9392f3636ae156236dc26d0510ee069d8d`.

- [x] Inspect actual source, ownership and current #295/#293 discussion.
- [x] Seal unchanged producer payload/pool canonical and physical fixtures.
- [ ] SP1.0: neutral read boundary, metadata-only scoped catalog, wire allocation.
- [ ] SP1.1: authenticated real MinIO FULL/PREFIX/pool reader and refusals.
- [ ] SP1.2: shared selector/pool algorithms, bounded private placement and readiness.
- [ ] SP1.3: generic SDK/FUSE three-Commit retained history proof.
- [ ] Freeze/register comparable MinIO history profile; stride10/3/1 once each.
- [ ] Final owning Core/adapter checks and complete source/status handoff.

Every checkbox is a delivery gate, not a forecast. Independent filesystem-root
oracle is currently INCOMPLETE because deterministic directory timestamps have
not been established through supported operations. Resource admission remains
open: codec/cache/index operations require an explicitly serialized daemon
owner; the eager 16MiB encoder alone cannot fit alongside Workspace data within
16MiB. No quota increase or physical-memory PASS follows from assigning owners.
S2 shared-engine/population, C1 unfinished drafts, non-pausing/concurrent use,
full import, GC/restart/migration and cloud durability remain separate gates.

See [append-only log](IMPLEMENTATION-LOG.md) for actual checks and evidence.
