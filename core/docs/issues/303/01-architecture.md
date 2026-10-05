# 01 — Architecture: ownership, placement and boundaries

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Part of the [cluster two design set](README.md). Written 2026-10-05 against
> `main` `f96d97651`. Nothing here is implemented; claim labels are defined in
> the [entry point](README.md#claim-labels).

## 1. What the owner requires

[owner requirement]

| # | Requirement |
| --- | --- |
| R1 | Extremely fast, smooth filesystem mutations |
| R2 | A committed base served through cluster one's global SQLite storage |
| R3 | A daemon-owned SQLite overlay holding live Workspace metadata **and** payload |
| R4 | Several concurrent Workspaces per daemon |
| R5 | Several concurrent Execs per Workspace |
| R6 | At most one Commit in flight per Workspace |
| R7 | Filesystem activity continues while a Commit constructs and publishes its captured state |
| R8 | No artificial size or edit-count limit inherited from the private backing engine; no checkpoint work per file change |

## 2. The shape in one picture

[proposed design]

```text
 macOS host                                    Linux sandbox container (Docker VM)
+--------------------------------------+      +------------------------------------------------+
| SDK / sandbox owner                  |      |  commands (Exec 1..n), non-root identity       |
|   layerfs-api, layerfs-sandbox       |      |        |  POSIX calls                          |
|        | control (open, exec,        |      |        v                                       |
|        |  commit, close)             |      |  kernel FUSE mount, one per Workspace          |
|        +-----------------------------------> |        |  requests                             |
|                                      |      |        v                                       |
| store host  (layerfs-server)         |      |  layerfs-daemon                                |
|   one owner thread                   |      |   +-- layerfs-fuse      kernel <-> Workspace   |
|   +-- layerfs-storage  Save, Reader  |      |   +-- layerfs-workspace semantics, read plan,  |
|   +-- layerfs-history  typed ops     | <----------- base client      Commit orchestration    |
|   +-- layerfs-persistence            | bridge|   +-- layerfs-content   logical reads and      |
|          |                           | (AEAD)|   |                     canonical construction |
|          v                           |      |   +-- layerfs-overlay   SQL only               |
|   store.sqlite   GLOBAL STORE        |      |          |                                     |
|   immutable objects, packs, history  |      |          v                                     |
+--------------------------------------+      |   ws-<n>.db   one file per Workspace           |
                                              |   LIVE OVERLAY: names, inodes, payload         |
                                              +------------------------------------------------+

   current view of a Workspace  =  live overlay   over   committed base (root R)
```

Two databases, two owners, one transport:

- The **global Store** is cluster one's host-local SQLite file. Exactly one
  process opens it writable: the store host. [source-verified:
  `layerfs-persistence` refuses every open unless built for macOS,
  `core/crates/layerfs-persistence/src/open.rs:16`, `:64`]
- The **overlay** is one SQLite file per Workspace inside the container, opened
  only by the daemon that created it.
- The **bridge** carries control calls from the host into the daemon, and base
  reads, the Commit object stream and history calls from the daemon to the
  store host. A Rust trait is not an endpoint; [06](06-cluster-one-integration.md)
  lists the operations that must be added.

## 3. Ownership, execution location, database and transport

[proposed design; the "today" column is source-verified]

| Concern | Owner (crate) | Runs in | Database | Reached through | Today on `main` |
| --- | --- | --- | --- | --- | --- |
| Canonical objects, packs, reuse, encoding | `layerfs-storage` | store host | global Store | in-process | Member of the core workspace |
| History: Branch, stage, Commit, inode serials | `layerfs-history` over `layerfs-persistence` | store host | global Store | in-process | Member |
| Store host service: one owner thread, request queue, admission checks | `layerfs-server` (rewritten) | host | — | bridge listener | Excluded reference crate; imports `layerfs_storage::Store` and `layerfs_history::sqlite`, which no longer exist |
| Canonical construction and logical reads of the base | `layerfs-content` | **daemon** | none (pure computation) | function calls; objects through the base client | Member; never built for Linux |
| Base client: object fetch, authentication, bounded cache | `layerfs-workspace` (`base/`) | daemon | none | bridge `ReadObjects`, `Attributes` | Absent |
| Live overlay rows and their SQL | `layerfs-overlay` (new) | daemon | `ws-<n>.db` | function calls | Absent |
| Filesystem semantics, read plan, capture, install, Commit orchestration | `layerfs-workspace` (rewritten) | daemon | through `layerfs-overlay` | function calls | Excluded reference crate, 26,835 production lines of private backing |
| Kernel protocol | `layerfs-fuse` | daemon | none | `/dev/fuse` | Excluded reference crate |
| Composition, registry, Exec, control | `layerfs-daemon` | daemon | none | bridge control listener | Excluded reference crate; one Workspace, one session |
| Transport, wire contract | `layerfs-bridge` | both | none | TCP, Noise AEAD | Excluded reference crate |
| Container lifecycle, SDK | `layerfs-sandbox`, `layerfs-api` | host | none | Docker CLI, bridge | Excluded reference crates |

What each component must not know:

| Component | Must not know |
| --- | --- |
| `layerfs-overlay` | Cluster one, FUSE, the bridge, what a Commit is. It knows tables, generations and streams |
| `layerfs-workspace` | SQL text, kernel protocol, transport framing, pack formats |
| `layerfs-fuse` | Storage of any kind |
| `layerfs-daemon` | Filesystem semantics. It wires, admits and supervises |
| Store host | Workspace, overlay, generation, mount. It sees object reads, one Save session per Commit, and typed history calls |
| Cluster one crates | Anything about cluster two. No file in them is changed by this design; requested changes are listed in [06 §6](06-cluster-one-integration.md#6-prerequisites-outside-cluster-two) |

## 4. The three paths

[proposed design]

```text
READ / LOOKUP                 MUTATION                         COMMIT
-------------                 --------                         ------
FUSE request                  FUSE request                     control call
   |                             |                                |
resolve in overlay            validate                         admit (one per Workspace)
(0-2 point statements;        update the affected rows         capture: one UPDATE      <- short, under
 none when the overlay        in ONE short transaction         later writes go to G+1      the Workspace mutex
 is empty)                    reply                               |
   |                                                           construct from R + G     <- own thread,
for what the overlay          never: a base payload read,         |                        no lock held
does not hold:                a checkpoint, canonical          stream objects to the
base client -> cache          construction, a scan of          host Save; finish
   -> store host              the file or the Workspace           |
   |                                                           stage + conditional
reply                                                          history transition
                                                                  |
                                                               known success: install R'  <- short
                                                               retire G in bounded steps
```

Details: reads and the base in [02](02-base-overlay.md), mutations in
[03](03-mutation-hot-path.md), Commit in [04](04-concurrency-commit.md).

## 5. Inside the daemon

[proposed design]

```text
layerfs-daemon
  Registry: WorkspaceId -> Arc<Workspace>          limit: max_workspaces (configured)
  BaseCache: ObjectId -> canonical bytes           one byte budget, shared by every Workspace
  Upstream: pool of authenticated connections      limit: upstream_connections (configured)
  Maintenance thread                               round-robin, one bounded step per Workspace

  Workspace (one per open Workspace)
    core: Mutex<Core>
       Core = SQLite connection + generation numbers + allocators + open counts
    busy: per-inode flags + Condvar                multi-step operations on one inode
    base: Arc<BaseBinding>                         root R, scope, profile, root serial; swapped at install
    commit: CommitSlot                             idle | running | uncertain
    mount: FUSE session, 2 request threads
    execs: table                                   limit: max_execs (configured)
```

| Lock or gate | Scope | Protects | Never held across |
| --- | --- | --- | --- |
| Registry `RwLock` | daemon | The map of Workspaces | Anything but the map lookup |
| `Workspace.core` mutex | one Workspace | The connection, the generation numbers, the serial and stream allocators, open counts | A base fetch, a bridge call, construction, a kernel notification, another Workspace's lock |
| Per-inode busy flag | one inode | A multi-step operation on that inode: a large shrink, a directory emptiness check, a fold merge | Nothing else waits on it except operations on the same inode |
| `CommitSlot` | one Workspace | One Commit in flight | — (a state, not a mutex) |
| `BaseCache` shard mutex | daemon | Cache maps | A fetch |
| Upstream connection checkout | daemon | One connection for one call, or for one Save session | — |

Consequences, each a requirement above:

- **R4.** Workspaces share no lock on any request path. They share the disk, the
  base cache budget, the upstream pool and the store host; each of those is an
  explicit, configured resource.
- **R5.** Execs are processes on one mount. Their requests meet at the
  Workspace mutex and **wait**; nothing is refused for contention. Today's
  product returns `EBUSY` in that case
  (`core/crates/layerfs-workspace/src/runtime/coherence.rs:482-493`).
- **R6, R7.** The Commit slot admits one Commit. Its construction thread takes
  the Workspace mutex only for bounded reads of captured rows, so mutations
  interleave with it. There is no daemon-wide Commit lock: Commits of different
  Workspaces run on their own threads and meet only at the store host's queue,
  which interleaves them one accepted object at a time.
- **No control-plane slot.** Today one `slot` mutex is held across a whole Exec
  and a whole Commit and a second control connection is closed
  (`core/crates/layerfs-daemon/src/control.rs:126-131`, `:310-317`, `:435-451`).
  The replacement serves control calls concurrently; each call takes only the
  locks in the table.

Configured limits (`max_workspaces`, `max_execs`, `upstream_connections`, the
base cache budget, the overlay quota) are explicit resource limits. A request
beyond a count limit is refused with a typed result naming the limit; it is
never queued invisibly. Defaults are owner question O-13.

## 6. Trust boundary

[proposed design; the first sentence is source-verified]

Today there is no boundary: commands run as uid 0 with the daemon's
environment, which contains the bridge private key
(`core/crates/layerfs-sandbox/src/docker.rs:203-229`;
`core/crates/layerfs-daemon/src/execution.rs:70`). The replacement:

- Commands run as a configured non-root uid and gid with a cleared environment
  (the value is owner question O-8). The overlay directory is mode 0700, owned
  by the daemon's uid, on the container's own disk: never on the FUSE mount and
  never on a host bind mount.
- The daemon is inside the sandbox, so the store host treats it as untrusted
  input. For reads it serves objects by identity; the daemon authenticates
  every object it receives (BLAKE3 of the bytes equals the requested id) and
  trusts neither the transport nor its own cache.
- For Commit the store host recomputes every identity from the canonical bytes
  it is sent ([source-verified] `FinalizedObject::new` does this,
  `core/crates/layerfs-content/src/object/output.rs:98-110`) and must also
  re-derive each object's role-specific references before admitting it. That
  check does not exist today. It is a named prerequisite in
  [06 §6](06-cluster-one-integration.md#6-prerequisites-outside-cluster-two)
  and the reason placement is owner question O-2.

## 7. Decisions that shape the architecture

Each is [proposed design]. The full list with evidence is in
[08](08-decisions-provenance.md).

### K1 — One overlay database file per Workspace

The prepared plan fixed one database per daemon with every key prefixed by a
Workspace number. That gives the daemon one SQLite write lock, so a large write
or a retirement batch in Workspace A delays Workspace B; closing a Workspace is
a row-by-row delete; and a per-Workspace quota must be accounted by hand.

| | Shared file | File per Workspace (chosen) |
| --- | --- | --- |
| Writer | One lock for the daemon | Independent |
| Close | Delete every row; the file keeps its high-water size | Close and unlink |
| Quota | Hand accounting | `PRAGMA max_page_count` |
| Failure | Disk-full or corruption reaches every Workspace | One Workspace |
| Cost | — | One connection and one page cache allowance per open Workspace |

This reverses a "decided" item of the #303 planning prompt and is flagged in
[08](08-decisions-provenance.md).

### K2 — Canonical construction runs in the daemon; storage and history stay on the host

| | A — construct on the host | B — content and storage in the daemon | C — content in the daemon, storage and history on the host (chosen) |
| --- | --- | --- | --- |
| Crosses the bridge at Commit | Raw extents and rows | Sealed packs and locators | Finalized canonical objects |
| Replayable inputs | `apply_edits` reads replacement bytes twice, so the host must spool the upload or call back into the daemon | Direct from overlay rows | Direct from overlay rows |
| Base reads | One semantic call per lookup; results keyed by root, so every Commit invalidates them | A callback protocol across the bridge | Objects by identity; a cache that stays valid across roots and Workspaces |
| Can a sandbox corrupt shared storage? | No | Yes | Not by identity; role and references need the admission check of §6 |
| Daemon links | Overlay and bridge only | Content and storage (a C build of zstd) | Content (Rust and BLAKE3) |
| Reuse of the dormant host code | Highest | Lowest | Transport reused; two new operation families |

C is chosen because the base cache is keyed by immutable identity. After a
Workspace commits and the next tool call opens the new head, every unchanged
tree node is already cached. That is the owner's stated workload, a Workspace
per tool call. A is the fallback if the owner prefers the smallest trust
surface; it is what the dormant code implements. C also moves canonical
construction into the container, which the committed benchmark hosting rule
forbids today (`docs/general/benchmark_rules.md:14-20`); that is owner
question O-1.

**Not established:** that `layerfs-content` builds for
`aarch64-unknown-linux-musl`. Its source has no platform gate, but nothing was
compiled for this document.

### K3 — The overlay is a single-connection SQLite database with an in-memory rollback journal

`journal_mode = MEMORY`, `locking_mode = EXCLUSIVE`, `synchronous = OFF`, one
connection per Workspace. There is no write-ahead log, so there is **no
checkpoint of any kind** and payload is written to the file once. Each mutating
request is one short atomic SQL transaction. The overlay claims runtime
atomicity only; a Workspace does not survive its daemon, as today. The
alternative that survives a daemon crash (a write-ahead log with exclusive
locking) is described in [02 §2](02-base-overlay.md#2-database-placement-and-profile)
and is owner question O-3.

## 8. What is removed

[source-verified for what exists; proposed for the removal]

| Mechanism on `main` | Why it existed | Replaced by |
| --- | --- | --- |
| One file per 4 KiB page, written, read back and SHA-256 verified per mutation | A hand-built authenticated pager | SQLite pages; no per-mutation verification |
| Copy-on-write index revision published per mutation, with inline reclamation and a compaction plan | Crash-consistent private metadata | One short SQL transaction |
| One 8 MiB memory ledger shared by every Workspace, surfacing as `ENOSPC` | Bounding resident structures that grew with dirty state | State lives in rows; resident memory is windows |
| 128 open handles, 32 captures, 32 metadata roots, index depth 7 | Fixed-size structures | No such structures |
| A node-table scan on every FORGET and every close | Garbage collection of a resident node table | A lookup-count map; no scan |
| `EBUSY` on overlapping callbacks; one payload acquisition per daemon | Coherence by refusal | A short wait on the Workspace mutex |
| One control session; one lock across Exec and Commit | Single-Workspace daemon | Registry and per-Workspace state |
| Prepared-stream upload capped at 256 MiB; host-side construction spool | Host construction | Daemon construction, object stream with backpressure |
| One host round trip per created inode | No local allocator | A reserved serial range, refilled in the background |
| Per-WRITE kernel invalidation | Direct-I/O coherence | Not needed under [05 §4](05-fuse-assessment.md#4-coherence-a-lifetime-is-not-a-design) |

Source for each row is in the limitation inventory,
[02 §10](02-base-overlay.md#10-limitation-inventory).
