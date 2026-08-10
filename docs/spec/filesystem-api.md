# Filesystem API specification

| Field | Value |
| --- | --- |
| Status | Draft |
| Target | `@ephemeralai/fs` version 0.1 |
| Last updated | 2026-08-10 |

This document defines the portable filesystem and database-adapter contracts
for Ephemeral AI FS. It is normative for paths, namespace operations,
metadata, I/O, lifecycle, errors, and conformance. Storage representation and
branch publication are specified separately.

The words MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY have the meanings stated
in the repository-level [`SPEC.md`](../../SPEC.md).

## Scope and portability boundary

`@ephemeralai/fs` MUST expose a host-independent, asynchronous filesystem API.
The core package MUST NOT import Cloudflare Durable Object, Ephemeral AI
Computer, FUSE, RPC, container, Node.js `Buffer`, or Node.js filesystem types.
It MAY use platform-neutral JavaScript types, including `Uint8Array`, WHATWG
`ReadableStream`, `AbortSignal`, and `Error`.

The core package MUST depend on the database contract in this document rather
than a concrete SQLite driver. The Node.js and Durable Object integrations
MUST live in their adapter packages. A host MAY transport the core API through
remote calls or add convenience helpers built from its primitives, but it MUST
NOT define different filesystem semantics. In Ephemeral AI Computer,
`workspace.fs` MUST expose this contract and replace the current
`WorkspaceFilesystem` contract. Transport wrappers remain host code and MUST
mirror the portable methods, results, and errors one-for-one.

Computer MAY retain a host-owned DOFS comparison adapter. When selected, that
adapter MUST implement the common methods, results, and errors in this contract
and MUST report branch-only capabilities as unsupported. It MUST use storage
isolated from Ephemeral AI FS and MUST NOT become an automatic fallback. The
Ephemeral AI FS core and adapters MUST NOT import or configure DOFS.

Version 0.1 includes path-based operations. Persistent file descriptors,
memory mapping, advisory locking, ownership, access-control enforcement,
extended attributes, device nodes, sockets, FIFOs, sparse-file reporting,
search helpers, and watch subscriptions are deferred unless this document
states otherwise.

## Common types

The following declarations describe the required public shape. An
implementation MAY add readonly fields or overloads before version 1.0, but it
MUST NOT change the meaning of a field defined here.

```ts
export type FileType = "file" | "directory" | "symlink";

export type FileContent =
  | string
  | Uint8Array
  | ReadableStream<Uint8Array>;

export interface FileStat {
  /** Stable within one filesystem for the lifetime of the inode. */
  readonly id: string;
  readonly name: string;
  readonly type: FileType;
  readonly mode: number;
  readonly size: number;
  readonly nlink: number;
  readonly mtimeMs: number;
  readonly ctimeMs: number;
  readonly birthtimeMs: number;
  isFile(): boolean;
  isDirectory(): boolean;
  isSymbolicLink(): boolean;
}

export interface DirectoryEntry {
  readonly name: string;
  readonly parentPath: string;
  readonly type: FileType;
  isFile(): boolean;
  isDirectory(): boolean;
  isSymbolicLink(): boolean;
}

export interface ReadTextOptions {
  readonly encoding: "utf8";
}

export interface ReadRangeOptions {
  readonly offset: number;
  readonly length: number;
}

export interface ReadStreamOptions {
  readonly offset?: number;
  readonly length?: number;
  readonly signal?: AbortSignal;
}

export interface WriteFileOptions {
  readonly mode?: number;
  readonly exclusive?: boolean;
  readonly signal?: AbortSignal;
}

export interface MkdirOptions {
  readonly recursive?: boolean;
  readonly mode?: number;
}

export interface ReaddirOptions {
  readonly limit?: number;
  readonly startAfter?: string;
}

export interface RmOptions {
  readonly recursive?: boolean;
  readonly force?: boolean;
}
```

All numeric byte offsets, lengths, sizes, timestamps, limits, and modes MUST
be finite non-negative safe integers unless an operation explicitly permits a
negative value. No operation in version 0.1 permits a negative value. Invalid
numbers MUST fail with `EINVAL` before visible state changes.

## Paths

### Grammar

A public path MUST be a JavaScript string with the following grammar:

```text
path       = "/" *(segment "/") [segment]
segment    = 1*(Unicode scalar value other than "/" or U+0000)
```

For API convenience, repeated separators, `.` segments, `..` segments, and a
trailing separator are accepted as inputs and canonicalized as described
below. They are not stored as directory-entry names.

A path MUST:

- be non-empty and begin with `/`;
- contain no U+0000 NUL character;
- contain only well-formed Unicode scalar values; and
- remain within all configured path limits after UTF-8 encoding.

Names are case-sensitive. The implementation MUST preserve the supplied
Unicode scalar values and MUST NOT perform Unicode normalization or locale
case folding. Backslash (`\\`) is an ordinary name character, not a
separator. An empty segment, `.`, or `..` cannot be created as a directory
entry.

### Canonicalization

Before namespace lookup, every operation MUST canonicalize each path as
follows:

1. Split on `/`.
2. Remove empty and `.` segments.
3. For each `..`, remove the preceding retained segment.
4. Reject the path with `EINVAL` if `..` would move above the root.
5. Join retained segments with one `/`, prefixed by `/`.
6. Produce `/` when no segment remains.

Consequently, `/a//b/`, `/a/./b`, and `/a/x/../b` identify the same entry.
The original spelling MUST NOT create a second namespace identity. Errors
SHOULD report the canonical path when canonicalization succeeded and the
original input when it did not.

`symlink()` is the only exception for path-like input: it MUST store its
`target` string verbatim and MUST NOT canonicalize it at creation time.

### Ordering

Directory names MUST be ordered by unsigned lexicographic comparison of their
UTF-8 bytes. Ordering MUST NOT depend on locale, adapter, database collation,
or insertion order. The same ordering applies to pagination and conformance
fixtures.

## Root semantics

The root directory `/` MUST exist after a new filesystem is opened and MUST
have a stable inode identity for the lifetime of that filesystem. Its `name`
in `stat()` and `lstat()` results MUST be the empty string.

The root:

- MUST be a directory;
- MUST NOT be renamed, unlinked, replaced, hard-linked, or used as a symlink
  destination;
- MUST NOT be removed, including with `{ recursive: true, force: true }`;
- MUST cause `mkdir("/")` to fail with `EEXIST`; and
- MUST make `mkdir("/", { recursive: true })` a successful no-op.

An attempt to remove or rename the root MUST fail with `EPERM`. An attempt to
write file content at the root MUST fail with `EISDIR`.

## Namespace and link model

### Regular files

A regular file has an inode identity, byte content, mode, timestamps, and one
or more directory entries. Content is an uninterpreted byte sequence. The
core MUST NOT infer a media type, newline convention, or character encoding.

### Directories

A directory maps unique names to inode identities. Directories MUST NOT have
hard links in version 0.1. Directory `size` MUST be `0`; callers MUST NOT use
it to infer the number or encoded size of children.

### Symbolic-link operations

A symbolic link stores a target string. The target MAY be absolute, relative,
dangling, or cyclic. An absolute target resolves from `/`. A relative target
resolves from the directory containing the link. Resolution MUST process `.`
and `..` in the target and MUST prevent escape above the root.

Operations MUST follow symbolic links in intermediate path segments. `stat`,
`readFile`, `readRange`, `readStream`, `writeFile`, `writeRange`, `truncate`,
`readdir`, `chmod`, and `link` at its source MUST also follow a final symbolic
link. `lstat`, `readlink`, `unlink`, `rm`, `rename` at its source, and all
creation destinations MUST operate on or inspect the final directory entry
without following it.

Resolution MUST fail with `ELOOP` after more than 40 symbolic-link traversals
for one operation. A dangling final link followed by an operation MUST fail
with `ENOENT`. Creating through a dangling final link is deferred: in version
0.1, `writeFile` on a dangling link MUST fail with `ENOENT` rather than create
the target.

The `size` reported by `lstat` for a symbolic link MUST equal the UTF-8 byte
length of its stored target. Its initial mode MUST be `0o777`. Permission bits
on a symbolic link are informational and cannot be changed in version 0.1.

### Hard links

`link(existingPath, newPath)` MUST create an additional directory entry for
the source regular-file inode. The source MAY itself be reached through a
symbolic link; the operation follows that source link. Hard links to
directories or symbolic-link inodes MUST fail with `EPERM`.

All hard links to a file MUST expose the same content, mode, inode identity,
and inode timestamps. `nlink` MUST equal the current number of directory
entries referring to the inode. Unlinking one name MUST preserve the inode and
content while another link remains. Creating a hard link MUST fail with
`EEXIST` if the destination already exists.

`FileStat.nlink` MUST have these exact values:

- for a regular file, the number of live directory entries in the selected
  main or branch view that refer to its inode;
- for a non-root directory, `1`;
- for a symbolic link, `1`; and
- for the root directory, `1`.

The value MUST be computed from the selected view. Private branch aliases MUST
affect branch results without changing main results before publication.

## Metadata and clocks

Modes contain only the low 12 POSIX permission and special bits (`0o7777`).
File-type bits MUST NOT be stored in or returned by `mode`. Values supplied by
a caller MUST be masked with `0o7777`. Default modes are `0o644` for regular
files, `0o755` for directories, and `0o777` for symbolic links.

Modes are metadata only in version 0.1. The core has no user, group, or
effective identity and MUST NOT reject reads or writes based on mode bits.
Ownership, `chown`, ACLs, and an `access` operation are deferred.

Timestamps MUST be integer Unix epoch milliseconds. `EphemeralFS.open` MAY
receive a clock for deterministic testing; otherwise it MUST use `Date.now`.
One logical mutation MUST sample the clock once. To tolerate a clock moving
backward, a modified inode's new `mtimeMs` or `ctimeMs` MUST NOT be less than
its prior value.

Timestamp behavior is:

- A new inode receives equal `birthtimeMs`, `mtimeMs`, and `ctimeMs` values
  from the mutation's one clock sample.
- `birthtimeMs` is set when an inode is created and MUST never change.
- A content write or truncate updates the file's `mtimeMs` and `ctimeMs`.
- `chmod` updates `ctimeMs` but MUST NOT update `mtimeMs`.
- Adding or removing a directory entry updates the containing directory's
  `mtimeMs` and `ctimeMs`.
- Adding or removing a hard link updates the linked file's `ctimeMs`.
- Renaming updates both affected parent directories. It also updates the moved
  inode's `ctimeMs`; it MUST NOT update a regular file's `mtimeMs`.
- Reading and listing MUST NOT update timestamps. Access time is deliberately
  not exposed in version 0.1.

Metadata changes and the namespace or content change that caused them MUST be
part of the same atomic transaction.

## Public filesystem interface

```ts
export interface EphemeralFilesystem {
  readFile(path: string): Promise<Uint8Array>;
  readFile(path: string, options: ReadTextOptions): Promise<string>;
  readRange(path: string, options: ReadRangeOptions): Promise<Uint8Array>;
  readStream(
    path: string,
    options?: ReadStreamOptions,
  ): Promise<ReadableStream<Uint8Array>>;

  writeFile(
    path: string,
    content: FileContent,
    options?: WriteFileOptions,
  ): Promise<void>;
  writeRange(path: string, offset: number, content: Uint8Array): Promise<void>;
  replaceRange(
    path: string,
    offset: number,
    deleteLength: number,
    insertBytes: Uint8Array,
  ): Promise<void>;
  truncate(path: string, size?: number): Promise<void>;

  mkdir(path: string, options?: MkdirOptions): Promise<void>;
  readdir(path: string, options?: ReaddirOptions): Promise<DirectoryEntry[]>;
  stat(path: string): Promise<FileStat>;
  lstat(path: string): Promise<FileStat>;
  chmod(path: string, mode: number): Promise<void>;
  link(existingPath: string, newPath: string): Promise<void>;
  symlink(target: string, path: string): Promise<void>;
  readlink(path: string): Promise<string>;
  rename(oldPath: string, newPath: string): Promise<void>;
  unlink(path: string): Promise<void>;
  rm(path: string, options?: RmOptions): Promise<void>;

  close(): Promise<void>;
}

export interface OpenFilesystemOptions {
  readonly database: FilesystemDatabaseAdapter;
  readonly clock?: () => number;
  readonly filesystem?: Partial<FilesystemLimits>;
  readonly storage?: Partial<StorageLimits>;
  readonly runtime?: Partial<RuntimeLimits>;
  readonly format?: Partial<StorageFormatOptions>;
  /** Defined normatively by the branches and publication specification. */
  readonly branch?: Partial<BranchConfiguration>;
  readonly observer?: FilesystemObserver;
  /** Close an externally supplied adapter when the filesystem closes. */
  readonly ownsDatabase?: boolean;
}

export interface EphemeralFilesystemAdministration {
  readonly capabilities: FilesystemCapabilities;
  readonly maintenance: FilesystemMaintenance;
}

export declare class EphemeralFS
  implements EphemeralFilesystem, EphemeralFilesystemAdministration
{
  static open(options: OpenFilesystemOptions): Promise<EphemeralFS>;
  // Methods are those declared by EphemeralFilesystem.
}
```

`BranchConfiguration` is owned by
[`branches-and-publication.md`](./branches-and-publication.md). It contains the
effective branch identifier, operation identifier, active-branch,
changed-path, conflict, terminal-retention, and publication-result-retention
limits defined there. `OpenFilesystemOptions.branch` is the portable creation
and open path for caller-selected branch configuration.

Administration is deliberately separate from `EphemeralFilesystem`. The root
`EphemeralFS` object exposes global maintenance and capabilities; a branch
filesystem handle MUST NOT inherit database-wide garbage collection,
verification, ownership, or close authority.

Every method MUST return or reject a promise. An implementation MAY perform
argument checks before constructing the promise, but callers MUST NOT depend
on synchronous throws.

### Reading complete files

`readFile(path)` MUST return a new `Uint8Array` containing the complete bytes
of the file. Mutating the returned array MUST NOT mutate stored content.

`readFile(path, { encoding: "utf8" })` MUST decode the complete bytes with the
WHATWG UTF-8 decoder in non-fatal mode. Ill-formed byte sequences therefore
produce U+FFFD replacement characters. No other encoding is included in
version 0.1.

Complete materialization MUST fail with `EFBIG` if the file exceeds
`maxMaterializedBytes`. Callers MUST use `readRange` or `readStream` for a
larger file.

### Range reads

`readRange` reads at most `length` bytes beginning at `offset`. It MUST return
an empty array when `length` is zero or `offset` is at or beyond end of file.
It MUST return the available suffix without padding when the requested range
crosses end of file. The returned array MUST NOT alias mutable internal
storage.

The filesystem MUST resolve and type-check the path even for a zero-length
request. A directory MUST fail with `EISDIR` and a missing path with `ENOENT`.
`length` MUST NOT exceed `maxMaterializedBytes`.

### Streaming reads

`readStream` MUST return a WHATWG `ReadableStream<Uint8Array>`. Its `offset`
defaults to `0`; an omitted `length` means through end of file. Its range and
EOF behavior otherwise matches `readRange`.

The returned stream MUST represent a snapshot of the file selected while
`readStream` resolves. A concurrent overwrite, rename, unlink, publication, or
garbage-collection pass MUST NOT mix revisions or cause already selected
content to disappear.

Before `readStream` resolves, the implementation MUST select a representation
for the requested snapshot and durably acquire the read-stream lease defined
by the storage specification. Its collision-resistant identifier and secret
owner nonce MUST bind the selected revision, inode identity, node token,
representation roots, and stream identifier.

For immutable content, the lease MUST root the selected manifest. The manifest
transitively protects its objects, so opening a stream MUST NOT create one
membership row per object or verify every object before resolving. Each
selected object MUST instead be loaded and verified before its bytes are
enqueued.

For branch pages or patches, the lease or snapshot pin MUST protect the exact
selected base manifest and overlay rows without materializing a new complete
file. The acquisition transaction MUST revalidate the branch generation.
Publication, discard, and garbage collection may detach those rows but MUST
NOT reclaim them until the stream releases its lease.

Lease selection and activation MUST perform bounded work independent of the
logical file size. `readStream` MUST NOT resolve until the lease is active, but
it MUST NOT scan, hash, copy, or materialize the complete file merely to open
the stream.

The initial TTL is the effective `readLeaseMs` storage limit. While a stream
remains readable, the implementation MUST renew early. Renewal MUST match the
lease identifier, owner nonce, and prior expiry in one write transaction and
extend from `max(effectiveNow, priorExpiry)`. It MUST NOT revive an expired or
released lease.

If renewal cannot commit before expiry, or observes an owner mismatch, the
stream MUST error before loading another object. Fully consuming, canceling,
erroring, or closing the stream MUST release the lease idempotently for that
owner. Every acquisition, activation, membership change, renewal, release, or
expiry MUST increment the storage root mutation generation. A process crash
MAY leave a lease until expiry; bounded maintenance performs expiry.

Because version 0.1 leases are durable rows, `readStream` on a read-only
adapter MUST fail with `EROFS`. `readFile` and `readRange` remain available
through one bounded `"read"` transaction. A future adapter capability MAY
define an equally durable external lease store without changing stream
snapshot semantics.

Stream chunks MUST be non-empty `Uint8Array` values and MUST be no larger than
`preferredStreamChunkBytes`. The implementation MUST honor backpressure and
the aggregate runtime budget. It MUST NOT retain already emitted chunks merely
because a scan is sequential. If
the signal is already aborted, or becomes aborted while reading, the stream
MUST error with an `AbortError` and release resources.

### Whole-file writes

`writeFile` MUST UTF-8 encode string input without a byte-order mark. A
`Uint8Array` input MUST be treated as the bytes visible when the method is
called; later caller mutation MUST NOT change the committed file. A stream
input MUST contain only `Uint8Array` chunks.

If the final target does not exist, `writeFile` creates a regular file and
requires its parent directory to exist. If the final target is a regular file,
the operation replaces its complete content while retaining its inode and
existing mode. If it resolves to a directory, the operation fails with
`EISDIR`. `{ exclusive: true }` MUST fail with `EEXIST` when any final
directory entry, including a symlink, exists.

`mode` applies only when a new inode is created. Supplying `mode` while
replacing an existing file MUST NOT change that file's mode; callers use
`chmod` for that purpose.

For byte-array and string inputs, content, metadata, and namespace updates
MUST become visible in one transaction. For stream input, the implementation
MAY stage immutable content objects as chunks arrive, but MUST NOT change the
visible namespace until the source ends successfully and one final
transaction commits. A stream error, abort, limit violation, or final
transaction failure MUST leave the previously visible path unchanged.

A streamed write that stages before its final transaction MUST create a
durable staging lease before its first staged allocation. Its identifier and
owner nonce MUST bind a write-session identifier and target baseline. Every
staged object and manifest MUST be attached to that lease in the transaction
that creates it. Garbage collection MUST treat an unexpired preparing or
active staging lease and all attached content as roots. The implementation
MUST renew the lease using `stagingLeaseMs` while preparation continues.

The final namespace transaction MUST verify the lease, expiry, manifest, and
objects, commit the visible file, and release the staging lease atomically. On
source failure, abort, or rejected final transaction, the implementation MUST
attempt an idempotent lease release. A process crash may leave the lease until
expiry. An expired streamed write MUST re-stage from retained input or fail
without changing namespace state. Content left after release or expiry is
unreachable staging and MAY be reclaimed by garbage collection.

### Range writes

`writeRange(path, offset, content)` MUST require an existing regular file and
MUST NOT create a missing file. It writes all input bytes beginning at
`offset`. It MUST preserve bytes outside the written range. If `offset` is
beyond end of file, the implementation MUST extend the file and fill the gap
with zero bytes. A zero-length input MUST still resolve and type-check the
path, but MUST NOT alter bytes or timestamps.

The write and any extension MUST be one atomic mutation. The implementation
MUST copy or consume the input before resolving the returned promise so later
caller mutation cannot affect stored data.

### Range replacement

`replaceRange(path, offset, deleteLength, insertBytes)` MUST require an
existing regular file. `offset` and `deleteLength` MUST be non-negative safe
integers. `offset` MUST be at most the current file size, and `deleteLength`
MUST be at most `size - offset`. An out-of-bounds range MUST fail with
`EINVAL`; unlike `writeRange`, this operation never implicitly zero-fills a
gap.

The operation MUST atomically remove `deleteLength` bytes beginning at
`offset`, then insert `insertBytes` at that same offset. It MUST preserve the
prefix before `offset` and the suffix after the removed range. The final size
MUST be computed with checked arithmetic and MUST NOT exceed the persisted
`maxFileBytes` limit.

The implementation MUST capture the input bytes before the returned promise
resolves. If both `deleteLength` and `insertBytes.byteLength` are zero, the
operation MUST still resolve and type-check the path, but MUST NOT change
content or timestamps. Any other successful replacement updates file
timestamps according to the content-write rules, even when the final bytes
happen to equal the prior bytes.

Insertion is `replaceRange(path, offset, 0, bytes)`. Deletion is
`replaceRange(path, offset, deleteLength, new Uint8Array())`. Internal
copy-on-write pages, ordered patches, and content-defined rechunking MUST be
observationally equivalent to this byte-array definition.

### Truncation

`truncate(path, size)` MUST require an existing regular file. `size` defaults
to `0`. Shrinking removes the suffix beginning at `size`. Growing appends zero
bytes. Truncating to the current size is a successful no-op and MUST NOT
change timestamps. All other truncations are one atomic mutation.

Sparse storage is an implementation detail. Reads MUST observe zero bytes in
every grown region regardless of whether physical zero-filled objects were
allocated.

### Directory creation and listing

`mkdir` creates a directory with the requested or default mode. Without
`recursive`, the parent MUST exist and the destination MUST NOT exist. With
`recursive`, missing ancestor directories MUST be created using the same mode,
and an existing destination directory is a successful no-op. An existing
non-directory at any required component MUST fail with `ENOTDIR` for an
intermediate component or `EEXIST` for the destination.

Recursive creation MUST be atomic: either every required directory is visible
or none is. `mkdir` MUST follow symbolic links in existing intermediate
components.

`readdir` MUST return direct children only and MUST follow a final symbolic
link to a directory. Each entry describes the directory entry itself; in
particular, a child symbolic link has type `symlink` even when its target is a
file or directory. Results MUST use the ordering specified above.

`limit`, when present, MUST be a non-negative safe integer. A zero limit
returns an empty list after resolving and type-checking the directory.
`startAfter`, when present, is a single raw entry name, not a path; it MUST
contain neither `/` nor NUL and MUST NOT be `.` or `..`. The result excludes
names less than or equal to `startAfter` in the required byte order. `limit`
is applied after `startAfter` and MUST NOT exceed `maxReaddirEntries`.

When `limit` is omitted, `readdir` MUST return the complete selected result if
its count is at most `maxReaddirEntries`. It MUST reject with `EFBIG`, without
returning a partial list, when more entries remain. Callers page a larger
directory by supplying a bounded `limit` and the previous page's last name as
`startAfter`.

This cursor is not a snapshot: concurrent directory mutation between calls
MAY cause entries to be skipped or observed according to their current names.
Callers needing a snapshot MUST perform the walk inside a branch or other
stable view.

### Stat and chmod

`stat` follows a final symbolic link. `lstat` does not. Both MUST return a
snapshot value; later mutations MUST NOT modify a returned object.
`name` MUST be the last segment of the selected canonical path, even when
`stat` follows that entry to a target with a different name.

Exactly one of the three type predicates on `FileStat` MUST return true, and
it MUST agree with `type`. `id` MUST be opaque to callers. Hard links MUST
return equal IDs; an inode deleted after its last link MUST NOT have its ID
reused during the lifetime of the database.

`chmod` follows a final symbolic link, applies `mode & 0o7777`, and uses the
timestamp rules above. Applying the already stored mode is a successful no-op
and MUST NOT change timestamps.

### Symbolic links

`symlink(target, path)` MUST reject an empty target or a target containing NUL
or ill-formed Unicode with `EINVAL`. It MUST allow dangling targets. The
destination parent must exist, and the destination must not. The target MUST
be stored byte-for-byte as its JavaScript string representation.

`readlink(path)` MUST return that stored target without normalization. It MUST
not follow the final link. A non-link path MUST fail with `EINVAL`.

### Rename

`rename(oldPath, newPath)` MUST atomically move exactly the source directory
entry. It MUST preserve inode identity and MUST NOT copy or rewrite regular
file content. The source must exist and the destination parent must exist.
Equal canonical paths make the operation a successful no-op.

If a destination exists:

- a non-directory source MAY atomically replace a non-directory destination;
- a directory source MAY replace an empty destination directory;
- replacing a directory with a non-directory MUST fail with `EISDIR`;
- replacing a non-directory with a directory MUST fail with `ENOTDIR`; and
- replacing a non-empty directory MUST fail with `ENOTEMPTY`.

A directory MUST NOT be moved into itself or any resolved descendant; this
must be checked after following destination-parent symbolic links and fail
with `EINVAL`. Renaming one hard-link name over another name for the same
inode MUST remove the source name and leave the destination name linked to the
inode.

### Unlink and recursive removal

`unlink(path)` MUST remove one regular-file or symbolic-link directory entry.
It MUST NOT follow a final symbolic link. It MUST fail with `EISDIR` for a
directory and `ENOENT` for a missing path.

`rm(path)` removes one file, symbolic link, or empty directory. A non-empty
directory without `{ recursive: true }` MUST fail with `ENOTEMPTY`.
`{ recursive: true }` MUST remove the selected directory and all descendants
as one logical operation. A selected symbolic link MUST itself be removed,
not traversed, even when it points to a directory. `{ force: true }` suppresses
only `ENOENT` for the selected path; it MUST NOT suppress invalid paths,
permission errors, corruption, or adapter failures.

Recursive removal SHOULD use bounded internal batches for storage work, but
partial namespace removal MUST never become visible. If an implementation
cannot atomically remove a tree within the configured mutation limits, it MUST
fail with `EFBIG` before changing the namespace.

## Atomicity and concurrent operations

Each path-based mutation in this document is one atomic database transaction.
A successful promise resolution means the mutation is committed. A rejected
promise means no namespace, metadata, or referenced-content change from that
operation is visible. Staged but unreachable immutable objects are permitted
only where explicitly stated and MUST be safe for garbage collection.

Each materializing read and metadata read MUST observe one committed snapshot.
A read MUST see either the state before or after a concurrent transaction, not
a mixture. Stream snapshot behavior is defined separately above.

The library MUST be safe when asynchronous calls overlap on one filesystem
instance and when several instances use the same database. It MUST rely on
SQLite transaction serialization and MUST NOT use process-local state as the
only concurrency guard. When two ordinary writes target the same path without
branch publication, transactions commit in some serial order and the later
commit wins. Same-path conflict detection for private branches is defined in
the branch specification, not by the main-view file methods.

An implementation MAY retry transient SQLite busy failures. Retries MUST be
bounded, MUST rerun the entire transaction, and MUST NOT retry an input,
integrity, or filesystem-semantic error. If contention remains after the
configured policy, the operation MUST fail with `EBUSY`.

Version 0.1 does not expose persistent file handles. There is therefore no
seek pointer, shared open-file description, open-unlinked inode contract, or
flush method. `readStream` is a snapshot stream, not a file handle. A future
handle API MUST be added without changing these path-operation semantics.

## Capabilities, maintenance, and observation

The root filesystem MUST expose immutable effective capabilities and a
portable maintenance surface. These APIs MUST contain no Node.js, Durable
Object, Computer, FUSE, or RPC types.

```ts
export type LimitDomain = "filesystem" | "storage" | "branch" | "runtime";
export type LimitScope = "persisted" | "runtime";

export interface EffectiveLimit {
  readonly domain: LimitDomain;
  readonly name: string;
  readonly value: number;
  readonly scope: LimitScope;
  readonly constrainedBy: "configuration" | "format" | "adapter";
}

export interface FilesystemCapabilities {
  readonly adapter: DatabaseAdapterCapabilities;
  readonly filesystem: Readonly<FilesystemLimits>;
  readonly storage: Readonly<StorageLimits>;
  readonly branch: Readonly<BranchConfiguration>;
  readonly runtime: Readonly<RuntimeLimits>;
  readonly format: Readonly<StorageFormat>;
  readonly effectiveLimits: readonly EffectiveLimit[];
  readonly readOnly: boolean;
}

export interface GarbageCollectionOptions {
  readonly runId?: string;
  readonly maxBatches?: number;
  readonly signal?: AbortSignal;
}

export interface GarbageCollectionResult {
  readonly runId: string;
  readonly state: "complete" | "paused" | "abandoned";
  readonly examinedManifestCount: number;
  readonly deletedManifestCount: number;
  readonly examinedObjectCount: number;
  readonly deletedObjectCount: number;
  readonly reclaimedObjectPayloadBytes: number;
  readonly reclaimedManifestPayloadBytes: number;
  readonly reclaimedBranchOverlayPayloadBytes: number;
  readonly committedBatches: number;
  readonly elapsedMs: number;
}

export interface PhysicalStorageSnapshot {
  readonly mainFileBytes?: number;
  readonly walBytes?: number;
  readonly freelistBytes?: number;
}

export interface StorageSnapshot {
  readonly rootMutationGeneration: number;
  readonly mainLogicalBytes: number;
  readonly storedObjectPayloadBytes: number;
  readonly storedManifestPayloadBytes: number;
  readonly reachableObjectPayloadBytes: number;
  readonly reachableManifestPayloadBytes: number;
  readonly reclaimablePayloadBytes: number;
  readonly branchPageBytes: number;
  readonly branchPatchBytes: number;
  readonly branchExclusiveObjectBytes: number;
  readonly branchExclusiveManifestBytes: number;
  readonly branchExclusivePayloadBytes: number;
  readonly objectCount: number;
  readonly manifestCount: number;
  readonly revisionCount: number;
  readonly includesNamespaceMetadata: boolean;
  readonly includesOperationResults: boolean;
  readonly physical?: PhysicalStorageSnapshot;
}

export type VerificationScope =
  | "metadata"
  | "namespace"
  | "manifests"
  | "objects"
  | "head";

export interface VerificationOptions {
  readonly scopes?: readonly VerificationScope[];
  readonly cursor?: string;
  readonly maxEntities?: number;
  readonly signal?: AbortSignal;
}

export interface VerificationResult {
  readonly rootMutationGeneration: number;
  readonly checkedEntities: number;
  readonly complete: boolean;
  readonly nextCursor: string | null;
}

export interface FilesystemMaintenance {
  collectGarbage(
    options?: GarbageCollectionOptions,
  ): Promise<GarbageCollectionResult>;
  snapshotStorage(): Promise<StorageSnapshot>;
  verify(options?: VerificationOptions): Promise<VerificationResult>;
}

export interface FilesystemObservation {
  readonly type: "operation" | "integrity" | "maintenance";
  readonly operation: string;
  readonly outcome: "success" | "error";
  readonly elapsedMs: number;
  readonly counters: Readonly<Record<string, number>>;
  readonly errorCode?: FilesystemErrorCode;
}

export type FilesystemObserver = (event: FilesystemObservation) => void;
```

`capabilities` MUST report the adapter maxima and every effective filesystem,
storage, and branch value used by the opened instance. `effectiveLimits` MUST
identify whether each value is persisted for the database or may vary at
runtime, and whether configuration, binary format, or adapter capacity is the
tightest bound. The report MUST NOT claim a value larger than the adapter can
execute safely.

`collectGarbage` MUST implement the bounded mark-and-sweep behavior and result
counters defined by the storage specification. `maxBatches` defaults to `1`
and MUST be a positive safe integer. A paused run MUST return a reusable
`runId`; a later call MAY resume it. A read-only filesystem MUST reject
collection with `EROFS`.

`snapshotStorage` MUST compute all logical and payload counters in one read
transaction. Physical counters are optional and MUST be clearly separated
from payload counters. Hard-linked file bytes count once in logical set-based
metrics. The two inclusion booleans MUST state the accounting boundary.

One `verify` call MUST examine at most `maxEntities`, which defaults to
`maxQueryBatchSize` and MUST NOT exceed it. `nextCursor` is opaque and bound to
the returned root mutation generation. Resuming after that generation changes
MUST fail with `EBUSY`; it MUST NOT mix verification snapshots. Verification
MUST be read-only, MUST NOT repair data, and MUST reject the first verified
integrity failure with `ECORRUPT`. A complete result has `nextCursor: null`.

The observer is optional and synchronous. The core SHOULD emit operation
counters required by the storage specification and an integrity observation
before surfacing verified corruption. Observer code MUST NOT run inside an
authoritative transaction. An observer throw MUST be caught and MUST NOT alter
an operation's result, rollback, retry, or error. Events MUST NOT include file
bytes, inserted bytes, secrets, or paths not already supplied by the caller.

Maintenance calls and capability access after root close begins MUST follow
the `EBADF` rules below. A branch handle MUST NOT expose `FilesystemMaintenance`.

## Errors

All filesystem failures MUST reject with `FilesystemError` except cancellation,
which uses a platform `AbortError`, and programmer type violations that the
TypeScript type system cannot express, which MAY use `TypeError`.

```ts
export type FilesystemErrorCode =
  | "EINVAL"
  | "ENOENT"
  | "ENOTDIR"
  | "EISDIR"
  | "EEXIST"
  | "ENOTEMPTY"
  | "ELOOP"
  | "EPERM"
  | "EROFS"
  | "EBADF"
  | "EAGAIN"
  | "EBUSY"
  | "EFBIG"
  | "ENOSPC"
  | "ECORRUPT"
  | "ESCHEMA"
  | "EIO";

export class FilesystemError extends Error {
  readonly name: "FilesystemError";
  readonly code: FilesystemErrorCode;
  readonly syscall?: string;
  readonly path?: string;
  readonly destination?: string;
  readonly cause?: unknown;
}
```

Codes have these meanings:

| Code | Meaning |
| --- | --- |
| `EINVAL` | Invalid path, number, option, mode, target, or operation. |
| `ENOENT` | A selected path, target, or required parent does not exist. |
| `ENOTDIR` | A directory operand or path component is not a directory. |
| `EISDIR` | A file-only operation selected a directory. |
| `EEXIST` | A creation destination already exists. |
| `ENOTEMPTY` | A directory must be empty for the requested operation. |
| `ELOOP` | Symbolic-link resolution exceeded 40 traversals. |
| `EPERM` | The operation is structurally forbidden. |
| `EROFS` | The database or selected view is read-only. |
| `EBADF` | The filesystem or adapter is closed. |
| `EAGAIN` | Bounded synchronous backpressure could not admit work yet. |
| `EBUSY` | Bounded SQLite contention retries were exhausted. |
| `EFBIG` | A configured content or operation limit was exceeded. |
| `ENOSPC` | SQLite reports that storage capacity is exhausted. |
| `ECORRUPT` | Persisted data or the SQLite database is corrupt. |
| `ESCHEMA` | The schema or format is unsupported or cannot migrate safely. |
| `EIO` | An unexpected storage, hashing, or content failure. |

An error involving one path SHOULD set `path`. `rename` and `link` errors MAY
also set `destination`. `syscall` SHOULD contain the public operation name.
Adapters MUST preserve the original error as `cause` when wrapping it is safe.
Error messages are diagnostic and are not stable API; callers MUST branch on
`code`.

A missing intermediate component MUST produce `ENOENT`; an existing
non-directory intermediate component MUST produce `ENOTDIR`. Implementations
MUST apply this rule consistently across operations rather than inheriting a
driver-specific lookup result.

### Storage error mapping

Adapters MUST expose distinguishable read-only, busy, capacity, statement-
limit, corruption, closed, constraint, and general I/O categories. The core
MUST map them as follows:

| Storage condition | Public code |
| --- | --- |
| Read-only database or transaction | `EROFS` |
| Busy or locked after bounded retries | `EBUSY` |
| Database full or quota exhausted | `ENOSPC` |
| BLOB, binding, value, or configured size limit | `EFBIG` |
| Closed adapter or connection | `EBADF` |
| SQLite or database-page corruption | `ECORRUPT` |
| Verified filesystem or content invariant failure | `ECORRUPT` |
| Unsupported schema or persisted format version | `ESCHEMA` |
| Unexpected storage or operating-system I/O failure | `EIO` |

A uniqueness or foreign-key constraint used to enforce a known filesystem
precondition MUST be re-read and mapped to its semantic code, such as
`EEXIST`. A constraint that demonstrates invalid persisted state MUST map to
`ECORRUPT`. Any other unexpected constraint failure MUST map to `EIO`.

`StorageIntegrityError` and `DatabaseCorruptionError` from the storage
specification MUST surface publicly as `FilesystemError` with `ECORRUPT` and
the storage error as `cause`. Unsupported schema, manifest, hash, or chunker
versions MUST map to `ESCHEMA`; they MUST NOT appear as `ENOENT` or `EIO`.

### Validation precedence

When several failures are simultaneously observable, implementations MUST use
this order:

1. A closed or closing filesystem or branch handle fails with `EBADF`.
2. An already aborted signal fails with `AbortError`.
3. Branch existence and lifecycle checks use the branch error contract.
4. Argument, option, number, and path validation fails before database access.
5. Namespace and file-type checks produce their semantic filesystem error.
6. Adapter and persisted-storage failures use the mapping above.

An operation admitted before close begins may finish under its original
validation. Close cancels active streams as defined below. The core MUST NOT
replace a verified corruption or unsupported-format error with a path-missing
error.

## Database adapter contract

### Values and statements

The core uses this structural interface:

```ts
export type SqliteValue = null | string | number | Uint8Array;
export type SqliteBindings = readonly SqliteValue[];
export type SqliteRow = Readonly<Record<string, SqliteValue>>;

export interface SqliteRunResult {
  readonly changes: number;
  readonly lastInsertRowid?: number;
}

export interface QueryBudget {
  readonly maxRows: number;
  readonly maxBytes: number;
}

export interface FilesystemSqlExecutor {
  run(sql: string, bindings?: SqliteBindings): SqliteRunResult;
  all<Row extends SqliteRow = SqliteRow>(
    sql: string,
    bindings: SqliteBindings,
    budget: QueryBudget,
  ): readonly Row[];
}

export type TransactionMode = "read" | "write" | "exclusive";

export interface DatabaseAdapterCapabilities {
  /** Largest BLOB the adapter can bind and return exactly. */
  readonly maxBlobBytes: number;
  /** Largest number of positional bindings in one statement. */
  readonly maxBindings: number;
  readonly durability: "acknowledged" | "relaxed-test";
  readonly journalMode: "wal" | "rollback" | "runtime-managed";
  readonly memoryPolicy: "configured" | "runtime-managed";
  readonly cacheTargetBytes?: number;
  readonly mmapLimitBytes?: number;
}

export interface FilesystemDatabaseAdapter extends FilesystemSqlExecutor {
  readonly kind: "sqlite";
  readonly readOnly: boolean;
  readonly capabilities: DatabaseAdapterCapabilities;
  transaction<T>(
    mode: TransactionMode,
    callback: (tx: FilesystemSqlExecutor) => T,
  ): T;
  close(): void | Promise<void>;
}
```

Bindings MUST use positional `?` parameters. The core MUST bind application
values and MUST NOT interpolate path, content, identifier, or metadata values
into SQL text. Adapters MUST return BLOB values as detached `Uint8Array`
instances, SQLite NULL as `null`, TEXT as `string`, and safe INTEGER or REAL
values as `number`. Persisted integers used by the core MUST remain in the
safe-integer range.

`all` MUST return rows in statement result order. Returned row objects MUST
remain usable after the next statement. `run().changes` MUST describe the
immediately executed statement, not the connection-wide cumulative count.

Every multi-row statement MUST contain a row bound derived from
`QueryBudget.maxRows`. The adapter MUST decode incrementally and stop before
retained row capacity exceeds `QueryBudget.maxBytes`. A driver API that
materializes the complete result before enforcing both bounds MUST NOT
implement `all` directly; its adapter must use a bounded cursor or visitor.

Adapter capability values MUST be positive safe integers. `maxBlobBytes` is
the greatest BLOB byte length that can be bound and returned without loss.
`maxBindings` is the greatest positional parameter count accepted by one
statement. The core MUST batch below both values and MUST validate capabilities
before initialization, allocation, hashing, or migration.

The adapter MUST provide SQLite with foreign-key enforcement. It MUST support
transactions, savepoints, common table expressions, `ON CONFLICT`, and
`RETURNING`. The adapter documentation MUST state its minimum SQLite version.
The core SHOULD avoid optional extensions and MUST NOT require a network
connection.

### Transaction requirements

`transaction(mode, callback)` MUST:

- invoke its callback synchronously;
- reject a callback result that is a promise;
- commit all callback statements before returning its value;
- roll back all callback statements when the callback throws;
- rethrow the original error after rollback;
- prevent statements from another operation from interleaving on the same
  connection; and
- support nested calls with savepoints or document that the core owns nesting
  and never passes a nested call to the adapter.

A `"read"` transaction MUST expose one consistent snapshot and MUST reject a
write statement. A `"write"` transaction MUST permit reads and writes and
serialize conflicting writers. An `"exclusive"` transaction MUST additionally
exclude another initializer, migrator, or transaction that could observe a
partially changed schema. A read-only adapter MUST support `"read"` and MUST
reject `"write"` and `"exclusive"` with its read-only error category.

The core MUST NOT perform network I/O, stream reads, or other asynchronous work
inside the callback. It MUST prepare asynchronous input and hashes before
opening the final transaction.

If a process or isolate terminates during a transaction, SQLite recovery MUST
leave either the pre-transaction or committed state after reopen. An adapter
MUST NOT emulate transactions with a sequence of independently committed
statements.

### Adapter ownership

An adapter represents one logical SQLite database. It MUST serialize use of a
single underlying connection. `close()` is mandatory and MUST be idempotent.
It MAY be a no-op for runtime-owned storage, but it MUST release adapter-local
resources. An adapter MAY expose additional driver-specific configuration
outside the core interface.

`ownsDatabase` defaults to `false` when a caller passes an adapter to
`EphemeralFS.open`. With `ownsDatabase: false`, closing the filesystem MUST
not close the adapter. With `ownsDatabase: true`, closing the filesystem MUST
call `adapter.close()` after filesystem operations and snapshot streams have
released their resources.

An adapter MUST invoke its underlying close action at most once. Every adapter
`close()` call MUST return the same settled outcome. If the first action fails,
later calls MUST report that same failure and MUST NOT retry the underlying
close action implicitly.

## Required adapters

### Node.js SQLite

`@ephemeralai/fs-sqlite-node` MUST provide a factory with this conceptual
shape:

```ts
export interface OpenNodeSqliteOptions {
  readonly filename: string;
  readonly readOnly?: boolean;
  readonly create?: boolean;
  readonly busyTimeoutMs?: number;
  readonly durability?: "acknowledged" | "relaxed-test";
  readonly cacheTargetBytes?: number;
  readonly mmapLimitBytes?: number;
}

export declare function openNodeSqlite(
  options: OpenNodeSqliteOptions,
): Promise<FilesystemDatabaseAdapter>;
```

The package MAY select the concrete Node.js SQLite driver until its first
stable release. It MUST document that driver and supported Node.js versions.
It MUST enable foreign keys, configure a bounded busy timeout, normalize BLOBs
to detached `Uint8Array` values, and implement the transaction guarantees
above. It MUST report tested driver limits and its durability profile through
`capabilities`.

`durability` defaults to `"acknowledged"`. For a file-backed writable
database, that profile MUST use WAL or an equivalently crash-safe rollback
journal, production-safe synchronous commit, foreign keys, a bounded busy
timeout, and a bounded checkpoint or journal-size policy. Returning from a
write transaction means SQLite acknowledged that profile. A
`"relaxed-test"` profile requires explicit opt-in and MUST NOT be used by the
Computer production factory.

`cacheTargetBytes` defaults to 16 MiB and `mmapLimitBytes` defaults to zero.
The adapter MUST apply finite SQLite page-cache and memory-map settings before
opening the filesystem, report the effective values through capabilities, and
use a file-backed temporary-store policy for storage-scale sort or staging
work. These adapter-managed allocations are measured separately from the
core's exact managed-memory counter. Computer MUST include them in its
process-wide memory configuration and resident-memory measurements.

The Node adapter owns a database it opens and MUST close its connection when
its `close()` resolves. `filename: ":memory:"` MUST be supported for tests.

### Durable Object SQLite

`@ephemeralai/fs-sqlite-cloudflare` MUST adapt Durable Object SQLite storage
to `FilesystemDatabaseAdapter`. Cloudflare-specific types MAY appear in this
adapter package but MUST NOT leak through `@ephemeralai/fs` declarations.

The adapter MUST use the Durable Object's transactional SQLite facility; it
MUST NOT emulate rollback in memory. It MUST normalize Cloudflare BLOB results
such as `ArrayBuffer` to detached `Uint8Array` values and normalize cursor rows
to ordinary JavaScript objects. It MUST preserve statement order and
transaction serialization within the object. It MUST report conservative
tested Durable Object BLOB and binding limits through `capabilities`.
It MUST report `"acknowledged"` durability and `"runtime-managed"` journal
mode when the runtime owns those policies. It MUST also report
`"runtime-managed"` memory policy. The portable core MUST still use bounded
queries and buffers; the runtime-owned cache is not permission to mirror
SQLite state in JavaScript memory.

Closing a Durable Object adapter MUST release adapter-local resources but MUST
NOT close, delete, or invalidate runtime-owned Durable Object storage. The
underlying-storage portion of `close()` MAY be a no-op. The adapter SHOULD be
passed with `ownsDatabase: false` when another owner will continue to use it.

The adapter's integration conformance tests MUST run in a supported Workers
runtime or faithful local Workers runtime, not only against a mock that wraps
the Node adapter.

## Open, migration, and close lifecycle

`EphemeralFS.open` MUST complete these steps before returning:

1. Validate options and adapter capabilities.
2. Use a `"read"` transaction to inspect a current existing schema and verify
   its persisted configuration and root invariants.
3. Reject a newer or otherwise unsupported version with `ESCHEMA` without
   writing.
4. If initialization or migration is required, require a writable adapter and
   perform it in an `"exclusive"` transaction.
5. Re-read the completed schema and configuration in one `"read"` transaction.
6. Return a usable filesystem only after every required transaction commits.

Opening a read-only database that requires initialization or migration MUST
fail with `EROFS`. A failed open MUST NOT return a partial instance and MUST
close the adapter only when ownership was requested. Multiple instances MAY
open the same current database; schema migration MUST be serialized so only
one instance applies a version step.

A current read-only database MUST open using read snapshots only. Opening it
MUST NOT request an `"exclusive"` transaction, change pragmas stored in the
database, update a last-opened field, renew leases, or perform maintenance.

Configured storage-format limits or algorithms that affect persisted data
MUST be recorded in the database. A later open with incompatible requested
values MUST fail with `ESCHEMA` rather than silently reinterpret data.
Runtime-only limits such as `maxMaterializedBytes` need not be persisted.

`close()` MUST be idempotent. It MUST stop accepting new operations, error
active snapshot streams with `EBADF`, wait for already admitted non-stream
operations and stream resource release, and then close an owned adapter.
Operations started after close begins MUST fail with `EBADF`. A close failure
MUST still leave the filesystem handle permanently closed. Every later
`close()` call MUST return the same settled outcome and MUST NOT invoke adapter
close again. Implementations SHOULD support `Symbol.asyncDispose` when the
target JavaScript runtime supports it.

## Search and watch scope

Search and watch are explicitly deferred from the stable version 0.1 core
surface.

- The core MUST NOT expose `grep`, `find`, `glob`, or full-text indexing as a
  required filesystem primitive. A portable helper package MAY implement
  search with `readdir`, `stat`, `readRange`, and `readStream`.
- The core MUST NOT promise OS-style `watch` semantics. SQLite polling,
  Durable Object notifications, Computer RPC events, and host filesystem
  watchers have different delivery and durability guarantees.
- A future watch contract must specify cursor persistence, event coalescing,
  rename representation, overflow, recursive scope, branch visibility, and
  recovery before it is added here.

Experimental search or watch exports MUST be labeled unstable, MUST be absent
from the version 0.1 conformance requirements, and MUST NOT introduce host
types into the core package.

## Limits

Limits belong to three separate configuration domains. Namespace and
materialization limits use `FilesystemLimits`. Content representation,
maintenance, and lease limits use `StorageLimits`, cross-defined with the
storage specification. Branch limits and retention use `BranchConfiguration`,
defined by the branches and publication specification.

```ts
export interface FilesystemLimits {
  readonly maxPathBytes: number;
  readonly maxNameBytes: number;
  readonly maxSymlinkTargetBytes: number;
  readonly maxSymlinkTraversals: number;
  readonly maxMaterializedBytes: number;
  readonly preferredStreamChunkBytes: number;
  readonly maxAtomicTreeEntries: number;
  readonly maxReaddirEntries: number;
}

export interface StorageLimits {
  readonly maxManifestEntries: number;
  readonly maxManifestBytes: number;
  readonly maxFileBytes: number;
  readonly maxWriteBytes: number;
  readonly maxManagedPayloadBytes: number;
  readonly maxStagingPayloadBytes: number;
  readonly maxBranchOverlayBytes: number;
  readonly maxMaintenanceBytes: number;
  readonly maintenanceReserveBytes: number;
  readonly maxPermanentIdentifiers: number;
  readonly maxFinalTransactionRows: number;
  readonly maxFinalTransactionBytes: number;
  readonly maxRevisionReplaySteps: number;
  readonly maxPatchesPerFile: number;
  readonly maxPatchBytesPerFile: number;
  readonly maxQueryBatchSize: number;
  readonly maxGcBatchSize: number;
  readonly maxRetainedRevisions: number;
  readonly readLeaseMs: number;
  readonly stagingLeaseMs: number;
}

export interface RuntimeLimits {
  readonly maxManagedResidentBytes: number;
  readonly maxCacheBytes: number;
  readonly maxPendingWriteBytes: number;
  readonly maxWriteSessionBytes: number;
  readonly maxPrefetchBytes: number;
  readonly maxQueryBatchBytes: number;
  readonly maxPreparedResultBytes: number;
  readonly maxConcurrentStreams: number;
  readonly maxConcurrentOperations: number;
  readonly maxOpenBranchHandles: number;
  readonly maxOpenNodeVfsSessions: number;
}

export type CowPageBytes = 4096 | 8192 | 16384;

export interface StorageFormatOptions {
  readonly cowPageBytes?: CowPageBytes;
}

export interface StorageFormat {
  readonly cowPageBytes: CowPageBytes;
  readonly hashAlgorithm: "sha256";
  readonly chunkerAlgorithm: "fastcdc-v1";
  readonly manifestFormat: "efs-manifest-v1";
}
```

Version 0.1 filesystem defaults are 4,096 path bytes, 255 name bytes, 4,096
symlink-target bytes, 40 symlink traversals, 64 MiB of materialized result,
256 KiB preferred stream chunks, 100,000 atomic tree entries, and 10,000
directory entries per materialized listing. Storage defaults and valid ranges
are normative in the storage specification. Branch defaults are normative in
the branches and publication specification.

Version 0.1 runtime defaults are 128 MiB `maxManagedResidentBytes`, 64 MiB
`maxCacheBytes`, 64 MiB `maxPendingWriteBytes`, 16 MiB
`maxWriteSessionBytes`, 1 MiB `maxPrefetchBytes`, 64 MiB
`maxPreparedResultBytes`, 2 MiB `maxQueryBatchBytes`, and 64 concurrent
streams. It also permits 256 admitted operations, 1,024 open branch handles,
and 256 open Node VFS sessions. Sub-limits do not add to the aggregate
allowance: all implementation-owned live bytes participate in
`maxManagedResidentBytes`.

Runtime accounting includes content and manifest caches, decoded manifests,
prefetch, rechunking windows, pending write copies, prepared result arrays,
and queued but not emitted stream bytes. Caller-owned input before admission,
already emitted output, JavaScript runtime overhead, and SQLite's internal page
cache are outside that exact counter and MUST be measured separately when the
runtime exposes them.

Before allocating accounted bytes, the implementation MUST reserve them under
the aggregate and applicable sub-limit. On pressure it MUST evict derived
cache entries, bypass cache admission, flush bounded staged writes, or apply
backpressure. It MUST NOT multiply a per-handle allowance across concurrent
handles beyond the aggregate limit. Cancellation, close, failure, and retry
exhaustion MUST release every reservation.

`FilesystemLimits.maxMaterializedBytes` MUST NOT exceed
`RuntimeLimits.maxPreparedResultBytes`. Returned byte arrays, directory
collections, changed-path arrays, conflict arrays, and other materialized
results count as prepared results until ownership transfers to the caller.
Each admitted operation and branch handle MUST consume its count slot and
reserve bounded control state. Rejection, completion, cancellation, handle
close, and filesystem close MUST release the corresponding slot.

The format defaults to 8,192-byte copy-on-write pages. Creation may select
4,096 or 16,384 instead. The effective value is persisted and exposed through
`capabilities.format`. Supplying a conflicting value when reopening an
existing filesystem MUST fail with `ESCHEMA`.

Path, name, and symlink limits are measured after UTF-8 encoding. The complete
canonical path includes `/` separators. A value equal to a limit is allowed.
Exceeding a path, name, or symlink-target limit MUST fail with `EINVAL`.
Exceeding a file, materialization, or atomic-tree limit MUST fail with
`EFBIG`. Physical database exhaustion MUST fail with `ENOSPC`.

`maxFileBytes` MUST NOT default to `Number.MAX_SAFE_INTEGER`. For a new
database, the core MUST derive a finite upper bound from all of:

- JavaScript's safe-integer offset bound;
- the compact manifest's 32-byte header and 36-byte entries;
- the adapter's `maxBlobBytes` for encoded manifests and objects;
- the configured `maxManifestEntries`; and
- the persisted maximum chunk size.

For compact manifest version 1, the exact derivation is:

```text
blobEntryCapacity = floor((adapter.capabilities.maxBlobBytes - 32) / 36)
maxManifestEntries = min(configuredEntryCap, 2^32 - 1,
                         blobEntryCapacity)
formatFileCapacity = maxManifestEntries * (fastCdcMinimum + 1)
maxFileBytes = min(configuredFileCap, Number.MAX_SAFE_INTEGER,
                   formatFileCapacity)
```

All arithmetic MUST be checked. The `fastCdcMinimum + 1` span is the safe
worst case for the exact version 1 scan index. It guarantees that every
canonical manifest for an accepted file fits in one adapter BLOB. Object and
chunk BLOBs MUST separately fit `maxBlobBytes` and their unsigned 32-bit size
fields.

The selected effective `maxFileBytes` MUST be persisted when the database is
created. Reopening with a smaller adapter capacity or an incompatible requested
value MUST fail with `ESCHEMA`; it MUST NOT silently reinterpret or truncate
files. A caller MAY request a smaller valid value at creation.

Path, name, symlink, file, manifest-entry, patch, atomic-tree, write, and
retention limits affect accepted persisted state and MUST be consistent for
every writer. Materialization, preferred stream chunk, directory result,
query-batch, garbage-collection batch, and lease TTL values MAY vary by
instance where the stored row records the required expiry or bound.

Implementations MUST validate arithmetic for overflow before allocating or
changing state. They MUST bound SQL result materialization and destructive
tree walks. An adapter MAY impose stricter environmental limits only when it
documents them and maps limit failures to the codes above.

## Shared conformance suite

`@ephemeralai/fs-testkit` MUST export a shared suite that accepts an adapter
factory rather than importing a concrete driver:

```ts
export type ConformanceCapability =
  | "read-only-reopen"
  | "second-connection"
  | "schema-fixtures"
  | "fault-injection"
  | "garbage-collection"
  | "physical-reopen"
  | "crash-recovery"
  | "ownership";

export interface ConformanceReopenOptions {
  readonly readOnly?: boolean;
  readonly physical?: boolean;
}

export interface ConformanceFaultController {
  arm(point: string, occurrence?: number): void;
  clear(): void;
}

export interface ConformanceOwnershipProbe {
  readonly adapter: FilesystemDatabaseAdapter;
  closeCallCount(): number;
}

export interface ConformanceDatabase {
  readonly adapter: FilesystemDatabaseAdapter;
  readonly capabilities: readonly ConformanceCapability[];
  readonly faults?: ConformanceFaultController;
  reopen(
    options?: ConformanceReopenOptions,
  ): Promise<FilesystemDatabaseAdapter>;
  openSecondConnection?(): Promise<FilesystemDatabaseAdapter>;
  reopenFromFixture?(fixtureName: string): Promise<FilesystemDatabaseAdapter>;
  collectGarbage?(
    filesystem: EphemeralFS,
    options?: GarbageCollectionOptions,
  ): Promise<GarbageCollectionResult>;
  crashAndReopen?(): Promise<FilesystemDatabaseAdapter>;
  createOwnershipProbe?(): Promise<ConformanceOwnershipProbe>;
  dispose(): Promise<void>;
}

export interface ConformanceAdapterFactory {
  readonly name: string;
  create(): Promise<ConformanceDatabase>;
}

export declare function filesystemConformance(
  factory: ConformanceAdapterFactory,
): void;
```

Each optional hook MUST be present exactly when its matching capability tag is
present. The suite MUST report a capability-based skip; it MUST NOT silently
pass a case whose hook is absent. `reopen({ physical: true })` must close and
recreate the physical driver connection. `crashAndReopen` must omit orderly
filesystem and adapter close so recovery is tested from durable state.

The Node.js and Durable Object adapters MUST pass the same normative cases.
The suite MUST include, at minimum:

- path validation, canonical aliases, UTF-8 names, ordering, and every root
  rule;
- regular files, directories, symlinks, dangling links, cycles, relative
  targets, hard links, link counts, and final-link follow rules;
- defaults, mode masking, every timestamp transition, monotonic timestamps,
  and inode identity across rename and hard links;
- byte, UTF-8 text, range, zero-length, EOF, stream, abort, backpressure, and
  file-size-limit reads;
- atomic read-lease acquisition, renewal, release, expiry, crash cleanup, and
  survival through concurrent garbage collection;
- create, replace, exclusive, streamed failure, range overwrite, zero-filled
  extension, shrink, and grow writes;
- `replaceRange` insertion, deletion, replacement, invalid ranges, input
  copying, timestamp behavior, and ordered combinations with `writeRange` and
  `truncate`;
- streamed-write staging lease acquisition before allocation, renewal, atomic
  release on commit, best-effort release on abort, and expiry after crash;
- recursive and non-recursive `mkdir`, ordered and paginated `readdir`,
  `chmod`, compatible and incompatible rename, `unlink`, and every `rm`
  option;
- complete `readdir` below the configured cap, `EFBIG` above it when `limit`
  is omitted, and explicit paging at the cap;
- exact error codes and path fields for missing, wrong-type, invalid,
  over-limit, closed, read-only, busy, corruption, and schema failures;
- `"read"`, `"write"`, and `"exclusive"` transaction modes, capability
  limits, and read-only open without a write transaction;
- rollback after an injected failure at each mutation phase, including a
  failed final transaction after streamed staging;
- overlapping operations on one instance and two instances, with results
  equivalent to a serial transaction order;
- snapshot reads and streams during overwrite, rename, unlink, and garbage
  collection;
- close ownership, idempotent close, reopen persistence, migration from every
  released fixture, interrupted transaction recovery, and newer-schema
  refusal;
- capability reporting, bounded accounting, verification cursors, observer
  isolation, and interrupted/resumed garbage collection; and
- invariants after every mutation sequence: one root, no dangling directory
  entries, correct link counts, reachable manifests, and content hashes that
  match verified bytes.

Conformance tests MUST use a deterministic controllable clock where timestamp
values are asserted. State-machine tests SHOULD generate operation sequences
and compare observable behavior across adapters. Crash and corruption tests
MAY use adapter-specific fault-injection hooks, but the expected public errors
and recovered filesystem state MUST remain shared.

A behavior described in this document is implemented only when its shared
case passes against both required adapters. Experimental search, watch, and
file-handle APIs MUST NOT be counted toward version 0.1 conformance.

## Deferred decisions

The following are intentionally outside version 0.1 and require a later
normative revision:

- persistent file handles, append mode, fsync, and advisory locks;
- `copyFile`, recursive copy, reflink, and cloning APIs;
- caller-controlled `utimes`, access time, ownership, ACLs, and permission
  enforcement;
- search, glob, grep, indexes, and watch subscriptions;
- Windows paths, case-insensitive namespaces, and Unicode normalization;
- sparse-file extent discovery and hole punching;
- symbolic-link creation through a dangling final link;
- cross-database rename or links; and
- host runtime, FUSE, transport, or remote synchronization types.

These deferrals do not permit adapter-specific behavior to leak into the
portable methods defined above.
