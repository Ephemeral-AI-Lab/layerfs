# Workspace

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The filesystem semantics of a mounted Workspace: a complete mutable view of one immutable committed root, composed from demand-read base objects and the Workspace's own overlay rows.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-workspace` (its `construction/` belongs to [Commit](../commit/README.md)) |
| Source | [`base`](../../../../engine/crates/layerfs-workspace/src/base), [`operations`](../../../../engine/crates/layerfs-workspace/src/operations), [`mutation`](../../../../engine/crates/layerfs-workspace/src/mutation), [`workspace`](../../../../engine/crates/layerfs-workspace/src/workspace), [`ports`](../../../../engine/crates/layerfs-workspace/src/ports) under `engine/crates/layerfs-workspace/src` |
| Uses | [Overlay](../overlay/README.md), [Core: Objects](../../core/objects/README.md), [Core: Files](../../core/files/README.md), [Core: Filesystem trees](../../core/filesystem-trees/README.md) |
| Used by | [Store calls](../store-calls/README.md), [Commit](../commit/README.md), [Kernel routes](../kernel-routes/README.md), [Daemon](../daemon/README.md) |
| Flows | [A Workspace session](../../flows/workspace.md), [A Commit](../../flows/commit.md) |

## Picture

```text
 any process that sees the mount            SDK file.* calls
        | ordinary syscalls                        |
        v                                          v
 +---------------+                          +-------------+
 | Kernel routes |  kernel visits           | Store calls | MutationPlan
 +---------------+                          +-------------+
        |                                          |
        v                                          v
 +------------------------------------------------------------------+
 | Workspace: Route, base cell (BaseView), Serials                  |
 |   Operations (lookup, stat, list, read) ---> Mutation (plan,     |
 |                                               NamespaceJob)      |
 |   Base: BaseView over CanonicalClient    Ports: OverlayRead,     |
 |         + shared CanonicalCache                 OverlayJobs, ... |
 +---------|-------------------------------------------|------------+
           | demand reads, outside every overlay lock  | one short job
           v                                           v
 +--------------------------+           +------------------------------+
 | global Store: immutable  |           | Workspace overlay file:      |
 | objects of the base root |           | names, inodes, cells, records|
 +--------------------------+           +------------------------------+
```

Base I/O never happens under the overlay lock, and an overlay job does no
Store I/O ([`ports/overlay.rs`](../../../../engine/crates/layerfs-workspace/src/ports/overlay.rs)).

## What it does

### The effective view

```text
 active generation    names, inodes, cells (newest)     a row decides;
 captured generation  sealed rows not yet installed     a whiteout hides
 committed base       immutable root in the Store       the name below;
                                                        no row falls through
```

| Behaviour | Detail | Source |
| --- | --- | --- |
| Whole tree | `.git` and its index, ignored files, dependencies, symlinks, caches and output are all visible; nothing filters names by Git rules or any pattern | [`base/view.rs`](../../../../engine/crates/layerfs-workspace/src/base/view.rs), [`operations`](../../../../engine/crates/layerfs-workspace/src/operations) |
| Mount without a scan | `BaseView::open` acquires the root object once and checks its `InodeScope`; it lists, copies and mirrors nothing | [`base/view.rs`](../../../../engine/crates/layerfs-workspace/src/base/view.rs), [`store/bind.rs`](../../../../engine/crates/layerfs-store-service/src/store/bind.rs) |
| No Exec identity | Kernel visits carry a mount, serial and handle only; no type names an Exec, command or process | [`kernel_visit.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_visit.rs) |
| Shared frontier | A change is published to the one overlay file the Workspace owns and is visible to every later call, whoever launched it | [`mutation/job.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/job.rs) |
| Atomic operation | All effects of one operation publish in one overlay transaction, or none do | same |
| Lost reply | `Outcome::Applied` carries a `Publication`; the reply attempt releases it, sent or not, and never undoes it | [`operations/types.rs`](../../../../engine/crates/layerfs-workspace/src/operations/types.rs) |
| Per call or per task | The crate has no notion of a tool call or task: one Workspace serves any number of sequential or concurrent calls and Commits | [`workspace/state.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/state.rs) |

### Base

```text
 BaseView { client: Arc<CanonicalClient>, identity, root (checked) }
   inode | child | list | stat | readlink | plan_read | content | at(root)
        |
 CanonicalClient: read_canonical_batch(ids <= 4,096)
        +--> CanonicalCache hit? -> bytes
        +--> miss: one upstream demand; len <= 16 MiB;
             ObjectId::for_bytes(bytes) == id; total <= 32 MiB
```

| Behaviour | Detail | Source |
| --- | --- | --- |
| Demand reads | Every answer reads only the objects it needs: inode table pages, directory pages, metadata root, file mapping (`plan_read` at most 128 KiB) | [`view.rs`](../../../../engine/crates/layerfs-workspace/src/base/view.rs) |
| Older roots | `at(root)` lets a read window that names an older root (an unlinked file, a captured reader) read it after an install moved on | same |
| Authentication | Every fetched object is hashed back to its `ObjectId`; provider errors stay with the source and are never cached | [`client.rs`](../../../../engine/crates/layerfs-workspace/src/base/client.rs) |
| Cache | LRU over objects (charge `len + 256`), file lengths (`8 + 256`) and portable metadata (`16 + 256`) in one byte allowance; an object larger than it bypasses; eviction never changes authoritative state | [`cache.rs`](../../../../engine/crates/layerfs-workspace/src/base/cache.rs) |
| One cache per Store binding | A daemon's `Store` creates one `CanonicalCache` (daemon default 64 MiB) and every Workspace client shares it | [`store/open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs), [`install_types.rs`](../../../../engine/crates/layerfs-daemon/src/install_types.rs) |
| Constructed objects | A Commit stages the tree objects it built (at most 1/8 of the allowance, file payload excluded) and `admit_constructed` places them only after its root is known published | [`cache.rs`](../../../../engine/crates/layerfs-workspace/src/base/cache.rs) |
| Lengths | Asked of the length port and remembered; a lookup of an unremembered file also fetches up to 128 sibling lengths in one demand | [`client.rs`](../../../../engine/crates/layerfs-workspace/src/base/client.rs), [`view.rs`](../../../../engine/crates/layerfs-workspace/src/base/view.rs) |
| Read-ahead | A base read that continues a followed stream of chunked content fetches its range plus 8 MiB in one demand; streams live with the cache (allowance / 16 MiB of them); a cache under 16 MiB, a memory-only client, a random read or a whole-file object never reads ahead | [`client.rs`](../../../../engine/crates/layerfs-workspace/src/base/client.rs) |
| Directory link counts | Derived from the number of child directories: listed in windows of 256 rows / 64 KiB and remembered by content root (`DIRECTORY_COUNT_CAPACITY` 16,384); after a Commit, changed directories' counts are remembered under their new roots | [`links.rs`](../../../../engine/crates/layerfs-workspace/src/base/links.rs) |
| Memory-only client | `CanonicalClient::resident` answers from the cache inside an owner job; objects over 64 KiB are not copied; a miss leaves the visit undecided | [`client.rs`](../../../../engine/crates/layerfs-workspace/src/base/client.rs) |

### Mutation

```text
 caller          Workspace               overlay lock (one job)      Store
   | Operation      | next_serial (creating ops only)                    |
   |--------------->| prepare_mutation: no SQL, no base I/O              |
   |                |--- round 1 ------------>| decide over rows        |
   |                |<-- Needs[...] ----------| writes nothing          |
   |                | supply(view) ----- no lock held ------------------>|
   |                |<------------------------------------ base facts ---|
   |                |--- round 2 ------------>| decide again; apply:    |
   |                |<-- Applied{publication}-| one transaction         |
   |<-- Outcome ----|  reply attempt -> reply_attempted(publication)     |
```

| Behaviour | Detail | Source |
| --- | --- | --- |
| Plan stages | `MutationPlan`: `Owner` -> `accept` -> `Base` (`supply`) -> `Owner` ... -> `Finished`; `accept` takes one owner result and finishes on any publication or error | [`plan.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/plan.rs) |
| Job outcome | `Applied { publication, inode }`, `Unchanged`, `Needs(Vec<Need>)` or `Refused(Refusal)`; only `Applied` changes state | [`job.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/job.rs) |
| Changes | One job emits `Changes`: inodes, names (`Bound` or `Removed`, stamped `inherited`), `created`, a symlink `cell`, one `write` with `claims`, `detached`, `moved_directory`, `predecessor` | same |
| Facts | `Eval` decides from a local row, else a base fact, else returns a `Need`; supplying a bound name also supplies its inode and the rest of a destination path, so an undisturbed operation settles in two jobs. `FACT_WINDOW` (768) refetches, never refuses | [`eval.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/eval.rs), [`facts.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/facts.rs) |
| Driver | `Workspace::mutate` loops job / accept / supply for a caller that owns a `SourceView`; SDK writes use the same loop, kernel requests use `kernel_mutation_visit` | [`driver.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/driver.rs), [`write/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |
| Newer state wins | Another operation may publish between two rounds; the next round decides over the rows current in its own job | [`job.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/job.rs) |

### State, serials and install

| Behaviour | Detail | Source |
| --- | --- | --- |
| Binding | `Workspace { route, base: Arc<RwLock<BaseView>>, serials, operation_client }`; `open` or `bind` binds a checked root; `scoped(client)` shares the base cell and serials with one operation's provider | [`state.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/state.rs) |
| Route | `Route { namespace > 0, engine != 0, incarnation != 0 }`; every source, file, capture and kernel mount is checked against its engine, else `Stale` | [`route.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/route.rs) |
| Source view | `view_for_source(BaseSource)` checks engine and current root (`BaseChanged`); `stat` reads overlay, base, then overlay again, so a row published meanwhile wins | [`view.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/view.rs) |
| Serials | Reserved from the scope's `InodeSerials` allocator `SERIAL_REFILL` (1,024) at a time, outside every lock; consumed, never recycled; one call makes at most one allocator request; an optional low-water refills early | [`serials.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/serials.rs) |
| Install | `prepare_base_install(capture, root)` opens the new root with no lock; `install_prepared_base` holds the base write lock for one SQL step (`overlay.install`), with nothing fallible after it; later changes stay | [`install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs) |

### Ports

| Port | Calls | Implemented by | Source |
| --- | --- | --- | --- |
| `OverlayRead`, `OverlayJobs` | Inode, name, name window, cell, read window; `namespace(job)` | `Overlay` directly; `OwnerClient` under the Workspace lock | [`ports/overlay.rs`](../../../../engine/crates/layerfs-workspace/src/ports/overlay.rs), [`read_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/read_port.rs) |
| `OverlayFileRead` | `file_read` (works after the last name is gone), `captured_read` | same | [`ports/files.rs`](../../../../engine/crates/layerfs-workspace/src/ports/files.rs), [`file_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/file_port.rs) |
| `OverlayCapturedNamespace`, `OverlayCapturedRuns` | Replayable pages of captured inodes and names, symlinks, digests, predecessors, `keep_live_row`; one captured run step | same | [`captured_namespace.rs`](../../../../engine/crates/layerfs-workspace/src/ports/captured_namespace.rs), [`captured_runs.rs`](../../../../engine/crates/layerfs-workspace/src/ports/captured_runs.rs) |
| `OverlayOperationRecords` | Guarded key/value records, windows of 64 | same | [`operation_record.rs`](../../../../engine/crates/layerfs-workspace/src/ports/operation_record.rs), [`operation_record_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/operation_record_port.rs) |
| `FileLengths`, `FileDigests` | Store length of a content root; BLAKE3 digest to saved root, `FILE_DIGEST_WINDOW` 1,024 | `CanonicalClient`, `StorePorts`, `StoreDigests` | [`lengths.rs`](../../../../engine/crates/layerfs-workspace/src/ports/lengths.rs), [`digests.rs`](../../../../engine/crates/layerfs-workspace/src/ports/digests.rs), [`store/digests.rs`](../../../../engine/crates/layerfs-store-service/src/store/digests.rs) |

Every port method is one short overlay job: an error after the attempt is
the exact outcome, never retried. Replaying a sealed captured page is
reading, not retrying.

## Invariants

| Rule | Enforced by |
| --- | --- |
| A Workspace is the complete tree, including `.git` and ignored files | No name filter anywhere in [`operations`](../../../../engine/crates/layerfs-workspace/src/operations) or [`base`](../../../../engine/crates/layerfs-workspace/src/base) |
| Mount binds the root without scanning, copying or restoring it | `BaseView::open`: one root acquisition and a scope check ([`view.rs`](../../../../engine/crates/layerfs-workspace/src/base/view.rs)) |
| Different Branches reuse identical objects by `ObjectId`: Workspaces of any Branch on one daemon share one cache keyed by object identity, so no entry is keyed by root and install needs no invalidation | `Cache.objects: BTreeMap<ObjectId, ..>` ([`cache.rs`](../../../../engine/crates/layerfs-workspace/src/base/cache.rs)); one `CanonicalCache` per `Store` ([`store/open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs)) |
| A cached entry grants no authority; the cache owner restricts sharing to one authorization context | [`cache.rs`](../../../../engine/crates/layerfs-workspace/src/base/cache.rs) `CanonicalCache` |
| An overlay job does no Store, provider or content I/O; missing base facts become `Need`s | [`ports/overlay.rs`](../../../../engine/crates/layerfs-workspace/src/ports/overlay.rs), [`eval.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/eval.rs) |
| One attempt per operation; a finished plan refuses another `accept` or `supply` | [`plan.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/plan.rs) |
| Base facts stay true while a source is owned, because install waits for source release | [`facts.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/facts.rs), [`state.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/state.rs) |
| Inode serials come from the scope's allocator, never invented or reused | [`serials.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/serials.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| One write, one read, one listing window | `WRITE_WINDOW` 256 KiB, `READ_WINDOW` 128 KiB, `PAGE_ROWS` 1,024 names (whiteouts included); larger work is several windows | [`file/write.rs`](../../../../engine/crates/layerfs-workspace/src/operations/file/write.rs), [`file/read.rs`](../../../../engine/crates/layerfs-workspace/src/operations/file/read.rs), [`namespace/list.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/list.rs) |
| Total files, names, edits or Workspace size | No cap | [`operations/types.rs`](../../../../engine/crates/layerfs-workspace/src/operations/types.rs) |
| Largest file offset | `i64::MAX`; past it `Refusal::TooLarge` (EFBIG) | [`file/write.rs`](../../../../engine/crates/layerfs-workspace/src/operations/file/write.rs) |
| Name length | 255 bytes | [`namespace/list.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/list.rs) |
| Definite pre-effect refusals | `Refusal`: EEXIST, ENOENT, ENOTDIR, EISDIR, ENOTEMPTY, EPERM, EOPNOTSUPP, EINVAL, EMLINK, EFBIG and ancestry cases | [`operations/types.rs`](../../../../engine/crates/layerfs-workspace/src/operations/types.rs) |
| One base demand | At most 4,096 ids, 16 MiB per object, 32 MiB returned | [`client.rs`](../../../../engine/crates/layerfs-workspace/src/base/client.rs) |
| Base root changed under a source | `WorkspaceError::BaseChanged` | [`view.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/view.rs) |
| Stale route or engine | `OverlayError::Stale` | same |
| Poisoned base or serial lock | `WorkspaceError::BindingPoisoned` | [`state.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/state.rs) |

## Demonstration

The tests use an in-memory content Store, a fresh overlay file in the temp
directory and a counting serial allocator; no Docker, root or mount.

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`namespace_view.rs`](../../../../engine/crates/layerfs-workspace/tests/namespace_view.rs) | `cargo +1.85.1 test --locked -p layerfs-workspace --test namespace_view` | Linux, macOS | Listing under mutation, no name or inode cap, known install keeping later changes |
| [`mutation_plan.rs`](../../../../engine/crates/layerfs-workspace/tests/mutation_plan.rs) | `cargo +1.85.1 test --locked -p layerfs-workspace --test mutation_plan` | Linux, macOS | No base demand in owner rounds; `Needs` without a revision change; exactly one publication; no re-entry |
| [`shared_cache_client.rs`](../../../../engine/crates/layerfs-workspace/tests/shared_cache_client.rs) | `cargo +1.85.1 test --locked -p layerfs-workspace --test shared_cache_client` | Linux, macOS | One demand for exactly what is missing, exact LRU eviction, re-authentication after eviction |
| [`directory_links.rs`](../../../../engine/crates/layerfs-workspace/tests/directory_links.rs) | `cargo +1.85.1 test --locked -p layerfs-workspace --test directory_links` | Linux, macOS | Exact child-directory counts, maintained and derived |
| [`read_ahead.rs`](../../../../engine/crates/layerfs-workspace/tests/read_ahead.rs) | `cargo +1.85.1 test --locked -p layerfs-workspace --test read_ahead` | Linux, macOS | Sequential base reads fetch the next 8 MiB in one demand; random reads fetch nothing ahead |

## Pages

| Page | What it covers |
| --- | --- |
| [Operations](operations.md) | Each filesystem operation: namespace changes, listing, attributes, file reads and writes, recognition, and kernel visits. |
