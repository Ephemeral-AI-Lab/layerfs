# Cloudflare Computer — architecture overview

> **Status:** Research; informative and not a product contract.

External study for LayerFS v0.1.7, recorded under issue
[#158](https://github.com/Ephemeral-AI-Lab/layerfs/issues/158) and feeding the
[#155](https://github.com/Ephemeral-AI-Lab/layerfs/issues/155) architecture
refactor. This document describes what Cloudflare Computer is and what LayerFS
can learn from it. It selects nothing, authorizes nothing, and measures
nothing; observations that imply work belong to the v0.1.7 checklist.

- **Source pin:** upstream `https://github.com/cloudflare/computer`, studied
  from the local snapshot
  `/Users/yifanxu/Ephemeral-AI-Lab/study/cloudflare-computer` at commit
  `64c462b083860cad29b374b9e4cda1e6a680f902` (2026-08-26, "bench: account for
  spawned computerd tasks"). Every citation below was read at that commit.
  Written 2026-09-16.
- **Method:** source reads only — no builds, tests, benchmarks, or code
  changes in either repository. Research ran as eight parallel read-only area
  studies (DO store/schema; sync protocol and mounts; runtime and backend
  registry; container backend; isolate backends; fs/tools/observe; git/assets/
  artifacts; lifecycle + spec-vs-code audit). About 30 citations from those
  reports were re-opened and verified by the coordinating author, and the core
  wire contracts, schema, and runtime were additionally read first-hand.
  Unverifiable claims are listed in [Unknowns](#7-unknowns), never guessed.
- **Preview caveat:** the subject's own README marks it PREVIEW ONLY with
  unstable APIs and says its spec under `docs/` is forward-looking — "read it
  for intent, not as description of the code today." Where docs and code
  disagree at this pin, this document reports the code and notes the drift.

## 1. The system in one paragraph

Cloudflare Computer is a virtual filesystem that lives inside a Cloudflare
Durable Object. The DO holds the authoritative filesystem state in SQLite —
an inode graph plus a content-addressed chunk store — and exposes it through
a Node-like async `fs` API and one pluggable execution surface,
`workspace.runtime.exec(source, { backend })`. Three backends ship: a sandbox
**container** in which a daemon (`computerd`, a single Node-SEA binary) mounts
the state as a real FUSE filesystem and syncs changes with the DO over a
capnweb RPC WebSocket; an **isolate shell** running just-bash (a JS bash
interpreter, no real processes) in a Dynamic Worker; and an **isolate
JavaScript** runner evaluating an ES module in a fresh Dynamic Worker with
Workspace-backed `node:fs/promises` and trusted `ws:git`/`ws:artifacts`
modules. Both isolates reach the authoritative Workspace over Workers RPC —
no second store, no sync round trip. Its own framing (`docs/README.md:13-31`):
persistent SQLite-backed filesystems "primarily designed for agents that need
small, portable filesystems," ~10 GB scale ("agent-scale workspaces, not full
monorepos"), durable across DO restarts, execution as a projection of the one
store.

## 2. Component map

```text
cloudflare-computer @ 64c462b (TypeScript throughout)
├── packages/dofs (@cloudflare/dofs) — the DO-side SQLite VFS; the heart.
│     schema/ (v6 DDL + one-transaction migrations), fs/ (one module per op:
│     writeFile with 512 KiB chunking + open-file write buffers, readFile,
│     resolve/resolveCache, mount-guard, gc, watch, …), sync/ (changes,
│     coalesce, apply, blobs, manifests, watermarks, invariant, ignore),
│     storage.ts (Database over DurableObjectStorageLike; reentrant
│     transactionSync), provider.ts (node:fs shape for FUSE and git)
├── packages/rpc (@cloudflare/computer-rpc) — capnweb wire contract:
│     SyncRPC + ShellRPC composed as WorkspaceRPC; sync-driver
│     (pushOnce/pullOnce/batch, PULL_BATCH_SIZE=256)
├── packages/computerd (@cloudflare/computerd) — in-container daemon (Node
│     SEA binary): cli/ (env, mount-before-listen, routes, /connect
│     reverse-dial), fuse/ (fuse-native driver, options, backend, tracer),
│     exec/ (runner, env allowlist, SQLite event log), shim/ (userspace
│     disk mirror for no-/dev/fuse environments)
├── packages/computer (@cloudflare/computer) — the package DOs consume:
│     workspace.ts (owns the dofs Database; backend registry; push/pull;
│     per-backend mutation FIFOs; retryPendingSync), backend.ts (the
│     backend contract), runtime/ (router, wire codecs, capability, bridge,
│     egress), backends/ (container/, worker-shell/, worker-javascript/),
│     shell.ts (exec bracket), stub/client/with-workspace/proxy (Workers-RPC
│     surface + loopback proxies), tools/ (AI SDK tools), git/, assets/,
│     artifacts/, observe* (span observer)
├── packages/computer-computerd-linux-x64 — private FROM scratch GHCR image
│     carrying the prebuilt SEA binary (the image, not npm, is the artifact)
├── docs/01..19 — numbered spec; forward-looking per README (see §7)
├── examples/ — container, worker-shell, worker-javascript, egress, mcp,
│     think, think-compare-runtimes, tutorial, artifacts, assets, celld
└── script/ — fs-bench.sh, npm-bench.sh, soak scripts, deployed
      durable-fs bench harness (identity-pinned)
```

**Stack and deployment shape.** Worker-side: Cloudflare Workers runtime —
Durable Objects (authority), Dynamic Workers via a Worker Loader binding
(execution isolates), a Containers binding (sandbox), Workers RPC (DO↔Worker)
and capnweb-over-WebSocket (DO↔computerd). Container-side: a Node 22
single-executable binary embedding fuse-native and libfuse
(`packages/computerd/scripts/build-bin.mjs:1-130`) in a thin Debian image
(`examples/container/Dockerfile:17-42`). Production topology: one DO paired
with one container plus per-exec isolate Workers. The DO↔computerd WebSocket
is **reverse-dialed** — the container is not directly reachable, so computerd
calls out to the DO
(`packages/computer/src/backends/container/cloudflare-container.ts:290-302`;
`packages/computerd/src/cli/computerd.ts:416-536`).

## 3. Authority and data flow

**Figure 1 — deployment topology** (this study's synthesis from the code; the
subject's own diagram, `docs/assets/arch.png`, shows only the container half
and predates the `workspace.runtime` refactor — the code is the authority):

```text
                Cloudflare Workers runtime
┌─────────────────────────────────────────────────────┐
│ Durable Object = the authority                      │
│   Workspace (owns dofs Database)                    │
│     ├─ fs        → WorkspaceFilesystem (in-process) │
│     ├─ runtime   → WorkspaceRuntime router          │
│     └─ backends  → registry: id → handle (lazy)     │
│   SQLite in DO storage ← the ONLY durable state     │
│     (inode graph + sha256 chunk store, watermarks,  │
│      launch record, runtime identity, secret,       │
│      execution-runtime table, JS exec journal)      │
└──────┬──────────────────────────┬───────────────────┘
       │ Workers RPC (stub)       │ capnweb over WebSocket
       │ "no second store"        │ REVERSE-DIALED:
       ▼                          ▼ container calls out to the DO
┌──────────────┐        ┌────────────────────────────┐
│ Dynamic      │        │ Sandbox container          │
│ Worker       │        │  computerd (Node SEA bin)  │
│ ─ isolate    │        │   ├─ FUSE mount /workspace │
│   shell (JS  │        │   ├─ in-memory SQLite VFS  │
│   bash) or   │        │   └─ /bin/sh exec + log    │
│   JS module  │        └────────────────────────────┘
└──────────────┘
```

**Where state lives.** The DO's SQLite storage (platform-managed, ~10 GB
shared, vendor-documented at `docs/README.md:29`, not enforced in code) is
the only durable state: the VFS tables and sync watermarks
(`packages/dofs/src/schema/core.ts:20-80`, `schema/sync.ts:5-65`), the
container launch record and runtime identity
(`packages/computer/src/backends/container/container-launch-record.ts:38,60-69`,
`container-runtime-identity.ts:17-35`), the RPC client secret
(`container-client-secret.ts:22-41`), the execution→runtime table
(`packages/computer/src/execution-runtime-tracker.ts:16-54`), and the
worker-javascript execution journal
(`packages/computer/src/backends/worker-javascript/worker-javascript.ts:246-278`).
Everything else is process-lifetime: the capnweb session, the container's
in-memory VFS, computerd's exec logs.

**Who may write the authoritative store.** (a) In-DO callers of
`Workspace.fs` — mutations run in-process against the DO SQLite
(`packages/computer/src/workspace.ts:1-10,443-455`), serialized by the DO
runtime's input gates; (b) the sync apply path, pulling container writes
back; (c) isolate backends — they hold no store; every fs op is one RPC hop
into the DO (`packages/computer/src/backends/worker-shell/adapter.ts:151-154`,
`packages/computer/src/proxy.ts:182-228`). Container processes write only the
container-side copy.

**DO-side write path.** `writeFile` → one `transactionSync`: canonicalize →
resolve (40-follow symlink cap) → create/replace inode → chunk into fixed
512 KiB windows (`packages/dofs/src/fs/writeFile.ts:29`) → sha256 each into
`vfs_blobs`/`vfs_blob_bytes` with dedup (`writeFile.ts:298-309,514-526`) →
chunk rows + manifest → one rev bump stamped on the node
(`packages/dofs/src/rev.ts:11-21`). Streaming writes stage blobs per window
outside the transaction, then commit inode/chunks/manifest in one short
transaction (`writeFile.ts:326-401`). All file content is blob content — no
inline storage (removed by commit 541c142, 2026-06-09); only symlink targets
are stored inline on the node row.

**Container-side write path.** FUSE write → provider write buffer (in-memory
`Uint8Array`, committed once at release via `applyChunkedInodeUpdate`, which
re-hashes only touched chunks — `writeFile.ts:643-697,733-866`) → the
container's in-memory SQLite VFS, rev-stamped. Those writes reach the DO only
when the DO pulls.

**Figure 2 — the exec round trip:**

```text
runtime.exec(cmd, {backend})
  │
  ▼ [1] PUSH   DO→container: ChangeEntry stream, hashes only
  │           (objects first: hasObjects probe → pushObjects, then entries)
  │           failed push ⇒ ABORT before spawn (never run on stale state)
  ▼ [2] SPAWN /bin/sh -c "cd '<cwd>' && cmd"   (cwd not passed to spawn:
  │           uv_spawn chdir would deadlock on computerd's own FUSE)
  ▼ [3] EVENTS {id, seq} stdout/stderr/exit — live stream + SQLite replay
  │           log; backpressure propagates to the kernel pipe
  ▼ (drain)
  ▼ [4] PULL   container→DO: fetchChanges(after cursor), batches of 256,
  │           probe → fetchObjects → per-mutation idempotent apply
  ▼ [5] RESULT {status, exitCode, stdout, stderr, pushed, pulled, skipped, sync}
              └ pull failed ⇒ durable SyncRetryIntent (DO alarms), fenced to
                runtimeId; reached a replacement container ⇒ EEXEC_LOST / "lost"
```

**The exec round trip** (the load-bearing flow; `packages/computer/src/shell.ts:150-192`):
1. **Push** — DO→container: coalesced `ChangeEntry` stream (hashes only);
   objects first (`hasObjects` probe, `pushObjects` for the missing subset),
   then entries. A failed push aborts before spawn: "running with stale or
   incomplete workspace contents is not safe."
2. **Spawn** — `/bin/sh -c "cd '<cwd>' && <cmd>"`; cwd is not passed to spawn
   because `uv_spawn`'s fork+chdir would deadlock against computerd's own
   FUSE mount (`packages/computerd/src/exec/runner.ts:116-149`).
3. **Events** — `{id, seq, name: stdout|stderr|exit}` stream plus a SQLite
   replay log; backpressure is pull-based end-to-end to the kernel pipe
   (`runner.ts:346-420`).
4. **Pull** — after drain: `fetchChanges(after cursor)` in batches of 256,
   probe/fetch missing objects, apply per-mutation; idempotent via
   `alreadyApplied` (`packages/rpc/src/sync-driver.ts:81,214-283`;
   `packages/dofs/src/sync/apply.ts:291-293,563-586`).
5. **Result** — carries its own sync accounting: `pushed`, `pulled`,
   `skipped`, `sync` (`packages/computer/src/runtime/types.ts:83-102`).

**Read paths.** DO: `readFile` resolves once, snapshots the chunk list, pulls
blob bytes lazily — immutable hashes mean later writes cannot tear the range
(`packages/dofs/src/fs/readFile.ts:79-123`). Container: FUSE reads go through
`readRangeSync` over overlapping chunks with a 16-entry blob LRU
(`packages/dofs/src/fs/blobCache.ts:29,45-81`) and `max_read=max_write=512 KiB`
matching the chunk size (`packages/computerd/src/fuse/options.ts:20,65-73`).

**Mount lifecycle.** `ready()` materializes eager mounts exactly once per
store: the R2 provider streams objects in (concurrency 8), the `_vfs_mounts`
row is read-write during materialize and flips to the registered mode after;
failure rolls the subtree back (`packages/computer/src/mounts/index.ts:33-116,142-175`).

**Concurrency model.** Within a DO incarnation the runtime's input gates
serialize all handler executions; the code contains no locking,
`busy_timeout`, or WAL configuration and leans on this
(`docs/02_sync_protocol.md:310-317`; `packages/dofs/src/rev.ts:8-10`). On top,
a per-backend tail-promise FIFO serializes only push/pull and the exec
brackets; reads bypass it; rejections are non-contagious
(`packages/computer/src/workspace.ts:1026-1056`). There is **no hibernation**:
the `/api` upgrade calls `server.accept()`, not `ctx.acceptWebSocket()`
(`cloudflare-container.ts:426`), so the DO stays memory-resident for the
session's lifetime; hibernation is a forward-looking doc sketch only
(`docs/11_lifecycle.md#Hibernation`). Lazy: backend connects, mount indexing
(once), isolate warm-up (shell), result-side streams.

## 4. Implementation details

Each mechanism is flagged **[implemented]** (verified in code at this pin) or
**[spec-only]** (doc-described, absent in code).

### 4.1 Storage model

- Schema v6 (`packages/dofs/src/schema/core.ts:16-17`): `vfs_nodes` (inode,
  type, mode, mtime, rev, size, manifest_hash, link_target, mount_root),
  `vfs_dirents` (parent,name)→child **WITHOUT ROWID** (covering resolve read),
  `vfs_blobs`+`vfs_blob_bytes` (sha256 store, FK cascade), `vfs_chunks`
  (inode,idx)→(hash,size) WITHOUT ROWID; six ordered in-transaction migrations
  (`schema/migrations.ts:163-188`). **[implemented]**
- Content addressing: fixed 512 KiB windows with deterministic boundaries
  (`chunkIdx = floor(offset/CHUNK_SIZE)`), sha256 per chunk; identical content
  at any number of paths stores and transfers once
  (`docs/02_sync_protocol.md#Chunking`; `writeFile.ts:410-421`). Fixed-size,
  *not* content-defined — their own doc defers CDC (FastCDC/buzhash) and names
  the head-insertion cost (`docs/02_sync_protocol.md:509-514`). **[implemented]**
- Three per-`Database` WeakMap caches — resolve cache (LRU 8192,
  `resolveCache.ts:38`), 16-entry blob LRU, open-file write buffers — all lost
  on DO restart by design (`writeBuffer.ts:9-11`). **[implemented]**
- GC exists (`gc.ts:18-55`, 1-hour window, blobs+manifests only) but has no
  production caller and is not exported from the package index; `vfs_changes`
  tombstones are never pruned anywhere in the repo. **[implemented, dormant;
  growth unbounded and documented]**

### 4.2 Revision model and change capture

- One atomic rev counter in `vfs_meta`; each mutation transaction bumps it
  once and stamps it onto touched nodes; deletes append per-path tombstones
  (`rev.ts:11-21`; `sync/changes.ts:9-11`). Multi-node transactions (recursive
  rm, directory rename) share one rev. **[implemented]**
- The change stream is **derived, not stored**: `coalesceChanges` range-scans
  `vfs_nodes.rev > cursor`, unions tombstones, keeps the latest rev per path
  (live beats tombstone on delete-then-recreate), materializes each path's
  *current* state — the store keeps no content history (`sync/coalesce.ts:35-130`).
  **[implemented]**
- No rename opcode: the wire is final-state (entries + tombstones); a
  directory rename is a subtree restamp with per-old-path tombstones in one
  transaction (`fs/rename.ts:138-152,217-229`). A rename opcode and
  cross-revision chunking were considered and rejected with written rationale
  (`docs/02_sync_protocol.md#Alternatives-considered`). **[implemented]**
- `ChangeCursor = {rev, path|null}` is a resume point, not a snapshot handle;
  a path rewritten after stream open is dropped and redelivered under a later
  cursor, and the cursor never advances past the rev that would redeliver it
  (`docs/02_sync_protocol.md:159-175`). **[implemented]**
- Hardlinks fan out to one entry per name; identity is not preserved across
  the wire (`docs/02_sync_protocol.md:41-46`). **[implemented]**

### 4.3 Sync wire

**Figure 3 — the consistency model:** watermarks with a wire-checkable
invariant.

```text
DO SQLite (authority)                    container in-memory VFS
  currentRev   ──┐                        ┌──  currentRev
  pushRev      ──┤── push ───────────────▶│    appliedPushCursor
  fetchCursor  ──┤◀─ fetchChanges ────────│      (echoed on EVERY response)
                 │   hasObjects/fetchObjects (haves/wants by hash)
                 ▼
   assert appliedPushCursor ≥ {pushRev} on every push AND pull —
   the "receiver is caught up" invariant is inspectable on the wire
```

- `SyncRPC`: `push`, `fetchChanges`, `watermarks`, `readEntry`, `hasObjects`,
  `fetchObjects`, `pushObjects` (`packages/rpc/src/interface.ts:22-103`).
  Entries never carry bytes; file entries carry `(hash,size)[]`; the receiver
  probes (git-haves style) then pulls bytes by hash. **[implemented]**
- Cross-side invariant: every push/fetchChanges response echoes the
  receiver's `appliedPushCursor`; the sender asserts it covers its `pushRev`,
  and a violation throws and rebuilds the connection (`sync/invariant.ts:18-28`;
  assert sites `sync-driver.ts:204,387,573,675`). **[implemented]**
- Push applies atomically on the receiver (whole batch in one reentrant
  `transactionSync`); pull is per-mutation with a per-batch cursor checkpoint
  — a crash mid-pull re-fetches at most 256 entries, absorbed idempotently
  (`packages/rpc/src/server.ts:132-146`; `sync-driver.ts:272-278`). **[implemented]**
- Budgets: driver batches 256 entries / 4 MiB; apply-side 64 MiB / 1024-path
  caps are advisory (`apply.ts:48-76`); object probes batch 100 hashes (DO
  SQLite caps bound parameters at 100 — `sync/fetch.ts:38`). **[implemented]**
- `senderRev > 0` marks a sync peer; `senderRev === 0` marks an external
  writer/fresh receiver, applied as `local` so outbound sync is not silenced
  (`docs/02_sync_protocol.md:244-262`). Ignore lists: opt-in, whole-segment,
  wire-only filters; filtered entries still advance the cursor
  (`sync/ignore.ts:13-24`). **[implemented]**

### 4.4 Conflict semantics

Last-writer-wins at sync granularity with **no detection, merge, or error**:
two containers writing one path converge to whichever batch applies last;
type mismatches replace the local node tree, discarding local-only children
without tombstones; object-level integrity holds (never a partial apply),
content-level does not (`docs/02_sync_protocol.md#Conflict-semantics`;
`apply.ts:86-161`). The doc is unusually explicit about when LWW is and is
not safe (shared files, read-modify-write counters, handoffs without explicit
`pull()`), concluding: "good at durable per-agent storage. It is not (yet) a
shared CRDT." **[implemented]**

### 4.5 Failure and recovery

- Transport failures are classified conservatively (own error classes;
  name matching that survives structured clone; bounded cause-chain walk —
  `packages/computer/src/transport-failure.ts:33-116`); each logical operation
  gets **one** reconnect retry. Sync ops replay freely (watermarks advance
  only after committed work); command spawn replays only when provably
  pre-dispatch — otherwise the error states "the command may have started and
  was not replayed" (`workspace.ts:944-1024,1312-1341`). **[implemented]**
- Every (re)connect runs `reconcileWatermarks`: if the remote is behind the
  DO's durable cursors (e.g. computerd restarted), the divergent cursor resets
  to rev-0 and the next push re-baselines the fresh container VFS
  (`sync-driver.ts:736-775`). Container-local files never pulled before the
  process died are unrecoverable (`docs/07_injected_service.md:210`).
  **[implemented]**
- Failed post-command pulls become a durable retry: `SyncRetryIntent`
  persisted via a host scheduler (DO alarms), exponential backoff 1 s→60 s,
  5 attempts; budget hits reset the failure count so large trees drain across
  alarm turns; a retry reaching a *replacement* container reports `lost` via
  `EEXEC_LOST` rather than treating the empty VFS as a successful zero-entry
  pull (`workspace.ts:64-84,714-797,1484-1496`). **[implemented]**
- Container adoption: a running container is reused only on an exact durable
  launch-record digest (env + internet flag); mismatch or prior exit →
  destroy and relaunch (`container-host.ts:143-202`). Runtime identity is a
  UUID per launch — reconnect keeps it, replacement gets a new one
  (`container-runtime-identity.ts:17-35`). Health: a 20 s `watermarks()`
  heartbeat closes wedged sockets (`packages/computer/src/heartbeat.ts:25-42`);
  exec logs retain 5 min / 16 MiB per exec with `ELOG_TRUNCATED` on eviction
  (`packages/computerd/src/exec/log.ts:95-193`). **[implemented]**

### 4.6 Backend registry and runtime

**Figure 4 — one execution entry point, pluggable backends:**

```text
workspace.runtime.exec(source, { backend })
        │
        ▼
  WorkspaceRuntime (single router; id resolved, first-registered default)
        ├─ container-shell    command backend · sync:"remote" · capnweb WS
        ├─ worker-shell       command backend · sync:"none"  · warm Dynamic Worker
        └─ worker-javascript  module backend  · fresh Dynamic Worker + journal
```

- The backend contract is deliberately narrow: `id` (user-supplied selector;
  duplicates rejected), `type` (diagnostic only), `callable?`, and
  `connect(host)` returning a handle — "Backends do not see method
  signatures; they only produce handles and tear them down"
  (`packages/computer/src/backend.ts:16-19,38-70`). The handle declares `rpc`,
  optional `runtimeId`, `sync: "remote"|"none"` (worker isolates are `"none"`:
  the host store is their only store; push/pull are no-ops), optional
  `closed`, idempotent `close()`. **[implemented]**
- Two backend protocols behind one registration shape: command backends
  (`WorkspaceBackend` → `WorkspaceRPC` stub) and module backends
  (`WorkspaceModuleBackend`, `protocol: "module"`), both routed through the
  single `WorkspaceRuntime` (`workspace.ts:375-397`;
  `runtime/types.ts:186-201`). Default ids `container-shell`,
  `worker-shell`, `worker-javascript`; first registered is the default; a
  no-backend Workspace is filesystem-only. **[implemented]**
- The exec handle is a single-consumer `ReadableStream` with attached `id`,
  `result()`, `kill()`, `[Symbol.dispose]`; `result()` and streaming are
  mutually exclusive; utf8 decoding re-sequences flushed partial chunks with
  fractional `seq` so ordering stays monotonic; exit codes 129/130/137/143
  map to status `cancelled` (`runtime/runtime.ts:138-355`). A 1024-entry LRU
  plus a SQLite `computer_execution_runtime` table fence by-id operations to
  a runtime UUID across DO incarnations (`execution-runtime-tracker.ts:16-99`).
  **[implemented]**

### 4.7 Container backend

- Reverse-dial boot: arm the upgrade slot → health-probe → auth-enforcement
  check (an unauthenticated `GET /api` must 401, recycling images that
  predate the secret) → `POST /connect` → computerd dials out
  (`cloudflare-container.ts:228-298,596-623`; `computerd.ts:416-536`). Auth is
  a durable per-DO 128-bit bearer secret, enforced on every route except
  `/health`, deleted from `process.env` after boot so exec'd commands cannot
  leak it (`container-client-secret.ts:22-41`; `computerd.ts:103-129,584-585`).
  **[implemented]**
- FUSE write model: "the byte owner is DOFS, not the FUSE driver" — with the
  provider's buffered-write surface, `create`/`open`/`write`/`release` map
  onto dofs write buffers with no per-file staging in computerd
  (`packages/computerd/README.md:50-74`; `fuse/driver.ts:259-276,545-707`);
  per-file 256 MiB cap → `EFBIG`; a legacy staged-buffer fallback exists.
  Mount options are monkey-patched onto libfuse 2.9: `big_writes`,
  `max_write=max_read=524288`, `auto_cache` default (`kernel_cache` is
  documented unsafe because DO pushes change bytes under open fds —
  `fuse/options.ts:32-109`; `packages/computerd/bench-results.md:80-97`).
  **[implemented]**
- The shim (no `/dev/fuse`): a bidirectional userspace mirror — VFS→disk on
  provider watch, disk→VFS on a 250 ms reconcile poll with a shadow-tree echo
  guard; `afterApply`/`beforeFetch` settle hooks keep push/exec/pull coherent
  (`shim/shim.ts:93-446`; `computerd.ts:661-679`). **[implemented]**
- Exec env is a strict allowlist (`PATH/HOME/TMPDIR/TZ/LANG/TERM` + `LC_*` +
  operator `COMPUTER_VAR_*`); timeout default 320 s → SIGTERM → 5 s grace →
  SIGKILL (`exec/env.ts:15-48`; `runner.ts:56-64,202-246`). **[implemented]**

### 4.8 Isolate backends

- **Worker shell [implemented]:** one warm Dynamic Worker per (workspace,
  egress-policy) identity via `env.LOADER`; just-bash is a JS interpreter —
  no real processes; output buffered, emitted as ≤3 events (stdout, stderr,
  exit); wall-clock cooperative timeout only (no CPU limit); `getExec` always
  throws ENOENT — no reattach (`backends/worker-shell/worker-shell.ts:229-254`;
  `entrypoint.ts:176-315`). ~79 core commands (~1.75 MB) plus opt-in groups
  (curl, jq, yq, xan, sqlite, file, html-to-markdown), chunk-partitioned so
  unimported groups tree-shake away — but a registered name whose group was
  not imported fails at lazy load (`script/build-bundle.mjs:66-76`). The
  `python`/`js-exec` groups ship but cannot run under workerd
  (worker_threads dependency — `entrypoint.ts:129-131`). Built-ins
  `git`/`assets`/`artifacts` forward over the loopback to host-side clients,
  so network git works even with ambient egress blocked (`git-command.ts:76-109`).
  Adapter gaps documented: `link` throws ENOSYS, `mv` is cp+rm, not atomic
  (`adapter.ts:197-243`).
- **Worker JavaScript [implemented]:** a fresh Dynamic Worker per execution
  with `limits: {cpuMs: timeoutMs}`; an acorn module graph (≤128 modules,
  depth ≤32, string-literal dynamic imports, unknown bare imports rejected
  before Worker creation); `node:fs/promises` is generated code forwarding
  through a symbol-keyed global to a capability bridge confined under a root
  (lexical path checks, no symlink traversal, 1 MiB read / 1024-entry caps,
  `w`/`wx` flags only) with per-execution budgets (1 MiB payload, 30 s per
  call, 256 calls / 8 MiB totals) (`worker-javascript.ts:926-963`;
  `module-graph.ts:29-156,314-372`; `runtime/capability.ts`;
  `runtime/bridge.ts:8-64,88-222`). `ws:git`/`ws:artifacts` are narrowed
  host-authority proxies gated by `allowGitNetwork`/`allowArtifactNetwork`
  (`bridge.ts:243-358`). Executions journal into the Workspace's own SQLite
  for replay (`getExec({after})`), defaults 24 concurrent / 60-min retention /
  100 retained, `EEXEC_BUSY` admission, cancel-and-drain so an unawaited host
  call cannot mutate the workspace after exit 0, and restart recovery
  reconciles orphaned `running` rows to failed
  (`worker-javascript.ts:246-298,333-338,458-540,857-897`). The code defaults
  contradict the file's own JSDoc (24 vs "1", 60 min vs "five minutes" —
  `worker-javascript.ts:48,52` vs `:176,178`); docs/17 matches the code.

### 4.9 Mount subsystem, egress

Eager-only: `Mount = EagerMount` (`mounts/types.ts:36-52`); the only built-in
provider is R2 — streaming, read-only "in this milestone," indexed exactly
once per store (objects landing after the first index are never picked up)
(`mounts/providers/r2.ts:8-16`). Read-only roots are enforced at the *data
layer* — every dofs mutator and the sync apply path throw/skip EROFS, and
pulls surface `SkippedEntry` records instead of failing
(`fs/mount-guard.ts:54-86`; `sync/apply.ts:21-34,294-364`). **[implemented]**
Lazy stubs, GitHub providers, write-back, and mount conflict policies remain
**[spec-only]** (`docs/06_mount_interface.md` carries its own divergence
banner). Egress: one policy, three modes — `none` (`globalOutbound: null`),
`direct`, `http-gateway` (all egress rewritten with token headers, proxied
through a gateway Fetcher) (`runtime/egress.ts:1-19`;
`cloudflare-container.ts:379-400`). **[implemented]**

### 4.10 Lifecycle summary

DO incarnation: only SQLite survives. Container: adopt-or-relaunch by launch
record; per-generation exit monitors; teardown closes HTTP → SIGKILLs live
children → unmounts FUSE (`computerd.ts:685-709`). Session: reconnect replaces
the whole capnweb session, never splices a carrier. Exec: dual-layer records
(daemon SQLite log; DO journal for JS isolates). `close()` bumps a connection
generation, drops caches, then closes handles in parallel
(`workspace.ts:1075-1101`). **[implemented]**

## 5. Interfaces

**`workspace.runtime.exec(source, options)`** — the single execution entry
point. Options: `backend` (id; first-registered default), `id` (≤256 bytes),
`cwd`, `encoding: "utf8"`, `input` (structured; callable backends only),
`env`, `stdin`, `timeoutMs`, `sync: "wait"|"defer"` (`runtime/types.ts:104-114`).
`getExec(id, {resume: "tail"|"full"|seq})` reattaches; `killExec`;
`disposeExec`. **[implemented]**

**`Workspace.fs`** — 13 async methods (stream-or-utf8 `readFile` with byte
ranges, `stat/lstat/readlink`, paginated `readdir`, `find` (glob), `ls`,
`grep`, `writeFile` (string/bytes/stream, `exclusive`), `mkdir`, `rm`,
`chmod`, `symlink`) over POSIX-style errors: `WorkspaceFsError` with a
14-value `code` union and a `path` field (`packages/dofs/src/errors.ts:1-26`).
`rename`, `watch`, and hardlink `link` are not on the class but reachable via
`workspace.provider()`, the node:fs-shaped surface for FUSE and isomorphic-git
(`workspace.ts:540-545`). Across Workers RPC, `WorkspaceFilesystemStub`
mirrors the class one-for-one plus `exists`/`statOrNull`/`lstatOrNull`
conveniences (`stub.ts:82-174`). **[implemented]**

**Workspace-level API** — `push()`/`pull()` (per-backend; batch options
`{maxEntries: 64, maxBytes: 4 MiB}`), `ready()` (mount indexing + optional
backend pre-warm), `stub()` (Workers-RPC wrapper with pipelined sub-stubs),
`close()`, and `retryPendingSync()` returning the 5-arm union
`idle|complete|pending|exhausted|lost` (`workspace.ts:97-117`). **[implemented]**

**Tools** — `createAITools`: always `read`/`ls`/`find`/`grep`; `readonly:
true` stops there; else `write`/`edit`/`delete`; `exec` only when shell
options are passed; `publish` when assets are configured (`tools/ai.ts:23-50`).
Vercel AI SDK `tool()` + zod, both optional peer dependencies. Notable
shapes: `read` caps (2000 lines / 256 KiB / 3.5 MiB inline model bytes) with
`nextOffset`/`nextByteOffset` continuations and multimodal capture *during*
execution so regenerated history cannot observe later file changes
(`tools/fs/read.ts:24-27,283-357`); `edit` applies batches against original
content with a fuzzy-lookup fallback that never mutates matched bytes;
cross-tool path locking via an in-process WeakMap (`tools/fs/locks.ts:13-63`).
**[implemented]**

**Observe** — a span-wrapping hook (`WorkspaceObserver.span(name, attrs, run)`),
not an event stream; zero-cost no-op default; one shipped adapter
(`ctx.tracing`); error messages redacted (authorization/token/api-key/
password/secret/cookie, `Bearer …`) and truncated to 512 chars
(`observe.ts:101-134,200-216`). Spans cover connect, sync push/pull, exec
spawn, and fs ops *only through the stub boundary* — direct in-DO calls emit
nothing. **[implemented]**; the "OpenTelemetry adapter" promised in comments
is **[spec-only]**.

**git / assets / artifacts** — `workspace.git`: opt-in isomorphic-git client
over the VFS (29 subcommands; typed API and argv CLI over one core; `.git` is
ordinary workspace files with no sync special-casing anywhere; pako aliased
to a `node:zlib` shim at build time) (`git/`; `rolldown.config.ts:13-15,77`).
**Assets**: `share(path, {expiresAfter ≤ 7 days})` streams to R2 via
`FixedLengthStream` and returns a hand-rolled SigV4 presigned GET URL; keys
hide all but the basename (`assets/`). **Artifacts**: `createArtifact` wraps
the Artifacts binding with `${sessionId}__name` prefix scoping (empty session
id is an error, never silently unscoped) and a CLI whose `create` composes
repo + token + git remote via an injected seam (`artifacts/`). Portability
gradient: git nearly portable, assets half, artifacts entirely
binding-specific. **[implemented]** (docs 13–15 match code at this pin).

**`ws:git` / `ws:artifacts`** — narrowed authority-gated proxies inside the
JS isolate (§4.8). **[implemented]**

## 6. What LayerFS can learn

Recommendations handed to the v0.1.7 checklist, not decisions.

### 6.1 Concept map

| Cloudflare Computer | Closest LayerFS concept | Notes |
| --- | --- | --- |
| Durable Object (authority) | `LayerStackStore` (local SQLite) | both "one durable SQLite"; CC's is platform-hosted, LFS's file-owned |
| DO SQLite (platform `SqlStorage`) | Store SQLite (`MEMORY`/`OFF`/`EXCLUSIVE`) | LFS disables journaling for speed; the platform owns CC's durability |
| mutable `vfs_nodes`/`vfs_dirents` graph | immutable tree objects + extent trees | CC mutates rows in place; LFS never updates canonical bytes |
| `vfs_blobs`/`vfs_chunks` (sha256, fixed 512 KiB) | canonical objects + CDC chunking | same dedup idea; LFS's CDC is finer — CC's own doc defers CDC |
| rev counter + derived change stream | *no equivalent* | LFS has content identity + Branch-head CAS, no mutation-log counter |
| `vfs_changes` tombstones (never pruned) | *no equivalent* | LFS never deletes objects in normal operations |
| Workspace (CC) = durable fs + runtime | ≈ Store + Branch surfaces | naming trap: CC's "Workspace" is durable; LFS's is the ephemeral view |
| container FUSE mount (synced copy) | Workspace FUSE projection | CC: batched sync around exec; LFS: live proxy + bounded spool |
| `SyncRPC` push/pull + watermarks | `layerfs-fuse` proxy transport | same problem (container view vs authority), different consistency model |
| `WorkspaceBackend`/`BackendHandle` registry | `layerfs-daemon` container execution | CC pluggable behind one entry point; LFS execution is daemon-owned |
| `workspace.runtime.exec` | SDK Exec/Shell | one router, typed wire, reattach/kill/dispose |
| `computerd` (mount + exec + sync, one binary) | daemon (control) + FUSE helper (projection), separate | CC bundles; LFS separates deliberately (fresh helper per Workspace) |
| reverse-dial + durable bearer secret | authenticated control daemon | same unreachable-sandbox problem, opposite dial direction |
| `runtimeId` UUID + `EEXEC_LOST` | *no equivalent* | exactly the "same runtime?" fencing 0.2 resumable work needs |
| `SyncRetryIntent` → `idle/complete/pending/exhausted/lost` | 0.2 Proposal outcomes (design terms) | closest analogue to 0.2's planned typed outcomes |
| last-writer-wins conflicts, no detection | CAS Branch head; 0.2 reconciliation | CC punts; 0.2 makes this the release-defining task |
| exec event stream (`id`/`seq`) + replay log | *no equivalent* | LFS captures final state at Commit; no exec observability contract |
| eager R2 mounts + EROFS data-layer guard | *no equivalent* | external prefill is not in LayerFS's model |
| GC (dormant) / tombstone growth | compaction removed 0.1.5; append-only objects | both accept growth; LFS documents reuse instead |
| ignore lists (sync-only filters) | *no equivalent* | LFS dirty frontier is explicit |
| observe spans (no-op default) | Monitor typed receipts | LFS receipts are outcome-typed; CC's are latency-oriented |
| `createAITools` agent tools | *no equivalent* | LayerFS owns no tool surface |
| git as a client over the VFS | LayerFS explicitly does not own Git | boundary confirmation, both directions |
| assets/artifacts publishing | *no equivalent* | vendor-binding-specific surfaces |
| schema v6 + in-transaction migrations | frozen v10 schema + connect-time preflight | both stamp a version and refuse forward-incompatible stores |
| ~10 GB platform cap | local disk | scale frame: "agent workspaces, not monorepos" |

### 6.2 Observations and verdicts

1. **A strict authority/projection split with rebuild-everything recovery.**
   One DO SQLite is the only durable state; every other component is
   reconstructible from cursors — computerd restarts re-baseline from rev 0,
   containers are adopted only on an exact launch-record digest, sessions are
   replaced whole. LayerFS already holds this principle; the lesson is the
   *recovery discipline*: durable state + cursors must suffice to rebuild
   every projection, and reconnect must re-derive rather than trust — the
   same shape [cloud-sqlite-vfs.md](../../../../0.2/cloud-sqlite-vfs.md)
   Stage 3 wants, demonstrated small. **Adopt the principle; it constrains
   v0.1.7's seam placement (nothing durable may live in a projection).**

2. **A narrow, honest backend contract.** `backend.ts` is 105 lines: backends
   register an id and produce handles; the handle declares whether it owns a
   store (`sync: "remote"|"none"`) and whether its transport can drop. Two
   backend protocols hide behind one registration shape; the runtime has one
   execution path. v0.1.7 can create this seam internally (registry +
   lifecycle + sync-mode declaration) without touching public SDK/CLI
   behavior. **Adopt — as internal structure for v0.1.7, not yet a public
   extension point.**

3. **Push-before-spawn as a safety gate; durable deferred sync fenced to a
   runtime identity.** A failed pre-exec push aborts execution rather than
   running against stale state; a failed post-exec pull becomes a durable
   retry fenced to the container's runtime UUID with terminal outcome `lost`.
   LayerFS's FUSE projection is live (no push needed), but the pattern —
   bracket executions with sync, make the deferred part durable, fence it,
   expose a typed outcome union — transfers directly to 0.2's
   pending-Proposal reconnect/crash-recovery requirements.
   **Borrow-with-changes.**

4. **State-based, content-addressed sync with haves/wants negotiation.** No
   operations on the wire — final state plus tombstones; `(rev, path)` cursor
   as a resume point; per-path coalescing; bytes move only by hash after a
   probe. The rejected-alternatives record (`docs/02#Alternatives-considered`)
   is a model decision record. This validates the batch-shaped
   `ObjectRead`/`ObjectStore` seam in cloud-sqlite-vfs §2(c): a remote
   projection syncs by identity sets, not operations. Do not copy the fixed
   512 KiB chunking (LFS's CDC is finer) or the history-less store.
   **Borrow the negotiation shape; avoid the history-less change stream.**

5. **LWW conflicts documented with unusual honesty.** The conflict-semantics
   section states plainly that concurrent writers lose data silently, lists
   the unsafe patterns, and refuses to pretend the workspace is a CRDT.
   LayerFS's 0.2 direction (automatic reconcile or structured resumable
   conflict work) is the opposite commitment — this study confirms the 0.2
   task is real work, not gold-plating. **Not-applicable as semantics; adopt
   the honesty as documentation practice.**

6. **Wire-checkable invariants.** The receiver echoes `appliedPushCursor` on
   every push and fetch; the sender asserts coverage — the "receiver is
   caught up" invariant is inspectable on the wire instead of load-bearing
   in-process state, and a regression trips an assertion instead of
   corrupting silently. LayerFS's daemon protocol and verification receipts
   could carry the same echoed-cursor discipline. **Adopt.**

7. **Single-DO concurrency as a platform property.** CC contains no locking
   code at all — the platform serializes; on top there is exactly one
   application-level serializer (the per-backend mutation FIFO) with reads
   bypassing it. This is what cloud-sqlite-vfs predicts for D1-A: once the
   Store is hosted, the one-writer invariant becomes a server property that
   must be auditable — CC's cursors-plus-echoed-assertions is the pattern.
   **Not-applicable locally today; borrow for any hosted-store design.**

8. **Container daemon details worth stealing piecemeal.** Replay-safety
   policy ("may have started, was not replayed" — never double-execute on
   ambiguous dispatch); runtime-identity UUID separating reconnect from
   replacement; durable (not per-connect) secrets so a restarted DO still
   matches a live container; fail-closed checks that the peer actually
   enforces auth; exec env allowlists; `cd`-quoting to dodge the uv_spawn/FUSE
   deadlock; mount-before-listen so `/health` implies FUSE. Each maps onto a
   `layerfs-daemon`/`layerfs-fuse` concern. **Borrow-with-changes,
   selectively.**

9. **Execution observability as a first-class contract.** Every exec has an
   id and monotonic seq, streams live, replays from a log, and settles a
   typed result carrying its own sync accounting. LayerFS's Exec runs a fresh
   process per execution and captures state only at Commit. If 0.2 makes
   Workspace = one tool call, an execution record with reattach and
   outcome-fenced retry is the shape of "resumable structured conflict work"
   at the execution layer. **Borrow-with-changes for 0.2, not v0.1.7.**

10. **Isolate backends are a cheaper execution class, not a replacement.**
    `sync: "none"` isolates call the authoritative store over RPC with zero
    projection — the filesystem is never copied. This works because their fs
    surface is async JS; arbitrary processes need a real filesystem, which is
    LayerFS's case. The transferable idea for 0.2's "highly concurrent,
    disposable" Workspaces is the authority declaration on the backend —
    "no second store" as a one-bit contract. **Borrow the contract bit;
    not-applicable as an execution class.**

11. **Numbered specs drift in both directions unless status banners are
    maintained.** At this pin the README marks mounts "(not yet implemented)"
    while an eager R2 mount ships; docs/03's DDL lags schema v4–v6; docs/04
    denies methods that exist; docs/12 undercounts built-ins; two docs cite
    commits absent from the local history. Docs 02 and 06 partially self-flag,
    and doc 11 marks hibernation forward-looking — accurately. The lesson for
    LayerFS: a spec series needs per-document status labels updated with the
    code, or it inverts from description to folklore. **Borrow-with-changes —
    numbered docs only with living status banners.**

12. **Performance honesty and its structure.** Vendor-reported numbers with
    stated conditions, committed reproduction scripts, and design-attributed
    costs: the write path "hashes each CHUNK_SIZE (512 KiB) chunk into a
    content-addressed blob store on every release" — so metadata-heavy work
    beats ext4 (stat 0.91×, rm 0.66×, find 0.72×, git-init 0.72×) while
    sequential 64 MiB I/O is 17–43× slower and a full `npm install` is ~2×
    ext4 (`docs/19_performance.md:15-68`). The corroborating insight is
    structural: content addressing on the hot write path costs exactly the
    sequential-I/O budget, and metadata-vs-bytes splits are the honest way to
    report it. **Adopt the reporting shape (already LayerFS policy); treat
    the numbers as corroborating context for LayerFS's own commit-phase
    costs, never as independent measurements.**

### 6.3 What not to borrow

- The **mutable in-place inode graph with no content history** — LayerFS's
  immutable snapshots and append-only objects are the product; CC's store
  cannot answer "what did this tree look like at rev N".
- **Unbounded tombstones and dormant GC** as growth characteristics.
- **LWW conflict semantics** anywhere near Branch integration.
- **`kernel_cache`-style projection caching** that lets authority changes
  bypass open file handles — CC itself documents it as unsafe under push.
- **In-code defaults contradicting their own JSDoc** (two instances found) —
  v0.1.7's refactor must keep doc-comments mechanically tied to the code.

## 7. Unknowns

Could not be verified from the snapshot; recorded rather than guessed:

- **Platform semantics behind vendor claims:** DO input-gate serialization
  and `transactionSync` atomicity (asserted by docs and code comments, not
  provable from this repo); Worker Loader isolate caching and `limits.cpuMs`
  enforcement; Dynamic Worker OOM isolation; workerd cross-request I/O guard
  history; whether the ~10 GB cap is enforced anywhere (no in-code check
  found).
- **Snapshot provenance:** the local git history is authored by the snapshot
  owner rather than upstream; docs cite commits (`dc692c0`, `c952c4d`) that
  do not resolve in the local history, and the deployed-bench script pins a
  different upstream commit (`de87919a`). File contents were studied as-is;
  whether the tree is a byte-faithful upstream import is unverifiable from
  inside the snapshot.
- **capnweb internals:** exact wire framing and binary-frame rejection
  behavior are library-internal; only the API boundary was verified.
- **`file://` git transport:** the CLI advertises it, but no file transport
  is registered anywhere — likely broken; unverified.
- **Miscellaneous:** whether `exec-wire.ts` (exported, unused in-repo) has
  external consumers; whether the GHCR tag `0.2.1` matches this source tree;
  the tree-shaking claim is verified by packaging construction, not by a
  bundle measurement; exact runtime error text for dropped optional shell
  commands; the keep-alive alarm referenced in `health-probe.ts:7` has no
  in-tree call site.

**Doc-vs-code drift inventory** (stale docs, all directions, at this pin):
README index marks mounts "(not yet implemented)" though an eager R2 mount
ships; docs/01 and docs/04 deny EROFS ("no production call site throws it")
though every mutator throws it; docs/03's DDL blocks lag schema v4–v6 and
deny symlink/chmod methods that exist; docs/05 omits the `sync: "defer"`
option; docs/08 marks the shipped per-backend FIFO "(planned)" and shows
stale signatures; docs/12 undercounts built-in commands and lists
`python`/`js-exec` groups that cannot run under workerd. Docs 19 and the two
bench documents are internally consistent. Doc 02's header sets the correct
discipline: "When code and doc disagree, code wins."

## 8. Source index

Consulted at pin `64c462b` (paths relative to the snapshot root):

- **Top level:** `README.md`, `AGENTS.md`, `docs/README.md`,
  `docs/assets/arch.png`.
- **Docs:** all of `docs/01`–`docs/19` audited; read in full: 02, 19, and
  `docs/README.md`; section-level reads: 01, 03–18.
- **dofs:** `src/schema/*`, `src/{storage,types,rev,provider,testing}.ts`,
  `src/fs/*` (filesystem, writeFile, readFile, resolve, resolveCache,
  writeBuffer, pendingWriteBuffer, blobCache, mount-guard, gc, watch, rm,
  rename, readdir, stat, find, grep, ls, symlink, chmod, unlink, link,
  mkdir, errors), `src/sync/*` (changes, coalesce, apply, blobs, watermarks,
  invariant, ignore, fetch, push, manifests).
- **rpc:** `src/{interface,server,client,sync-driver}.ts`, `README.md`.
- **computerd:** `README.md`, `bench-results.md`, `src/cli/computerd.ts`,
  `src/fuse/*`, `src/shim/shim.ts`, `src/exec/*`, `scripts/{build,build-bin,
  build-docker}.mjs`, `scripts/sea/*`.
- **computer:** `src/{workspace,backend,client,index,stub,shell,sh,
  with-workspace,proxy,proxy-stub,heartbeat,retry,transport-failure,
  exec-wire,execution-runtime-tracker,observe,observe-recorder}.ts`,
  `src/runtime/*`, `src/backends/{container,worker-shell,worker-javascript}/*`
  (incl. a `generated/` survey), `src/tools/**`, `src/git/*`, `src/assets/*`,
  `src/artifacts/*`, `src/mounts/**`, `src/observe/cloudflare.ts`,
  `package.json`, `rolldown.config.ts`.
- **Examples:** container (Dockerfile, src), worker-shell, worker-javascript,
  egress, mcp, think, artifacts, assets (selections).
- **Scripts (read only, never run):** `script/fs-bench.sh`,
  `script/npm-bench.sh`, `script/computerd-soak.mjs`,
  `script/computerd-stub-soak.mjs`, `script/deployed-durable-fs-bench.mjs`,
  `script/local-durable-population.mjs`.
