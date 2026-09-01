# AgentFS Notes

Date: 2026-09-01

This is the working comparison note for AgentFS and LayerFS. The benchmark
numbers are in [BENCHMARK.md](BENCHMARK.md); [SPEC.md](SPEC.md) is the copied
AgentFS specification at the checked-out commit.

## What AgentFS is

AgentFS is a Rust filesystem implementation backed by SQLite. Its normal
persistent layout is a database file. It is not inherently a containerized
filesystem and it is not memory-only unless the SDK is explicitly configured
with an ephemeral/in-memory database.

On macOS, the CLI uses a userspace NFS server and mounts it over loopback. On
Linux, the CLI uses FUSE. `agentfs run` creates a persistent session delta
database under `~/.agentfs/run/<session>/delta.db`, uses the current directory as
the base, mounts the delta, runs the child process, then unmounts it. The delta
database remains for later inspection/resumption.

The macOS path also uses the native sandbox mechanism (`sandbox-exec`). A
container is optional isolation, not an AgentFS requirement.

## Persistence and storage amplification

The default data unit is a fixed 4 KiB chunk. File data is represented by rows
keyed approximately as:

```text
fs_data(inode, chunk_index, data)
```

An in-place edit normally reads/modifies/replaces only the affected chunk(s),
so a 10-byte overwrite in the middle of a 100 MiB file does not persist a new
100 MiB copy. It does persist a new SQLite transaction and a replacement row
for the affected chunk, plus inode metadata changes.

Repeated edits to the same place do not automatically create ten historical
copies in the default AgentFS database. The current chunk row is replaced. The
database may still grow due to SQLite page/WAL/journal behavior and free-page
reuse, depending on the configured SQLite mode; AgentFS itself does not provide
built-in version history or snapshots.

There are two important exceptions:

- The CLI `fs write` command removes/recreates the target file before writing,
  so repeated CLI writes are not equivalent to repeated in-place SDK `pwrite`s.
- AgentFS overlay copy-up reads the complete base file and writes it into the
  delta the first time a base file is opened for modification. A small first
  edit to a large base file can therefore materialize the whole file in the
  overlay delta. This is separate from the direct mutable-file path.

Truncating smaller deletes chunks beyond the new end and trims the final chunk.
Growing a file can be sparse: the logical size increases without writing a
full range of zero chunks.

## POSIX and safety

AgentFS exposes a filesystem-like POSIX interface through its FUSE/NFS adapter,
but it should not be described as a complete drop-in implementation of every
POSIX filesystem semantic. The supported behavior is the behavior implemented
by the AgentFS SDK and adapters; unusual locking, metadata, mount, and kernel
edge cases need workload-specific testing.

The core is implemented in Rust, which gives the usual Rust memory-safety
properties for safe code. “Memory safe” does not mean “always memory cheap”:
overlay copy-up can allocate/read a whole large file, and the mount/server and
SQLite caches add working-set overhead. The fixed chunk format is simple and
predictable, but it is not optimal for every edit pattern.

## Edit-shape behavior

| Edit shape | AgentFS behavior | Expected cost |
| --- | --- | --- |
| Overwrite in place | Replace affected 4 KiB chunk(s) | Small, plus transaction overhead |
| Append | Add/modify tail chunk(s) | Small for small appends |
| Truncate smaller | Remove rows past new end | Proportional to removed chunks |
| Truncate larger | Increase logical size, usually sparse | Small metadata operation |
| Prepend or middle insert | Caller rewrites the file; no splice primitive | O(file size) |

The benchmark demonstrates that count-changing edits have two separate costs:

- The data-structure cost: a prepend/middle insertion moves the existing bytes.
- The request/transaction cost: many NFS writes become many `pwrite` calls and
  SQLite transactions.

This is why AgentFS is not simply “slow because of network latency.” The macOS
network is loopback. For a tiny edit, mount and protocol overhead can dominate;
for a large rewrite, the lack of splice plus transaction granularity dominates.

## Ten concurrent npm installs

This was a design brainstorm, not a completed npm benchmark.

With ten independent AgentFS sessions/workspaces:

- The installs are isolated if they use separate databases/deltas.
- Each install creates many paths and metadata rows. `node_modules` stresses
  path-count and small-file operations more than large sequential I/O.
- Repeated small writes can pay per-request open/pwrite/SQLite-transaction costs,
  especially through the mounted macOS path.
- Identical package file contents are not globally deduplicated by the default
  AgentFS fixed-chunk database. Ten independent databases therefore do not get
  LayerFS-style cross-workspace CAS sharing.
- Registry downloads, npm's own concurrency, CPU, and disk contention may be
  larger than the filesystem cost in a cold install.

If the ten installs share one AgentFS database, they also share one namespace,
so they need distinct directory prefixes or separate databases. That is an
isolation choice, not free parallel branching.

## AgentFS versus LayerFS

AgentFS is the simpler choice when the requirement is “run a process in a
persistent mutable filesystem.” It has a direct SQLite representation, ordinary
file operations, and inexpensive in-place edits when the caller already knows
the offsets.

LayerFS is the stronger choice when the requirement includes durable branches,
commits, immutable history, cross-branch reuse, or efficient rewrites. Its
content layer uses content-defined chunking (FastCDC, approximately 8–32 KiB
chunks with a 16 KiB target), immutable content-addressed objects, and rope-like
file edits/splices. A prepend or middle insertion can preserve and reuse most
unchanged content instead of recording a new copy of the whole file.

The CAS+CDC complexity is worth it when those properties matter. It is not
automatically worth it for a single mutable current-state filesystem: AgentFS's
fixed chunks and SQLite rows are easier to operate and are adequate for small
positional edits.

The latest LayerFS native-host diagnostic measured a 32 MiB prepend at about
218.5 ms for the complete lifecycle. Its receipt reported 1,730 reused objects
and 17 inserted objects. This is directionally better than the AgentFS mounted
results, but it is not a strict apples-to-apples product claim: the mount,
workload, commit path, and benchmark harness differ. In particular, AgentFS's
2.75 s streamed-prepend result includes 32 write requests; its batched version
was 0.61 s.

LayerFS has its own scaling trade-off. The current end-to-end candidate build
still materializes manifests for the whole path set, so commit cost can grow
with total path count even when content reuse is excellent. That is a measured
design risk for very large `node_modules` trees, not proof that npm install is a
blocker: the current checkout did not contain a completed npm install + commit
benchmark, and its recorded npm scenarios were skipped when npm was unavailable.

## Bottom line

- For ordinary mutable agent workspaces: AgentFS is straightforward and fast
  enough, especially when operations stay in one mounted session or SDK
  process.
- For tiny in-place changes: AgentFS updates only affected fixed chunks, but
  mount/protocol/transaction overhead is visible.
- For prepend/middle insertion: AgentFS must rewrite bytes; batching matters
  greatly, and there is no native splice optimization.
- For persistence with branching, history, and deduplicated large rewrites:
  LayerFS's CAS+CDC is the more appropriate architecture, with path-count
  commit scaling still needing targeted npm/node_modules measurements.
