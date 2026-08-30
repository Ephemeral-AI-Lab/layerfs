# LayerFS Architecture

> **Historical and superseded.** The binding architecture is
> [`docs/v2/spec.md`](docs/v2/spec.md).

## 1. Purpose

LayerFS is an immutable, content-addressed logical filesystem that can be
materialized into a host directory and later captured back into a new root.

The first product workflow is DeltaGit-oriented:

~~~text
LayerFS root
    ↓
materialize into a native workspace
    ↓
user or DeltaGit edits files
    ↓
capture the workspace
    ↓
new LayerFS root and delta
~~~

The architecture separates five responsibilities:

~~~text
layerfs-core   = what LayerFS means
layerfs-engine = where LayerFS data is stored
layerfs-os     = how the host filesystem works
layerfs-vfs    = how LayerFS is projected into a host filesystem
layerfs-sdk    = what users call
~~~

The first qualified composition is:

~~~text
macOS + APFS + Rust + SQLite rollback journal
~~~

The logical format must not depend on macOS, APFS, SQLite, or any other host
or storage implementation.

## 2. Component graph

~~~mermaid
flowchart TD
    A["DeltaGit or application"] --> B["layerfs-sdk"]
    B --> C["layerfs-vfs"]
    C --> D["layerfs-core"]
    C --> E["layerfs-engine"]
    C --> F["layerfs-os"]
    E --> D
    E --> G["SQLite"]
    F --> H["macOS / APFS"]
    F --> I["Linux / ext4 / XFS"]
    F --> J["Windows / NTFS"]
~~~

The dependency direction is:

~~~text
layerfs-sdk
    ↓
layerfs-vfs
    ├── layerfs-core
    ├── layerfs-engine ────→ layerfs-core
    └── layerfs-os
~~~

The core must not depend on the engine, OS adapter, VFS, or SDK. SQLite must
not know about APFS. The OS adapter must not know about SQLite. The VFS layer
is the composition point.

## 3. Repository layout

~~~text
layerfs/
├── Cargo.toml
├── crates/
│   ├── layerfs-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs
│   │       ├── limits.rs
│   │       ├── format/
│   │       ├── identity/
│   │       ├── object/
│   │       ├── cdc/
│   │       ├── cas/
│   │       ├── content/
│   │       ├── cow/
│   │       └── delta/
│   │
│   ├── layerfs-engine/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs
│   │       ├── transaction.rs
│   │       └── sqlite/
│   │           ├── mod.rs
│   │           ├── connection.rs
│   │           ├── schema.rs
│   │           ├── objects.rs
│   │           ├── roots.rs
│   │           ├── deltas.rs
│   │           └── capture.rs
│   │
│   ├── layerfs-os/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs
│   │       ├── capabilities.rs
│   │       └── platform/
│   │           ├── mod.rs
│   │           ├── unix.rs
│   │           ├── macos.rs
│   │           ├── linux.rs
│   │           └── windows.rs
│   │
│   ├── layerfs-vfs/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── materialize.rs
│   │       └── capture.rs
│   │
│   └── layerfs-sdk/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── error.rs
│           ├── materialize.rs
│           └── capture.rs
└── docs/
~~~

The workspace also contains `tools/layerfs-eval`, a non-product evaluation
binary. It may depend on `layerfs-os` for host observations, but product
crates do not depend on the evaluation tool.

The tree describes ownership, not a requirement to create empty modules for
future platforms. Unsupported platform files should be added only when their
implementation and direct qualification are ready.

## 4. layerfs-core: logical LayerFS

layerfs-core is pure logical LayerFS. It should be usable without a database
or native filesystem and should eventually be able to compile for WASM where
the selected dependencies permit it.

### Owns

- canonical paths and ordering;
- size and resource limits;
- canonical object encoding and decoding;
- domain-separated identities and typed IDs;
- content-defined chunking;
- immutable CAS semantics;
- logical file and directory content;
- authenticated copy-on-write tree mutation;
- deltas, changed paths, tombstones, parent roots, and new roots;
- bounded streaming and range-rejoin verification.

The core converts logical content into canonical objects and roots:

~~~text
byte stream + logical mutation
        ↓
layerfs-core
        ↓
canonical objects + delta + new root
~~~

CDC and CAS are application-level algorithms. They do not depend on SQLite,
PostgreSQL, a file pack, APFS, or POSIX.

### Must not own

- operating-system file handles;
- native paths or directory enumeration;
- SQLite connections or SQL statements;
- PostgreSQL clients;
- FUSE or OverlayFS;
- physical chunk placement;
- materialization policy.

The core may define bounded reader and writer ports, but it must not implement
the concrete storage or host filesystem behind them.

## 5. layerfs-engine: durable storage

layerfs-engine stores already-validated LayerFS values. SQLite is the first
backend and is an implementation detail of this crate.

### Owns

- canonical object and chunk persistence;
- root and delta persistence;
- authenticated object lookup;
- bounded object range reads;
- no-replace object insertion;
- capture transactions;
- root publication;
- durability and journal configuration;
- connection management and backend error conversion.

The initial physical layout may store canonical object bytes as immutable BLOB
records in SQLite. The logical engine contract must not depend on that choice.
If benchmarks later show that large BLOB rows are the bottleneck, the engine
may move object bytes to a file-backed carrier behind the same semantic port.

### Required semantic operations

The exact Rust names can change, but the engine needs the following semantic
capabilities:

~~~text
load_root(root)
read_object_range(object, range)
begin_capture(parent)
put_object_if_absent(id, canonical_bytes)
write_delta(delta)
commit_root(root)
~~~

The engine must guarantee:

- objects are immutable after successful publication;
- an existing object is authenticated before reuse;
- object insertion is no-replace and idempotent;
- referenced objects are durable before a root becomes visible;
- a failed capture cannot advance the parent head;
- root publication is one atomic engine transition;
- SQL schema and database identifiers are never LayerFS identity inputs.

### Must not own

- CDC boundaries;
- object identity rules;
- canonical bytes;
- materialization or capture policy;
- host filesystem operations;
- public SDK types.

The engine can depend on layerfs-core types and validation. The core cannot
depend on the engine.

## 6. layerfs-os: host filesystem mechanics

layerfs-os is the narrow adapter between LayerFS projection code and a real
operating system. It is not a POSIX implementation and not another storage
engine.

### Owns

- opening, reading, and writing host files;
- directory creation and enumeration;
- symlink creation and inspection;
- metadata and file identity observations;
- safe replacement and rename operations;
- removal operations;
- synchronization operations;
- native error classification;
- platform capability detection;
- APFS clonefile support when later qualified;
- Linux reflink support when later qualified;
- Windows-specific filesystem mechanics.

The adapter exposes only the operations needed by layerfs-vfs, such as:

~~~text
open_file
read_range
write_file
create_directory
create_symlink
list_directory
read_metadata
file_identity
replace_file
rename
remove
sync
capabilities
~~~

The adapter should stay thin. If it merely wraps std::fs without adding
platform behavior, it does not justify an additional abstraction.

### Platform organization

~~~text
platform/unix.rs
    genuinely shared Unix mechanics

platform/macos.rs
    macOS and APFS behavior

platform/linux.rs
    Linux filesystem behavior

platform/windows.rs
    Windows, NTFS, and ReFS behavior
~~~

unix.rs must not be treated as proof that macOS and Linux are equivalent.
APFS, ext4, XFS, OverlayFS, and their race and identity semantics require
separate qualification.

### Must not own

- LayerFS object IDs or roots;
- CDC or CAS;
- SQLite or PostgreSQL;
- materialization decisions;
- full POSIX compatibility;
- VFS mount policy.

## 7. layerfs-vfs: filesystem-facing projection

layerfs-vfs is the composition layer that turns logical LayerFS roots into
host filesystem views and captures host changes back into LayerFS.

The name VFS refers to the filesystem-facing view. It does not mean that the
first implementation must be a kernel VFS, FUSE filesystem, or complete POSIX
filesystem.

### Owns

- materializing a root to a native directory;
- checking whether a destination already matches a root;
- skipping unchanged files during warm materialization;
- incrementally updating changed paths;
- choosing copy versus an optional host clone optimization;
- capturing changed paths and bounded changed-file streams;
- coordinating CDC, CAS, COW, and delta construction;
- coordinating engine transactions;
- maintaining workspace provenance;
- returning opaque checkpoints and change summaries.

The VFS layer decides what should happen. The OS layer performs the host
operation.

~~~text
layerfs-vfs:
    “This destination file already matches the target object; skip it.”

layerfs-os:
    “Inspect this path and report its host state.”

layerfs-vfs:
    “This file changed; replace it safely.”

layerfs-os:
    “Write, synchronize, and atomically replace the path.”
~~~

### Initial projection

The first implementation is a normal native directory:

~~~text
layerfs-vfs::materialize
layerfs-vfs::capture
        ↓
layerfs-os::platform::macos
        ↓
macOS + APFS
~~~

Future projection techniques may include:

~~~text
layerfs-vfs::fuse          # later, separately qualified
layerfs-vfs::overlayfs     # later, Linux-only
~~~

They must not be added as empty scaffolding. Each projection needs its own
correctness, race, and performance qualification.

### Must not own

- canonical object encoding;
- CDC or CAS implementation;
- SQLite schema;
- platform-specific syscalls directly;
- unbounded whole-file buffers;
- hidden worker threads or retry loops;
- silent loss of unsupported file types or metadata.

## 8. layerfs-sdk: public facade

layerfs-sdk exposes the small public workflow and hides implementation details.

The initial public operations are:

~~~text
LayerFs::open
LayerFs::materialize
Workspace::capture
Workspace::discard
~~~

The public API should use opaque handles for roots, workspaces, and
checkpoints. It should not expose:

- SQLite connections;
- SQL transactions;
- PostgreSQL clients;
- CDC profiles;
- CAS internals;
- object tables;
- tree node types;
- native OS handles;
- FUSE or OverlayFS details.

For the DeltaGit workflow, the SDK does not initially need a broad general
purpose virtual filesystem API. The essential operations are:

~~~text
materialize(root, destination)
capture(workspace)
~~~

Internal range reads remain available to the VFS and engine so large files are
not reconstructed in memory. A public direct-read API can be added only when a
real caller requires it.

## 9. Materialization flow

~~~mermaid
sequenceDiagram
    participant S as SDK
    participant V as layerfs-vfs
    participant E as layerfs-engine
    participant C as layerfs-core
    participant O as layerfs-os
    participant H as Host filesystem

    S->>V: materialize(root, destination)
    V->>E: load_root(root)
    E-->>V: authenticated root and tree records
    V->>C: decode and traverse logical tree
    C-->>V: ordered files and metadata
    V->>O: inspect destination
    O->>H: stat and read metadata
    H-->>O: destination state
    O-->>V: host observations
    V->>E: read_object_range(object, range)
    E-->>V: bounded byte range
    V->>O: create or replace path
    O->>H: write, rename, and sync
    H-->>O: result
    O-->>V: result
    V-->>S: workspace handle
~~~

Materialization requirements:

- cold materialization must produce the exact expected tree;
- warm no-op materialization must not rewrite unchanged files;
- incremental materialization must update only changed paths where provenance
  permits;
- file bytes must be streamed or range-read;
- temporary files and replacement authority must be safe against races;
- APFS clone operations are optional optimizations, never correctness
  requirements;
- a failed materialization must not mutate the stored LayerFS root.

## 10. Capture flow

~~~mermaid
sequenceDiagram
    participant S as SDK
    participant V as layerfs-vfs
    participant O as layerfs-os
    participant H as Host filesystem
    participant C as layerfs-core
    participant E as layerfs-engine

    S->>V: capture(workspace)
    V->>O: enumerate and inspect workspace
    O->>H: list, stat, and read identity
    H-->>O: changed-path evidence
    O-->>V: frozen change evidence
    V->>O: stream changed file
    O->>H: bounded read
    H-->>O: byte stream
    O-->>V: byte stream
    V->>C: CDC, CAS, COW, and delta construction
    C-->>V: objects, delta, and new root
    V->>E: begin capture transaction
    V->>E: put objects if absent
    V->>E: write delta
    V->>E: commit root
    E-->>V: durable checkpoint
    V-->>S: checkpoint and change summary
~~~

Capture requirements:

- changed-path evidence is frozen before content capture;
- file identity is revalidated before consuming content;
- small edits reuse authenticated unchanged chunks;
- changed content is streamed through CDC and CAS;
- objects, delta, and root publication use one engine transaction;
- the workspace head advances only after the root is durable;
- a failed capture cannot publish a partial root;
- explicit discard is available and Drop is not the only cleanup path.

## 11. Logical file surface and POSIX

LayerFS defines portable logical filesystem semantics. It does not attempt to
implement all POSIX behavior.

The initial logical surface is:

| Feature | Initial status |
|---|---|
| Regular files | Required |
| Directories | Required |
| File bytes | Required |
| Empty directories | Required |
| Create, update, and delete | Required |
| Symlinks | Required if needed by the DeltaGit format |
| Executable bit | Required if Git mode compatibility requires it |
| Rename identity | Optional; may initially be delete plus create |
| Ownership | Not required initially |
| ACLs | Not required initially |
| Extended attributes | Not required initially |
| Hard links | Not required initially |
| Sockets, devices, and FIFOs | Not required initially |
| Full POSIX compliance | Not a product requirement |

POSIX and Windows differences are handled by layerfs-os and the selected VFS
projection. They must not alter canonical object identity or tree meaning.

Unsupported host features must be explicit. The system must not silently drop
metadata or file kinds that the selected logical format claims to preserve.

## 12. Platform support

Support is claimed by composition and direct qualification, not by compiling
conditional code.

| Composition | Initial status |
|---|---|
| macOS + APFS + SQLite + native directory VFS | First qualified target |
| macOS + non-APFS | Later qualification |
| Linux + ext4/XFS + native directory VFS | Later qualification |
| Linux + OverlayFS | Later optional projection |
| Linux + FUSE | Later optional projection |
| Windows + NTFS/ReFS + native directory VFS | Later qualification |
| WASI | Core first; host adapter later |
| Browser WASM | Core-only initially |

The first correctness path must use ordinary APFS file operations. APFS
clonefile is an optimization candidate, not a required primitive.

## 13. Storage and memory boundaries

Chunks are stored by layerfs-engine, not by layerfs-core, layerfs-vfs, or
layerfs-os.

Initial physical storage:

~~~text
layerfs-engine::sqlite
    ├── object metadata
    ├── canonical object bytes / chunks as BLOBs
    ├── roots
    └── deltas
~~~

The process memory bound is not supplied by SQLite alone. Memory comes from:

~~~text
LayerFS streaming buffers
SQLite page cache and statements
CDC/CAS temporary state
VFS file buffers
native OS page cache
~~~

The implementation must keep userspace buffers bounded and measure logical
memory, RSS/PSS, SQLite cache behavior, temporary storage, and host page-cache
effects separately.

The initial SQLite configuration disables WAL because the first workflow does
not require SQLite backup or WAL-based replication:

~~~sql
PRAGMA journal_mode = DELETE;
PRAGMA synchronous = FULL;
PRAGMA temp_store = FILE;
PRAGMA mmap_size = 0;
~~~

The final cache values must be selected from benchmark evidence and bounded
explicitly per connection.

## 14. Performance boundaries

The performance requirements are owned by different components:

~~~text
CDC/CAS work              → layerfs-core
SQL round trips           → layerfs-engine
object range reads        → layerfs-engine + layerfs-core
file rewrites             → layerfs-vfs
host filesystem latency   → layerfs-os
buffering and memory      → the owning boundary
~~~

The main acceptance properties are:

- cold materialization is measured separately from warm materialization;
- warm no-op materialization does not rewrite unchanged files;
- large-file reads use bounded range access;
- small edits do not scale linearly with total file size when the format and
  change-evidence path permit edit-sized work;
- object reuse is authenticated and counted;
- SQLite transactions and statements are measured directly;
- peak userspace memory remains bounded;
- performance claims are made only from the benchmark contract.

The architecture must not hide whole-file rescans, repeated SQL round trips,
or unbounded buffers behind a convenient API.

## 15. Non-goals for the first implementation

Do not build these before the macOS/APFS native-directory path is correct and
benchmarked:

- a full POSIX implementation;
- FUSE;
- OverlayFS;
- Windows materialization;
- browser-WASM filesystem support;
- PostgreSQL;
- the recovered custom storage engine;
- a general-purpose VFS syscall API;
- hidden background workers;
- automatic provider fallback;
- a second object storage layout.

The architecture leaves room for these later without placing their policy in
the core format.

## 16. Initial implementation order

1. Build layerfs-core and its canonical tests.
2. Build layerfs-engine with SQLite object and root transactions.
3. Build the macOS/APFS portion of layerfs-os.
4. Build layerfs-vfs::materialize for a native directory.
5. Build layerfs-vfs::capture for the DeltaGit workflow.
6. Expose the minimal SDK.
7. Run cold, warm, read, edit, write, memory, and storage-growth evaluation.
8. Optimize one measured bottleneck at a time.
9. Qualify additional platforms or storage backends only when required.

The central invariant is:

> LayerFS defines the logical data model; the engine stores it; the VFS
> projects it; the OS adapter performs host operations; and the SDK exposes
> only the workflow users need.
