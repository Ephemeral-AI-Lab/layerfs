# Workspace filesystem view: global Store base + overlay.sqlite

> **Status:** Current general guide.
> Source context reviewed on 2026-10-05 at local `main`
> `6e84b91818bff510f4902537e413eda5094cf754`.
> Documentation issue: [#309](https://github.com/Ephemeral-AI-Lab/layerfs/issues/309).

Navigation links were updated on 2026-10-06 for the responsibility-folder moves.
The source-review claims retain their original pin above; current continuation
scope is in the [S7–S9 handoff](../../core/docs/issues/307/HANDOFF-S7-S9.md).

This guide explains the cluster-two ownership and read-path design. Its central
ASCII diagram is preserved exactly from the owner-requested side conversation.
The diagram describes the integrated target; complete FUSE/runtime wiring and
effective filesystem semantics remain implementation work at the reviewed source.
It is not a release, performance or memory qualification claim.

The [cluster-two design](../../core/docs/issues/303/README.md),
[architecture and ownership](../../core/docs/issues/303/01-architecture.md),
[daemon contract](../../core/docs/issues/303/daemon-sqlite.md),
[FUSE contract](../../core/docs/issues/303/fuse.md) and
[runtime integration contract](../../core/docs/issues/303/06-cluster-one-integration.md)
remain authoritative. [Tracker #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307)
owns implementation and milestone evidence. This guide adds explanation, not a
new backend, cache algorithm or milestone-completion claim.

## 1. The view is a merge of two independently owned sources

```text
Immutable committed base selected by root R ----+
                                               +--> Workspace live view
Mutable changes in Workspace namespace NS -----+
```

The base is a complete committed LayerFS filesystem root, including `.git/index`,
ignored files, dependencies, symlinks, caches and outputs. Its objects are
immutable. Each Workspace selects a root; two branches can select different
roots while sharing objects with identical canonical IDs.

The overlay holds that Workspace's mutable state. One initialized database
belongs to each daemon, with separate Workspace namespaces inside it. Opening a
Workspace binds its base and namespace; the target repeated-mount path performs
no full-base scan, import, copy or dependency restoration. Demand I/O still pays
its actual cost.

| Live-view situation | Source and rule |
| --- | --- |
| Unchanged entry without an overlay override | Resolve through the selected immutable base |
| New entry | Resolve through the Workspace overlay |
| Changed namespace or metadata | Apply the visible overlay version |
| Deleted base entry | Overlay deletion hides it; absence must not resurrect the base entry |
| Partially modified file | Written ranges use overlay payload; inherited ranges use the base under the selected cutoff/validity rules |
| Directory listing | Merge ordered base/overlay pages, respecting replacement and deletion visibility |

The merge is a filesystem semantic operation, not concatenation of SQL rows.
Complete partial-write, truncate/regrow and sparse-hole semantics have their own
S5 contracts; this picture does not qualify those algorithms.

## 2. Main architecture diagram

The following is the exact diagram requested for preservation:

```text
                         LINUX SANDBOX
                  +-------------------------+
                  | Bash / tools / processes|
                  | open, read, write, stat |
                  +------------+------------+
                               |
                               v
                  +-------------------------+
                  | Linux VFS               |
                  | Per-mount kernel caches |
                  +------------+------------+
                               |
                     Requests requiring FUSE
                               |
                               v
  +---------------------- SANDBOX DAEMON -----------------------+
  |                                                            |
  |              +-----------------------------+               |
  |              | FUSE adapter                |               |
  |              | Kernel requests and replies |               |
  |              +--------------+--------------+               |
  |                             |                              |
  |                             v                              |
  |              +-----------------------------+               |
  |              | Workspace filesystem view   |               |
  |              |                             |               |
  |              | Selected base root R        |               |
  |              | + Workspace overlay NS      |               |
  |              | + visibility/merge rules    |               |
  |              +----------+---------+--------+               |
  |                         |         |                        |
  |       Inherited base    |         | Mutable state          |
  |                         v         v                        |
  |   +-----------------------+   +-------------------------+  |
  |   | Public content reader |   | Short typed overlay jobs|  |
  |   | Paths, metadata,      |   | Fair shared SQL service |  |
  |   | file roots and ranges |   +------------+------------+  |
  |   +-----------+-----------+                |               |
  |               |                            v               |
  |               v               +-------------------------+  |
  |   +-----------------------+   | overlay.sqlite          |  |
  |   | Base client           |   | ONE database per daemon |  |
  |   | Shared immutable cache|   |                         |  |
  |   | Key: ObjectId         |   | Workspace NS A: changes |  |
  |   +-----------+-----------+   | Workspace NS B: changes |  |
  |               |               | Payload / metadata /    |  |
  |               |               | generations / custody   |  |
  |               |               +-------------------------+  |
  +---------------|--------------------------------------------+
                  |
          Only on a base-cache miss
          Bounded authenticated demand
                  |
                  v
  +---------------------- HOST APPLICATION --------------------+
  |                                                            |
  |   +----------------------------------------------------+   |
  |   | SDK runtime adapters                               |   |
  |   | Authorization, request handling and delivery       |   |
  |   +--------------------------+-------------------------+   |
  |                              |                             |
  |                              v                             |
  |   +----------------------------------------------------+   |
  |   | layerfs-storage                                    |   |
  |   | Reader: locate, acquire, decode and authenticate   |   |
  |   +--------------------------+-------------------------+   |
  |                              | StorageProvider             |
  |                              v                             |
  |   +----------------------------------------------------+   |
  |   | layerfs-persistence                                |   |
  |   | Owns SQLite connections, schema and transactions   |   |
  |   +--------------------------+-------------------------+   |
  |                              |                             |
  |                              v                             |
  |   +----------------------------------------------------+   |
  |   | Global Store SQLite                                |   |
  |   | Immutable filesystem roots and encoded objects     |   |
  |   | Storage metadata, branches and Commit history      |   |
  |   +----------------------------------------------------+   |
  +------------------------------------------------------------+

      Results return up these paths to the Workspace and kernel.
```

The two branches under Workspace depict its sources, not two unconditional
database reads for every syscall. Workspace first establishes the appropriate
consistent view and bounded read inputs. Base acquisition follows when that view
requires inherited immutable data. A valid kernel cache may satisfy a request
before the daemon is contacted.

## 3. Global Store ownership belongs to layerfs-persistence

The global Store provider comes from
[`layerfs-persistence`](../../core/crates/layerfs-persistence/src/store/open.rs).
The host application selects
[`PersistenceConfig`](../../core/crates/layerfs-persistence/src/store/config.rs) and
creates or opens `Handles`. Those handles supply storage and history providers
over one combined host database.

| Component | Responsibility |
| --- | --- |
| `layerfs-persistence` | Host Store configuration/opening, SQLite schema/connections, provider transactions and persistence profiles |
| `layerfs-storage` | Object reuse, physical encoding/packs, Reader and Save over the storage provider |
| `layerfs-history` | Branch, staging and conditional Commit semantics over the history provider |
| SDK runtime | Embeds initialized host libraries; binds authority and serves bounded operations |
| `layerfs-content` in the daemon | Canonical filesystem/file reads and construction through public content APIs |
| Workspace | Effective filesystem semantics, selected base and overlay visibility |
| `layerfs-overlay` | Mutable SQL and physical payload, scratch and custody |
| Daemon | Service/process ownership, short-job scheduling and lifecycle |
| FUSE adapter | Kernel requests/replies, references and cache-coherence adaptation |

In the current SQLite composition, encoded packs or complete encoded groups are
stored as BLOBs in the host database. There is no required external payload-pack
directory. See the
[cluster-one persistence/layout contract](../../cluster_one_handbook.md#8-persistence-profiles-physical-layout-and-integration-limits).
`store.sqlite` is an explanatory filename; the application selects its actual
database path. Global means the committed Store shared by the relevant
Workspaces, not a mandatory universal database for every project on the host.

The host Store's selected Durable/Disposable profile and the daemon's
disposable overlay profile are separate contracts. The cache and overlay do not
inherit a durability guarantee merely because the global Store uses Durable.

## 4. What lazy FUSE access means

FUSE forwards normal kernel requests into Workspace operations. It does not
interpret host Store SQL or physical pack formats. The effective Workspace read
path uses public content APIs and the base client; the host runtime performs the
owning storage/provider work.

A base-content read follows this demand path:

1. A process issues an ordinary syscall. A valid kernel cache may satisfy it.
2. If service is needed, FUSE adapts the request to a Workspace operation.
3. Short overlay service establishes visible local metadata/data or a stable
   bounded plan for inherited base input.
4. The public content reader requests canonical objects through the base client.
5. An immutable cache hit supplies the authenticated retained object. A miss
   requests it through the authorized host runtime.
6. The host Storage Reader uses the persistence provider to acquire/decode the
   owning physical representation. Returned canonical identity is checked before
   cache retention/use. Results flow back into the Workspace reply.

Cache/SQL/Workspace locks must not span network acquisition, content construction
or a whole Exec/Commit. Long operations use bounded windows and fair service
rather than retaining the shared writer throughout.

Lazy means acquisition is driven by actual demand. It does not guarantee that
physical bytes equal the exact syscall range: namespace nodes, chunk/extent
objects, whole bounded small-file objects or pack units may need acquisition.
Mount may also acquire necessary root/binding metadata; it is not a zero-I/O
promise.

The main diagram follows base-object acquisition. Cheap owning file-length or
attribute facts have separate bounded runtime calls and immutable cache keys;
the cache-miss arrow does not prohibit those calls or control/history traffic.
[The file-length guide](../../core/docs/architecture/24-file-lengths.md) describes
the implemented owning API and its remaining daemon/stat integration.

## 5. Cache reuse and different Workspace branches

Kernel caches are per mount. The intended immutable object cache is shared per
daemon and keyed by exact `ObjectId`; the SQLite pager is shared across overlay
namespaces. Mutable overlay state remains Workspace-scoped.

Two Workspaces with the same canonical dependency object can reuse one retained
cache entry. A changed object has a different ID; the same pathname on different
branches is resolved through each Workspace's own root/overlay. A cache hit
does not grant authority to a Branch, Store or Workspace.

See [sandbox cache design](sandbox-cache-design.md) and
[documentation issue #308](https://github.com/Ephemeral-AI-Lab/layerfs/issues/308)
for byte-budget ownership, shared-eviction tradeoffs and aggregate memory gaps.

## 6. Writes and explicit Commit

Writes publish into the daemon overlay under the selected Workspace namespace.
A published mutation remains owned even if its reply is lost. Writing does not
implicitly Save, Commit, unmount or modify an immutable base object.

```text
Process write
     |
     v
Workspace semantics --> bounded overlay transaction --> overlay.sqlite
                                                           |
                                                explicit Commit capture
                                                           |
                                                           v
                                 stable captured input + base references
                                                           |
                                     bounded canonical construction
                                                           |
                                                           v
Host runtime --> Storage Save completion --> StageChanges --> CommitStaged
                                                           |
                                             known published outcome
                                                           |
                                                           v
Workspace installs new base root; later active changes remain in overlay
```

The Commit picture describes the target success path. Definite refusal, conflict
and uncertain outcomes retain their exact custody; an unknown result does not
authorize replay, rollback or install. Capture covers the shared Workspace's
locally published frontier, not only the invoking call's changes. Unpublished
dirty mmap stores are outside that frontier. Retained readers can remain on
their original immutable roots under the owning lifetime contract.

The overlay is not copied wholesale into the host database. Construction streams
canonical changes and reuses unchanged identities through cluster-one APIs.
Global history and local reclamation have separate ownership.

## 7. Evidence scope

This documentation records architecture and verified ownership/source roles. It
introduces no product source, dependency, runtime setting or measurement. The
diagram alone proves neither mounted correctness nor aggregate memory bounds.
Those require the owning #307 milestone and platform/family evidence. Existing
source pins, failures and unfinished runtime/FUSE/merge work retain their scope.
