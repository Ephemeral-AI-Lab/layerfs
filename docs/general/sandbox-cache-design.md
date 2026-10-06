# Sandbox cache design

> **Status:** Current general guide.
> Source review: 2026-10-05, local `main` commit
> `c6df039e63187da1f407843540c37477c9e25beb`.
> Documentation issue: [#308 — Sandbox cache design](https://github.com/Ephemeral-AI-Lab/layerfs/issues/308).

Navigation links were updated on 2026-10-06 for the responsibility-folder moves.
The source-review claims retain their original pin above; current continuation
scope is in the [S7–S9 handoff](../../core/docs/issues/307/HANDOFF-S7-S9.md).

This guide explains the existing cluster-two cache policy, the source implemented
at the reviewed commit, and why the ownership structure fits many Workspaces
using different branches of the same project. It describes implementation gaps
explicitly; it is not a release, memory qualification or performance claim.
Concurrent uncommitted implementation work is outside this source review.

The [cluster-two design index](../../core/docs/issues/303/README.md),
[cache ownership contract](../../core/docs/issues/303/02-base-overlay.md#8-caches-owner-key-visibility-invalidation),
[daemon contract](../../core/docs/issues/303/daemon-sqlite.md) and
[FUSE contract](../../core/docs/issues/303/fuse.md) remain authoritative. This
guide explains those contracts without selecting a new cache algorithm, changing
their budgets or completing a milestone in [tracker #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307).

## 1. Policy: shared immutable bytes, separate mutable views

The intended default is one daemon-owned immutable base object cache shared by
all its Workspaces. The cache follows the daemon boundary. Workspaces in one
sandbox share it when they use that daemon; separate daemons have separate cache
owners. A branch name, project path or Workspace identifier is not a content
cache key.

Each Workspace retains its own immutable base-root binding and mutable overlay
namespace. Shared bytes do not make their filesystem views identical. Authority
and access to a root are separate from possession of cached object bytes.

| State | Owner and key | Reviewed implementation status |
| --- | --- | --- |
| Immutable base objects | Daemon-wide target; exact `ObjectId` | `CanonicalClient` owns a byte-budgeted cache; sharing is possible through `Arc`, but daemon bootstrap does not yet enforce one instance |
| Immutable attributes | Daemon-wide target; content and metadata identities | Specified by the design; the complete daemon attribute cache is unfinished |
| SQLite pages | One overlay connection's pager; page number | Active shared overlay engine; allowance is a SQLite suggestion, not a whole-process memory bound |
| Mutable inode/name/payload/scratch/custody state | Workspace namespace, plus the owning generation or operation | SQL-backed engine relations exist; complete filesystem/lifetime integration is unfinished |
| Append hints | Workspace-local, bounded inode hints | Design policy; complete mutation integration is unfinished |
| Kernel dentries, attributes and file pages | Kernel, per mount | Owning FUSE coherence and residency qualification remain required |

There is no separate resident cache of authoritative mutable overlay rows outside
the SQLite pager. Mutable namespace indexes belong in the shared database. The
small immutable object cache does not replace canonical content formats or Store
ownership.

## 2. Existing source structure

```text
one daemon (complete process/bootstrap integration still in progress)
  |
  +-- shared CanonicalClient target
  |     +-- immutable objects keyed by ObjectId
  |     +-- recency indexes and one logical byte allowance
  |     +-- authenticated upstream object provider
  |
  +-- initialized Owner / Overlay
  |     +-- one SQLite connection and pager
  |     +-- bounded fair queues for short SQL jobs
  |     +-- Workspace-prefixed metadata, payload, scratch and custody
  |
  +-- Workspace A: root A + overlay route A
  +-- Workspace B: root B + overlay route B
  +-- native mounts: per-mount kernel caches (integration unfinished)
```

The relevant production boundaries are:

- [`workspace.rs`](../../core/crates/layerfs-workspace/src/workspace/state.rs):
  `Workspace::open` accepts `Arc<CanonicalClient>`, a filesystem root, allocation
  scope and incarnation, then binds a separate overlay route.
- [`base.rs`](../../core/crates/layerfs-workspace/src/base/view.rs): `BaseView` retains
  that root and shared client; lookup, listing and file reads use public
  cluster-one content APIs. Root binding reads the root without enumerating the
  whole namespace or materializing all file payloads.
- [`client.rs`](../../core/crates/layerfs-workspace/src/base/client.rs):
  `CanonicalClient` checks demand cardinality, object identities and processing
  windows, and releases the cache lock before upstream acquisition.
- [`cache.rs`](../../core/crates/layerfs-workspace/src/base/cache.rs): bounded standard
  maps store immutable bytes and recency. Large entries can bypass retention.
- [`owner.rs`](../../core/crates/layerfs-daemon/src/overlay/owner.rs) and
  [`credits.rs`](../../core/crates/layerfs-daemon/src/overlay/credits.rs): short SQL service
  and aggregate admission credits, including caller-retained result ownership.
- [`db.rs`](../../core/crates/layerfs-overlay/src/database/connection.rs),
  [`profile.rs`](../../core/crates/layerfs-overlay/src/database/profile.rs) and
  [`schema.sql`](../../core/crates/layerfs-overlay/sql/schema.sql): one initialized
  disposable connection, read-back settings and Workspace-prefixed relations.

The [base access guide](../../core/docs/architecture/20-workspace-base.md),
[daemon owner guide](../../core/docs/architecture/21-daemon-owner.md) and
[Store length-fact guide](../../core/docs/architecture/24-file-lengths.md)
describe the implemented seams. Source directory presence alone does not establish
an active mounted product; [the core manifest](../../core/Cargo.toml) identifies
active members.

## 3. Different branches safely reuse identical objects

Suppose Workspace A binds branch A's root and Workspace B binds branch B's root.
They may resolve the same path to different objects:

| Example | Workspace A | Workspace B | Cache behavior |
| --- | --- | --- | --- |
| Identical dependency object | Object `D` | Object `D` | One retained entry can serve both |
| Modified source object | Object `X` | Object `Y` | Distinct entries; each view resolves its own object |
| Uncommitted local write | Mutable rows under namespace A | Existing B view | A's overlay changes do not replace B's base objects |
| Commit/install in A | New base root plus later active A changes | Existing B root and changes | Immutable entries remain valid; live view and kernel coherence have separate obligations |

Sharing requires identical canonical identity. Identical logical bytes do not
promise an identical complete object layout: changed trees, metadata or construction
boundaries can produce different IDs. Reuse occurs for whichever canonical
objects actually retain the same ID.

Paths, inode serials and branch names must not be used as substitutes for immutable
object identity. Inode identity and mutable visibility are scoped to the Workspace
view. A cache hit also does not grant Branch, Store or Workspace authority;
authenticated runtime binding must enforce the allowed serving scope. The current
client delegates upstream authority to its provider and is not, by itself, proof
of complete cross-Workspace runtime authorization.

Installing a new root does not change the bytes of an existing immutable object,
so the base cache needs no blanket invalidation. FUSE pages, dentries and mutable
attributes still need the [owning coherence protocol](../../core/docs/issues/303/fuse.md#4-cache-coherence-and-lifetime-transitions).
Cache retention is not a canonical root-retention or future Store-GC lease.

## 4. Why this ownership structure is a good default

**Reuse follows actual common content.** Workspaces on nearby branches often
refer to many identical canonical objects. A shared cache can reuse those objects
without copying an entire project into each Workspace cache. This is a structural
opportunity for reuse, not a measured hit-rate or speed claim.

**The retained cache allowance does not multiply by Workspace count.** If N
Workspaces each receive a private allowance C, their aggregate allowance is N*C.
A shared cache has one daemon allowance C. This compares configured logical
retention allowances; active copies and kernel/pager memory remain additional.

**Repeated mounts reuse the initialized owners.** Root binding and overlay route
creation do not require a new database, full scan, dependency restoration or full
base import. A populated daemon cache can help later demand reads. Cached setup
must still be declared as warm in a measurement.

**Immutable keys simplify lifetime management.** Old and new roots can refer to
the same valid object. Entries are evicted by budget rather than erased whenever
one Workspace commits or closes. Closing a Workspace releases its own requests,
handles and mutable custody; other Workspaces may still benefit from cached bytes.

**Backed mutable state avoids a file-count-sized resident mirror.** Indexed SQL
rows hold overlay namespace and generation state. Demand reads and directory
pages use processing windows. This supports the intended large-namespace model
without assigning every file a permanent resident cache record; complete workload
qualification is still required.

**Management can remain per Workspace.** Resource attribution, fair service and
custody do not require a private copy of every immutable object. Count a cached
allocation once in the daemon's physical total, and report per-Workspace demand
and owned activity separately.

The tradeoff is shared eviction: a scanning Workspace can evict objects useful to
another. The SQLite pager also has no per-Workspace hot-page partition. Fair SQL
dispatch does not establish fair cache residency. Fixed private partitions are
not the default; a different residency policy needs a concrete isolation
requirement or count-driven evidence of harmful interference before selection.

## 5. Actual bounds and unfinished memory work

At the reviewed commit, `CanonicalClient::new(source, cache_bytes)` selects the
immutable cache allowance. There is no production daemon default for this
allowance yet. Entries charge their canonical byte length plus 256 bytes of
bookkeeping; the charge is logical accounting, not an allocator/RSS guarantee.
An entry whose charge exceeds the allowance bypasses cache retention.

Current demand windows are at most 4,096 IDs, 16 MiB per canonical object and
32 MiB of accepted logical batch bytes. These are processing windows, not limits
on total file, Workspace or Commit size. Cache hits clone returned bytes and
misses are retained only after identity checks. The upstream provider must bound
allocation before returning; cached-hit copies and a fetched miss batch can
coexist before the combined batch check. The 32 MiB accepted window is therefore
not a peak-heap guarantee. Aggregate concurrent object acquisition is unfinished.

The overlay defaults request a 2,048 KiB pager allowance, with `mmap_size=0`.
SQLite `cache_size` is a suggestion. Journal/dirty pages, scratch, reply buffers,
OS file caches and kernel mount state are additional resource domains.

The initial SQL owner defaults admit 8 MiB of credited work/results, reserve
64 KiB for lifecycle service and allow 16 outstanding namespace lanes, with
16 ordinary and two lifecycle slots per lane. These are active admission settings,
not a total Workspace-count limit. Credits span queued, executing and retained
results. They do not cover native producers' pre-admission copies or every
immutable/runtime/kernel allocation.

For cache entries E, ordered-map lookup and recency work are O(log E). Byte copies
pay their actual length. One insertion can evict multiple entries; cumulative
eviction work must charge each removed entry and its map operations. Bounded
retention does not establish constant latency or device throughput.

Complete daemon accounting must include retained cache allocations, outstanding
owned buffers, decoders, SQL queues, pager/journal use, runtime sessions, kernel
references, backing residency and reclamation debt. Bounds on individual pools
do not prove that their sum remains within the declared physical capacity.
The required policy is fair bounded admission and streamed progress, with exact
resource failures; it does not add a Bash timeout, total-flow cap or failed-operation
retry. Disposable overlay backing and global Store durability profiles remain
separate.

## 6. Implementation and qualification follow-through

These existing contract obligations remain open where the reviewed source has
only a seam or scoped proof:

- Wire the daemon's one shared immutable cache owner and publish its selected
  allowance with read-back/diagnostic state; preserve authorized serving scopes.
- Complete immutable attribute delivery/caching. Cheap Store length facts exist,
  but `BaseView` still opens a file view to derive regular-file length at this pin.
- Account for aggregate object-read, request/reply and runtime buffers before
  copying, including last-owner release and caller-retained results.
- Expose per-Workspace demand, queue wait, service work and mutable/custody debt
  alongside daemon-wide cache/physical totals. Current cache counters are
  client-wide; full per-Workspace attribution is unfinished.
- Prove same-project branch reuse and version separation, retained reads across
  install, concurrent cold misses, eviction pressure and unrelated progress.
- Prove repeated mount/unmount and sustained same-mount calls/Commits, automatic
  live/idle cleanup and correct kernel cache behavior under actual Linux FUSE.
- Qualify aggregate residency and physical headroom with the owning workload
  families. SQLite claims require actual EXPLAIN and correlated runtime profiles.

Any measurement follows the [measurement workflow](agent-measurement-policy.md),
[benchmark rules](benchmark_rules.md) and owning core harness, with prospectively
frozen cases/cache states and retained outcomes. This documentation performs no
measurement and claims no S0-S13 milestone completion.
