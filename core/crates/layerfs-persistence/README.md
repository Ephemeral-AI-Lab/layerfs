# LayerFS persistence

> **Status:** Current implementation under qualification; no release admission.

`layerfs-persistence` replaces `layerfs-metadata`. It composes the physical
Objects/Metadata provider and one shared history flow over a single host-local
SQLite Store. The domain crates have no engine dependencies. PostgreSQL is an
explicit `BackendSelection::Postgres` refusal before filesystem or SQL work;
there is no PostgreSQL implementation or fallback. MinIO/S3 are outside the
active product.

The twelve logical tables are `pack`; `store_policy`, `object_location`,
`metadata_value_group`, `content_signature`; and `history_meta`,
`scope_allocator`, `layer_stack`, `layer`, `commit`, `branch`, `workspace_stage`.
Schema 1 has application ID 1279677264 and 4096-byte pages. Existing incompatible
Stores are refused, without migration or automatic adoption. Files and
directories remain canonical objects, without persistent SQL mirrors.

`Handles::create`, `open_writable` and `open_read_only` require explicit
application authority. The handles share a connection/session, while each C2
publication and C5 operation has a separate bounded transaction. Published pack
bodies, descriptors, first-wins locators, pooled groups, signatures and window
changes commit together. Already sealed bodies use one final INSERT; construction
is private in the bounded C2 packer, so no published BLOB is appended or rewritten.
`Arc<Vec<u8>>` shares sealed construction storage through publication without a
second whole-body copy. Locator statements split at actual engine limits within
the same transaction. Canonical and physical bounds remain explicit.

The available profile is `sqlite-wal-full-macos-fullfsync-v1`: WAL, synchronous
FULL, foreign keys, fullfsync and checkpoint_fullfsync enabled, busy timeout zero,
mmap zero, page cache -2048 KiB, automatic checkpoint every 1000 pages, retained
journal limit 4194304 bytes. The last setting is not a WAL high-water bound.
Configuration and readback fail closed; other platforms currently refuse the
required macOS profile. The VFS is SQLite's default; its name/sync system calls
are not directly observed by the adapter. Settings establish requested SQLite
behaviour, not a power-loss claim about hardware.

SQL/VM/binding/transaction/body work is observed at actual execution boundaries.
Statement/commit/transaction wall spans overlap. Explicit `Handles::checkpoint`
returns Busy/pending frame information and must remain in the calling
application's lifecycle accounting. Close/checkpoint work cannot be moved out
of a benchmark to improve its number. Unknown outcomes quarantine the shared
session; a definite failure rolls back once. Read-only mutations and contention
never retry.

The host endpoint/transport required for a sandbox is a separate unresolved
application-adapter decision. A local Rust handle does not cross a process
boundary. Namespace entry-count collections are measured but are not yet a
whole-importer memory-bound proof.

The current plan and evidence live in
[issue 302](../../docs/issues/302/SQLITE-DESIGN-AND-PLAN.md). Performance qualification
requires all seven revised Step 10 selections and the prospectively frozen 10%
time margin; unit tests and compilation alone do not complete it.


A complete unobstructed explicit checkpoint releases unused main-file extents
past logical EOF using the safe macOS extent-transfer capability. The adapter
retains a main-file descriptor, transfers only extra allocation to an exclusively
created same-volume empty scratch, and closes/removes that scratch. No logical
Store bytes are copied or truncated. Readonly/busy cases perform no release;
unsupported volume/capability and cleanup failures are explicit, with uncertain
outcomes quarantined. Before/after allocation and total checkpoint wall are
reported. This operation is part of lifecycle accounting, not setup. Process-kill
proof after completed release is not a physical power-loss/in-syscall crash claim.
