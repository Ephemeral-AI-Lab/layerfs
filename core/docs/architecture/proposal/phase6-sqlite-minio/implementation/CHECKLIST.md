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
- [x] V4a: count/timing diagnostics and persistent authenticated metadata session;
  exact request IDs, EOF/version checks, bounded rotation and no resend; prove
  actual session counts and preserved full-path bytes/heads/cleanup.
- [x] V4b: bounded multi-object packs, locator batches, admitted decode/locator
  windows, immutable object ACK/registration and exact CAS collision checking.
- [x] V4c1 dependency: indexed dirty/name SQL rows, immediate-base file/namespace
  construction and captured-row install; real two-head gate passed. See
  [RESULTS-V4C1](RESULTS-V4C1.md). Speed/resource/canonical qualification unrun.
- [x] V4c2a dependency: typed file graph certification in existing global SQL;
  small-file provider gate passed. [RESULTS-V4C2A](RESULTS-V4C2A.md).
  Larger chunked-provider/resource/canonical qualification remains open.
- [ ] V4c: indexed changed catalogs, paged prepared rows, immediate-base C1 edits
  and filesystem COW; incremental certified publication validation, bounded source
  retirement, import/mount with inherited immutable data. No whole-workspace scan
  on an edit and no whole-file read on a localized large-file edit.
- [ ] V4d: generic syscall surface required by the actual namespace/package cases:
  rename/cycles, hardlinks, symlinks, orphan handles, metadata and explicit
  unsupported locking/durability behavior; source/generation/cancel custody.
- [x] V4d1 live67and128three-head composition/complete semantic-history-cleanup
  gates; [RESULTS-V4D1](RESULTS-V4D1.md). 270TIMEOUTand locality/scaling remain red.
- [x] V4c2b1 SQL-native constructor/process dependency: original67/270/128full
  three-head semantic/history/cleanup; [RESULTS-V4C2B1](RESULTS-V4C2B1.md).
  Service locality/root vectors/physical qualification remain open.
- [x] Run/prove all three named live shape cases with exact independent manifests,
  root/byte facts and retained history, through actual public WorkspaceApi.exec.
- [ ] Family1 public SDK namespace initialization; Family2 history retention;
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

- [x] V4c2b2 structural dependency: intrinsic facts, selected-root differences,
  indexed reference/parent proof, exact stamp and known C5installation;8external
  C1/SQLite tests pass. [Runtime checkpoint](V4C2B2-RUNTIME.md).
- [x] V4c2b2 real-provider original67/270/128three-head bytes/history/cleanup
  and local one-file service counts. [RESULTS-V4C2B2](RESULTS-V4C2B2.md).
  Physical/larger population/independent canonical qualification remains open.

- [x] V4c3a bounded cookie directory pages and referenced local source retirement;
  original full128five-root generic enumeration/churn/cleanUpToDate live proof.
  [RESULTS-V4C3A](RESULTS-V4C3A.md). Larger/physical/orphan/custody gates stay open.

- [x] V4c3b bounded read/write span keysets and short SQL windows; corrected
  full128+sparse six-root live semantic/history/cleanup proof.
  [RESULTS-V4C3B](RESULTS-V4C3B.md). Initial failed oracle retained; physical gates open.

Current action: V4c3c paged reservations/admission/inherited import/mount before raising
current512/256profile. Freeze exact source/count proofs, preserve accepted/Unknown
custody. Remaining generic syscalls, genuine seven-family/complete DeepSeek
commands/locality/resource proofs remain required. Full goal active.
