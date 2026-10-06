# 01 — Architecture

> **Status:** Current planning checklist; no release candidate exists.
> Part of the [Phase 7 packet](README.md). Proposal; nothing here is implemented.
> The component split in §3 was accepted by the owner on 2026-10-03.

> **Superseded 2026-10-05.** Components, ports and data paths are replaced by
> [`../303/01-architecture.md`](../303/01-architecture.md) and
> [`../303/06-cluster-one-integration.md`](../303/06-cluster-one-integration.md).
> The premise that any daemon can reach the global stores no longer holds. The
> text below is unchanged and is kept as context.

## 1. Premise: immutability removes the coordinator

Phase 4.5 routes construction through a host service (`layerfs-server`) because
one process owned the Store. Phase 6 showed that this is unnecessary. The reason
is a property of the objects, not of the experiment:

| Thing | Mutable? | Identity | Consequence |
| --- | --- | --- | --- |
| CDC chunk, whole-file payload | No | Canonical hash (C1) | Any daemon may build it; duplicates are the same object |
| Pack body | No | Digest of its bytes | Upload is put-if-absent; a retry or a race is harmless |
| File tree, directory, inode leaf | No | Canonical hash (C1) | Selected by root, never updated in place |
| Delta record | No | Depends on an immutable base | Base reachability is the only lifetime question |
| Layer, Commit | No | History identity (C5) | Append-only |
| Locator (object → pack position) | Insert-only | Object identity | First valid locator wins; later ones are ignored |
| **Branch head** | **Yes** | Branch identity | The single coordination point: one conditional update |
| Inode-serial reservation | Yes (counter) | LayerStack | Existing C5 `reserve_inodes`; a range is taken, then used locally |
| **Live Workspace overlay** | **Yes** | Workspace incarnation | Private to one daemon; disposable; never shared |

Everything expensive (chunking, hashing, compression, delta selection, packing,
upload) operates only on rows 1–6 and therefore runs wherever the bytes are — in
the daemon — with no shared lock. Everything that needs agreement is rows 7–8 and
fits in a short database transaction. The overlay (row 9) is the only place
in-place mutation exists, and it is local.

## 2. Three stores, one role each

```text
                    ┌──────────────────────── daemon (one per sandbox) ────────────────────────┐
 command ─syscall─> │ FUSE ──> Workspace ──> Overlay DB (SQLite, local, disposable)            │
                    │              │           metadata rows + payload block rows, by generation│
                    │              │                                                           │
                    │              └─ Commit: C1 construct ─> C2 encode/pack ──┐               │
                    └──────────────────────────────────────────────────────────┼───────────────┘
                                                                               │
                         file-content packs (immutable)                        │   metadata bodies, locators,
                    ┌──────────────────────────────┐                           │   history, Branch heads
                    │ S3-compatible object store   │ <──── put-if-absent ──────┤
                    │ (MinIO first)                │ ───── range / whole GET ──┤
                    └──────────────────────────────┘                           │
                    ┌──────────────────────────────┐                           │
                    │ PostgreSQL (off-the-shelf    │ <──── short transactions ─┘
                    │ server; no LayerFS service)  │
                    └──────────────────────────────┘
```

| Store | Holds | Never holds | Lifetime |
| --- | --- | --- | --- |
| **Overlay DB** (daemon SQLite) | Live names, inodes, attributes, changed payload blocks, handles, capture state | Committed history; canonical objects | A Workspace incarnation; safe to lose once published |
| **PostgreSQL** (global metadata) | Committed canonical *metadata* bodies, object locators and dependencies, Layers/Commits/Branches, allocations | File payload bytes; live Workspace state | The product's durable authority |
| **Object store** (S3 API) | File-content packs only (whole-file payloads and CDC chunks, FULL/PREFIX/STORED) | Any metadata, any mutable object | Immutable; removed only by reachability GC |

This is the Phase 6 strict split (#293, 2026-10-02) carried forward, with the
global metadata store changed from a shared SQLite file to a PostgreSQL server
(owner decision, 2026-10-03; reasons in [04 §3](04-storage-interfaces.md#3-global-metadata-store-postgresql)).
What changes in Phase 7 is the daemon side: the overlay absorbs payload bytes, so
the private backing directory, its source rows, reference counts, pins and
retirement queue all disappear ([02](02-workspace-overlay.md)).

## 3. Components

Thirteen crates in three groups. **Domain** crates hold logic and define ports,
and do no I/O. **Engine** crates each own exactly one physical store. **Runtime**
crates assemble and expose the product.

### One line each

| Group | Crate | Status | Responsibility |
| --- | --- | --- | --- |
| Domain | `layerfs-content` (C1) | Kept | Turns bytes and directory trees into canonical, content-addressed objects (CDC, hashing, file and filesystem construction) |
| Domain | `layerfs-storage` (C2) | Reworked | Decides how canonical objects are physically stored: exact reuse, compression and delta encoding, pack framing, locators, and which store each body goes to |
| Domain | `layerfs-history` (C5) | Kept | Defines Layers, Commits and Branch heads and the conditional publish |
| Engine | `layerfs-s3` | New | Implements C2's object-store port against any S3-compatible service |
| Engine | `layerfs-metadata` | New | Owns the global PostgreSQL schema and implements C2's locator port and C5's `HistoryCatalog` |
| Engine | `layerfs-overlay` | New | Owns the daemon SQLite database: a Workspace's live names, inodes and payload blocks as generation-keyed rows |
| Runtime | `layerfs-workspace` | Rewritten | Gives filesystem operations their meaning over overlay plus committed base, and orchestrates a Commit from capture to install |
| Runtime | `layerfs-fuse` | Kept | Translates Linux kernel filesystem requests into Workspace calls |
| Runtime | `layerfs-daemon` | Reworked | The process inside the sandbox: wires engines into ports, holds credentials, serves the control endpoint |
| Runtime | `layerfs-bridge` | Shrunk | Carries control messages (exec, commit, status, lifecycle) between the SDK and the daemon |
| Runtime | `layerfs-api` | Kept | The public SDK and its core types |
| Runtime | `layerfs-sandbox` | Kept | Creates and manages the Docker sandbox, including the object-store endpoint and shared volume |
| Runtime | `layerfs-telemetry` | Kept | Timers and counters every other crate reports through |

`layerfs-server` is removed: construction runs wherever the bytes are, using the
same C1/C2 libraries and the two engines directly. Namespace Init/import is a
cluster 1 function and needs no sandbox; the crate that holds it is open (D13).

### Two clusters

The crates fall into two clusters that are developed and verified separately
(owner direction, 2026-10-03).

| | Cluster 1 — storage | Cluster 2 — sandbox |
| --- | --- | --- |
| Crates | `layerfs-content`, `layerfs-storage`, `layerfs-history`, `layerfs-s3`, `layerfs-metadata` | `layerfs-overlay`, `layerfs-workspace`, `layerfs-fuse`, `layerfs-daemon`, `layerfs-bridge`, `layerfs-sandbox`, `layerfs-api` |
| Needs to run | The PostgreSQL and MinIO containers; **no sandbox, no daemon, no mount** | A sandbox container with FUSE |
| Knows about | Canonical objects, CDC, reuse, compression, deltas, packs, locators, history records | Filesystem operations, generations, capture, install, Exec |
| Does not know about | Workspace Commit, conflict, edits, overlay, mount | How objects are encoded or where they are stored |
| Independence | **Fully independent** | Mounting a Workspace and running operations through FUSE are independent of cluster 1; only the last step is not |

`layerfs-telemetry` is shared by both.

- **Cluster 1 never depends on cluster 2.** The boundary check enforces it with
  the dependency table below.
- **Construction is called through `layerfs-content`'s API.** In the product the
  caller is the daemon, but nothing in C1 or C2 requires one: C1 does no I/O and
  takes its consumer and provider from the caller, and C2 saves and reads with no
  Workspace or mount. At the Phase 4.5 pin the daemon and Workspace contain no
  reference to C1, C2 or C5 at all. So data construction and processing are
  verified in cluster 1 without a daemon.
- **Commit, conflict and edits belong to cluster 2.** The engines store bodies
  and rows and learn nothing of them. A Workspace Commit is the one place the
  clusters meet: it touches `layerfs-history`, `layerfs-metadata`, `layerfs-s3`
  and `layerfs-storage`. Reading unchanged files from a non-empty committed base
  is part of the same integration step.

### Relationship graph

An arrow means "depends on at compile time". An engine's arrow into a domain
crate also means "implements that crate's port". `layerfs-telemetry` is omitted;
every crate reports through it and it depends on nothing.

```text
   SDK side                                  Daemon side (inside the sandbox)

 layerfs-api ────┐                      layerfs-daemon   (composition root: wires engines into ports)
                 ├──> layerfs-bridge <────────┤
 layerfs-sandbox ┘    (control only)          │
                                              ├──> layerfs-fuse ──┐
                                              │                   v
                                              ├──────────> layerfs-workspace
                                              │              │     │      │       │
                                              │              │     │      │       │
                                              │              v     │      v       v
                                              │   layerfs-overlay  │  layerfs-storage   layerfs-history
                                              │         ┆          │      │   ^   ^        │     ^
                                              │         ┆          v      v   │   │        │     │
                                              │         ┆        layerfs-content <─────────┘     │
                                              │         ┆                     │   │              │
                                              ├─────────┆──────────> layerfs-s3   │              │
                                              └─────────┆──────────> layerfs-metadata ───────────┘
                                                        ┆                 ┆    ┆
                                                        v                 v    v
                                                  daemon SQLite       S3/MinIO   PostgreSQL
```

### Allowed dependencies

This table is the rule; `core/tools/check_product_boundary.py` is extended in M0
to check every crate's `Cargo.toml` against it.

| Crate | May depend on (first-party) | Role of the edge |
| --- | --- | --- |
| `layerfs-content` | telemetry | — |
| `layerfs-storage` | content, telemetry | Implements C1's object traits; defines the object-store and locator ports |
| `layerfs-history` | content | Uses identity types; defines `HistoryCatalog` |
| `layerfs-overlay` | nothing | Owns the daemon database |
| `layerfs-s3` | storage | Implements the object-store port |
| `layerfs-metadata` | storage, history | Implements the locator port and `HistoryCatalog`; owns the global schema |
| `layerfs-workspace` | overlay, content, storage, history, telemetry | Uses all four; never names an engine |
| `layerfs-fuse` | workspace | Calls Workspace operations |
| `layerfs-bridge` | nothing | — |
| `layerfs-daemon` | every daemon-side crate, bridge | The only crate that names `layerfs-s3` and `layerfs-metadata` |
| `layerfs-api`, `layerfs-sandbox` | bridge, telemetry | Send control requests |

What the graph guarantees:

- **No cycles.** Every arrow points toward `layerfs-content` or a leaf.
- **Engines are invisible to logic.** Only the daemon depends on `layerfs-s3` and
  `layerfs-metadata`, so either can be replaced without touching the Workspace,
  C2 or C5.
- **C2 and C5 need no network and no database to build or test.**
- **The SDK side and the daemon side share only `layerfs-bridge`**, and no
  filesystem content crosses it.
- **`layerfs-workspace` is the single fan-out point** in the product logic; a
  Commit's ordering lives there and nowhere else.

### Ports

The consumer owns the port; the engine depends on the consumer. This is the
pattern C1 and C2 already use.

| Port | Defined by | Implemented by | Exists today |
| --- | --- | --- | --- |
| `AuthenticatedObjects`, `FinalizedConsumer` | `layerfs-content` | `layerfs-storage` | Yes (`object/access.rs`, `object/output.rs`) |
| `ObjectStore` | `layerfs-storage` | `layerfs-s3` | No — [04 §2](04-storage-interfaces.md#2-object-store-port) |
| Locator and metadata-body port | `layerfs-storage` | `layerfs-metadata` | No — today C2's SQL is inside `cas/` and `sqlite/` ([04 §3](04-storage-interfaces.md#3-global-metadata-store-postgresql)) |
| `HistoryCatalog` | `layerfs-history` | `layerfs-metadata` | Trait yes (`catalog.rs`); the PostgreSQL provider is new |
| `LowerFilesystem` (committed base as seen by a Workspace) | `layerfs-workspace` | C1 reads over C2, wired by the daemon | Partly — Phase 4.5 reaches it through bridge Inspect calls |

Following `core/AGENTS.md`, internal code stays concrete functions and types.
These are real I/O boundaries; no further interface, factory or registry is
introduced.

**One deliberate asymmetry.** `layerfs-overlay` is used directly by
`layerfs-workspace`, not through a port. Generations and the visibility rule are
the Workspace's core data model, and a trait wide enough to hide them would leak.
The cost is that replacing the overlay's storage means changing the Workspace's
dependency.

### Couplings that remain

1. **Shared vocabulary.** C1's identity types are used by C2, C5 and the
   Workspace; a change to them ripples by design.
2. **One writer for the overlay database.** Workspaces in a daemon are separated
   by key but share one SQLite writer, so they are isolated logically, not in
   throughput. The global store has no such single writer.
3. **Block size versus chunk size.** Correctness is independent; performance is
   not — overlay blocks larger than the smallest CDC chunk widen what C1 must
   re-chunk.
4. **Ordering across C2 and C5 data.** "Register locators before publishing the
   root" spans both; the Workspace orchestrates it and `layerfs-metadata` is the
   one place that can check it.

## 4. Data paths

```text
WRITE   command → kernel → FUSE → Workspace → one overlay transaction
        (block row + inode row at the active generation). No hashing, CDC,
        compression, upload or global-database access.

READ    command → kernel → FUSE → Workspace read plan:
        active rows → frozen rows → committed base. Only uncovered gaps reach
        the base: C1 range read → C2 locator lookup (PostgreSQL) →
        ranged GET (object store) → decode → authenticate.

COMMIT  capture (one UPDATE) → page the frozen generation → C1 → C2 →
        packs to the object store, metadata bodies + locators to PostgreSQL →
        one conditional C5 publication → install (one UPDATE) → retire in batches.

INIT    import a directory: scan → C1 → C2 → same two stores → C5
        initialize. Same writer path as COMMIT, without an overlay.
```

The WRITE path touches only local state, which is what makes Exec latency
independent of the global stores. The COMMIT path is the only writer of the
global stores. READ is the only path that depends on remote latency, and only
for bytes the Workspace has not changed.

## 5. What is removed

Sizes are **physical lines** (`wc -l`, comments and blanks included) at
`7edddbdb8`, given to show scale. They are not production-LOC figures; the
per-commit production comparison uses `core/tools/production_loc.py`.

| Removed | Size today | Replaced by |
| --- | --- | --- |
| `layerfs-workspace/src/backing/` — authenticated 4096-byte pages, B+ ownership tree, metadata arena, pieces, segments, reclamation | 47 files, 15,026 lines | Overlay tables; SQLite's own B-tree and pager |
| `layerfs-workspace/src/commit/` — lowering, staged upload to the host | 13 files, 4,400 lines | Local capture → C1 → C2 |
| `layerfs-server/src/service/save/` — host-side construction and validation | 14 files, 2,615 lines | Daemon-local construction |
| Bridge payload/metadata streaming contracts (`prepared_stream`, `workspace_commit`, `metadata`, `source`) | part of 13 + 18 files, 7,581 lines | Control messages only |
| Phase 6 row-version machinery (`born`/`until`, pins, source refcounts, retirement indexes, triggers) | never merged | Generation key |

What is *not* simplified away: canonical formats and identities, CDC and delta
policy, pack framing, authentication of everything read, history semantics,
bounded windows, and the one-construction-worker rule.

## 6. Trust boundary

The daemon holds the object-store and PostgreSQL credentials. Commands run
inside the sandbox as a separate non-root identity and reach storage only through
the FUSE mount; they must not be able to read the overlay database or either set
of credentials. Phase 6 selected a root supervisor with a non-root
command identity, cleared capabilities and `no-new-privileges` (#293); Phase 7
keeps that boundary and re-proves it on the new layout. A daemon is a trusted
first-party publisher: the global side checks identity, scope and the expected
Branch head, and does not re-certify content by fetching packs.
