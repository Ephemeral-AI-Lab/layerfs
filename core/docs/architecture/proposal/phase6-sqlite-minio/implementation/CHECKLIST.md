# Autonomous Phase 6 integration checklist

> Status: Current planning checklist; no release candidate exists.

Owner objective, 2026-10-02: full MinIO CAS + global SQLite metadata + daemon
SQLite integration; real 67-descendant namespace, 270-component path, 128-file
cases; the seven fs-bench-pro families; complete DeepSeek harness import and
ordinary WorkspaceApi.exec mutations whose updates stay local as the namespace
and payload population grows. Goal remains active until all requested groups and
locality/scaling invariants have owning evidence. The earlier 4 KiB V3 proof does
not complete this objective.

Owned worktree: `/Users/yifanxu/.codex/worktrees/phase6-sqlite-minio-design/layerfs`.
Branch: `codex/phase6-metadata-experiments`. Initial current source:
`1a59e129f8a3d73f49efe9691b53ba4b377be612`. Preserve primary checkout, research
receipts, unmerged Phase 5 candidates and other owners' targets/runs.

The latest owner objective explicitly authorizes the new Phase 6 seven-family
work, superseding the earlier restriction on running those families in this
thread. It does not authorize changing #288, promoting historical receipts,
merging, issue closure or rollback. The owner-selected daemon SQLite/MinIO
architecture is a separate declared topology from historical host-Store profiles.
Numbers with unknown cache/resource observations remain ineligible/incomplete.

## Delivery and gates

- [x] V3: one full public-SDK/kernel-FUSE/SQL/C1/C2/MinIO/C5 two-head 4 KiB path.
- [ ] V4a: count/timing diagnostics and persistent authenticated metadata session;
  exact request IDs, EOF/version checks, bounded rotation and no resend; prove
  actual session counts and preserved full-path bytes/heads/cleanup.
- [ ] V4b: bounded multi-object packs, locator batches, admitted decode/locator
  windows, immutable object ACK/registration and exact CAS collision checking.
- [ ] V4c: indexed changed catalogs, paged prepared rows, immediate-base C1 edits
  and filesystem COW; incremental certified publication validation, bounded source
  retirement, import/mount with inherited immutable data. No whole-workspace scan
  on an edit and no whole-file read on a localized large-file edit.
- [ ] V4d: generic syscall surface required by the actual namespace/package cases:
  rename/cycles, hardlinks, symlinks, orphan handles, metadata and explicit
  unsupported locking/durability behavior; source/generation/cancel custody.
- [ ] Run/prove all three named live shape cases with exact independent manifests,
  root/byte facts and retained history, through actual public WorkspaceApi.exec.
- [ ] Family1 history retention; Family2 public SDK namespace initialization;
  Family3 writes; Family4 retained Commits; Family5 namespace; Family6 mutations;
  Family7 shell/package. Freeze membership from the current owning modules;
  native Family4 proofs are auxiliary coverage, not an eighth family. Use the
  existing runner/backend selection with source-aware adapters; preserve true
  public SDK Init rather than substituting direct construction as an Init result.
- [ ] DeepSeek: include the complete sealed source, import, real generic commands,
  retained historical reads, rename/link/unlink/overwrite/append/churn, and exact
  source/cache/work/resource/cleanup evidence. Compare prospective namespace
  scales with the same mutation and measure work counts, not just bytes/timers.
- [ ] Physical resource and healthy progress proof for admitted simultaneous
  windows, disk/SQLite/MinIO/cache domains; unavailable observation cannot pass.
- [ ] Final owning locked checks, source/binary/image/provider seals, per-commit
  production LOC and checkpoint comments on #294; major results linked from #293.

Current action: V4a diagnostic instrumentation, then the single session-reuse
change. Every larger group remains incomplete. Use meaningful submilestones;
retain failed attempts and keep one concrete next action across continuations.
