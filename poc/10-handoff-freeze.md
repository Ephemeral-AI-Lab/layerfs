# AppleWorkspaceV1 implementation handoff freeze

Status: **highest-precedence PoC handoff authority**. This file resolves the
independent Apple, storage/performance and portability/correctness audits. If an
older PoC passage conflicts with this file, repair the older passage before
implementation; do not choose locally.

## 1. Final disposition

```text
architecture: GO
implementation handoff: GO only with the decisions below frozen
profile name: AppleWorkspaceV1
"99% complete": display label only after every profile ledger row passes
```

AppleWorkspaceV1 is an exact ordinary-APFS workspace compatibility profile. It
does not claim fast arbitrary-shell capture at large scale, mounted filesystem
integration, hostile-writer safety or hardware power-loss qualification.

## 2. Current source versus target

| Surface | Current source | Handoff target |
|---|---|---|
| core file | flat public `LogicalFile` plus legacy K64/F64 codecs | mode-free persistent byte-measured extent rope |
| namespace | provisional full-`BTreeMap` COW | persistent directory tree + global inode table |
| engine object read | redundant metadata queries/BLOB passes | one-fetch/one-auth borrowed row + ordered batches of at most 64 |
| engine publication | public pre-publication object insertion, caller root ID, one visible root, 100 ms busy wait | sole `Publication`, authenticated root, named generation-CAS refs, zero busy wait, fresh reconciliation |
| VFS/SDK | stubs | universal driver port and two-state workspace API |
| OS | host probe | Apple projection + Store-generation drivers |
| APFS product path | benchmark-private | selectively extracted library implementation |

Plans and benchmark code are not current product evidence.

## 3. Canonical authority model

### 3.1 Mode-free content roots

```text
FileStateV3
├── logical_len
├── extent_count
├── tree_level
├── file profile ID
└── mapping_root

DirectoryStateV1
├── entry_count
├── tree_level
├── namespace profile ID
└── mapping_root

InodeRecordV1
├── kind
├── namespace_ref_count
├── content_root
└── metadata_root

PortableMetadataV1
├── permission_mode
└── mtime_seconds + mtime_nanoseconds
```

Mode has exactly one owner: `PortableMetadataV1`. Remove it from file and
directory content roots. `chmod` updates metadata/inode/inode-table roots, not
file content. Supported modes:

```text
regular:   0o777
directory: 0o1777
setuid/setgid: typed UnsupportedAppleMetadata
uid/gid: mapped to current workspace owner; noncanonical
atime/ctime/birthtime: noncanonical/excluded in v1
mtime: canonical nanosecond timestamp
```

### 3.2 Namespace/inode topology

```text
NamespaceRootV1
├── root_directory_inode: InodeId
├── inode_table_root: InodeTableNodeId
└── profile_id

DirectoryTree
└── CanonicalName -> InodeId

InodeTable
└── InodeId -> InodeRecordObjectId

MetadataTree
└── MetadataKey -> MetadataValueRoot
```

Inode records are separate authenticated objects; inode-table leaves remain
fixed-size:

```text
inode-table leaf entry = InodeId[32] + InodeRecordObjectId[32] = 64 B
node fixed bytes        = 44 B
non-root entries        = 64..=127
root leaf entries       = 1..=127
overflow split          = 128 -> 64/64
root branch children    = 2..=127; one child collapses
empty filesystem        = one root-directory inode entry, never empty table
```

Directory and metadata trees retain the 8 KiB byte limit, 40% non-root encoded
fill, deterministic half-nearest split and left-then-right borrow/merge rules.
The exact node/record bytes, inode and metadata branch descriptors, composite
key order, 8,192-byte envelope-inclusive limit, empty forms, mode/mtime value
bytes and global inode-table closure rules are frozen in
`02-data-structures-and-algorithms.md` section 10.1. Independent literal
goldens are required before a writer exists.

### 3.3 Inode allocation

Private durable engine metadata:

```text
StoreId:           [u8;32]
next_inode_serial: u64
```

Inside the same expected-head publication transaction:

```text
InodeId = BLAKE3(
    "layerfs/inode-id/v1\0"
    || StoreId
    || next_inode_serial:u64be
)
```

- increment the serial in that transaction;
- abort consumes no durable serial;
- unequal collision is fatal;
- compaction preserves StoreId and serial exactly;
- deterministic tests inject a fixed StoreId;
- separate Stores importing equal native trees may have different operational
  namespace roots.

### 3.4 Hard links

- regular-file hard links only;
- directory and symlink hard links typed unsupported;
- multiple names reference one `InodeId`;
- rename preserves `InodeId`;
- content or metadata update changes one inode record and all aliases;
- unlink decrements `namespace_ref_count`;
- unlink last name removes the inode-table entry in the new root;
- root directory has zero namespace parents; every other directory exactly one;
- validators reject directory cycles/multiple parents;
- external capture groups by opaque driver link key;
- stable observed native link count must equal in-workspace aliases, otherwise
  `ExternalHardLinkBoundary`;
- `into_external` retains an opaque `NativeHardLinkKey -> InodeId` topology map
  while invalidating every content/range/no-mutation authority;
- exact surviving group key reuses its prior ID; a new group allocates;
- on split, the surviving original group keeps its ID and new groups allocate;
- on merge, the current native target group's prior ID survives and the other
  prior IDs disappear;
- without a live topology map, every complete group allocates a new ID, so a
  reopened no-op external capture may change the operational namespace root;
- `InodeId` is operational topology identity, not user-visible birth identity;
- cold materialization builds one representative then native links;
- warm replacement reconstructs the entire alias group, never one path only.

### 3.5 Canonical names

Portable v1 names are exact canonical UTF-8 bytes:

```text
length 1..=255 bytes
not "." or ".."
no NUL, slash or backslash
no Unicode normalization before identity
```

The Apple driver separately proves representability and case/normalization
collision behavior for the opened volume. Stop calling names arbitrary raw
filesystem bytes.

## 4. Metadata freeze

```text
MetadataEntryV1
├── domain: canonical UTF-8 bytes
├── key: canonical bytes
├── required_for_exact_projection = true
└── value_file_root: mode-free FileStateRoot
```

All values, including tiny ones, use one bounded streaming representation. This
handles large resource forks without inline size exceptions.

Apple profile:

```text
apple.xattr/<raw xattr name> -> value_file_root
apple.acl                    -> canonical ordered ACE value root
apple.bsd-flags              -> canonical u32 value root
```

- `com.apple.FinderInfo` and `com.apple.ResourceFork` exist only as entries in
  the xattr map; no duplicate finder/resource-fork domains;
- freeze included ordinary readable/writable xattrs;
- unreadable, privileged, internal or unrepresentable attributes fail
  `UnsupportedAppleMetadata`;
- ACL codec records ACE order, qualifier UUID, allow/deny tag, flags and rights;
- supported flags: `UF_NODUMP`, `UF_IMMUTABLE`, `UF_APPEND`, `UF_OPAQUE`,
  `UF_HIDDEN`;
- `UF_COMPRESSED`, `SF_DATALESS` and privileged system flags excluded;
- apply mode, ACL, exact xattr set, then final restrictive flags;
- clone path removes seed-only xattrs and verifies the exact final set;
- no private LayerFS ownership xattr may appear on a published inode.

### 4.1 AppleMetadataV1 exact profile

Metadata tree keys are frozen as `(domain,key)`:

| Meaning | Domain bytes | Key bytes | Value bytes |
|---|---|---|---|
| mode | `portable` | `mode` | `u32be` with the kind-specific mask from section 3.1 |
| mtime | `portable` | `mtime` | `i64be seconds || u32be nanoseconds`, nanos `<=999_999_999` |
| xattr | `apple.xattr` | exact native name bytes, `1..=127`, no NUL | exact value through the mode-free file rope |
| ACL | `apple.acl` | empty | `AppleAclV1` bytes below through the mode-free file rope |
| BSD flags | `apple.bsd-flags` | empty | one nonzero supported `u32be` mask; absence means zero |

The included xattr set is every no-follow `listxattr` name whose exact value is
readable and can be restored and read back exactly, including
`com.apple.FinderInfo` and `com.apple.ResourceFork`, except this frozen typed
exclusion list:

```text
com.apple.decmpfs
com.apple.provenance
com.apple.quarantine
com.apple.macl
com.apple.rootless
com.layerfs.projection-owner-v1
```

Encountering an excluded, unreadable, oversize, NUL-containing, or
non-restorable attribute returns `UnsupportedAppleMetadata`; nothing is
silently dropped. If the OS synthesizes an excluded/system attribute on a new
temp, exact verification fails and the temp is not published. Unknown ordinary
attributes are admitted only by the same write/read-back test, not by prefix.

`AppleAclV1` stores the exact ACE order returned by the no-follow extended-ACL
API; it never sorts, deduplicates, or translates names:

```text
magic[8]          = "LFS4ACL\0"
version:u16be     = 1
entry_count:u16be = 1..=128

repeated entry_count times:
  tag:u8            = allow 1 | deny 2
  qualifier_kind:u8 = UUID 1
  reserved:u16be    = 0
  flags:u64be
  rights:u64be
  qualifier_uuid[16]
```

No qualifier-absence form exists; absence of an ACL is absence of the
`apple.acl` entry. The allowed rights mask is exactly `0x0010_3ffe`
(`READ_DATA`, `WRITE_DATA`,
`EXECUTE`, `DELETE`, `APPEND_DATA`, `DELETE_CHILD`, `READ_ATTRIBUTES`,
`WRITE_ATTRIBUTES`, `READ_EXTATTRIBUTES`, `WRITE_EXTATTRIBUTES`,
`READ_SECURITY`, `WRITE_SECURITY`, `CHANGE_OWNER`, and `SYNCHRONIZE`). The
allowed flags mask is exactly `0x0002_01f0` (`ENTRY_INHERITED`, `ENTRY_FILE_INHERIT`,
`ENTRY_DIRECTORY_INHERIT`, `ENTRY_LIMIT_INHERIT`, `ENTRY_ONLY_INHERIT`, and
`FLAG_NO_INHERIT`); every other bit/tag/qualifier kind and every trailing byte is
rejected. Exact length is `12 + 36*entry_count`, at most 4,620 bytes.

The supported BSD flag mask is exactly `0x0000_800f`:
`UF_NODUMP=0x1`, `UF_IMMUTABLE=0x2`, `UF_APPEND=0x4`, `UF_OPAQUE=0x8`, and
`UF_HIDDEN=0x8000`. Materialization applies content, mode, ACL, xattrs, then
final restrictive flags and reads back mode + ACL + exact xattr/flag set. Any
OS ACL/mode adjustment that differs from the canonical pair is
`UnrepresentableMetadata`, before visible replacement.

An existing visible destination carrying `UF_IMMUTABLE` or `UF_APPEND` may be
retained only by an exact freshly verified no-op. Any content, topology or
metadata replacement returns `NativeProtected` before mutation. The PoC never
clears a restrictive flag in place: that would visibly weaken the old state and
needs a separate crash-recoverable protocol. A caller may explicitly clear it
in an External workspace and capture that transition, or cold-materialize the
new root to a different destination. Tests prove protected non-no-op attempts
leave the destination byte-for-byte and metadata-for-metadata unchanged.

Use one mode-0700, same-volume LayerFS-owned staging directory; do not extract
the G5 `com.layerfs.projection-owner-v1` helper unchanged because it leaks onto
the renamed visible inode.

## 5. Engine authority freeze

`Publication` is the sole durable mutation capability. Store-level public
object insertion is not a product write path.

One canonical transition contains:

```text
BEGIN IMMEDIATE
validate expected ref name + generation + root
allocate required InodeIds
stream/authenticate/insert payload, file, directory, inode and metadata objects
insert authenticated NamespaceRoot + delta
update one named ref
COMMIT exactly once
freshly reconcile ambiguous return
```

Rules:

- root ID is computed from canonical NamespaceRoot bytes, never caller-supplied;
- `busy_timeout=0`; no hidden retry/pool;
- fetched/new/incumbent identity checks unconditional;
- Verified default; TrustedLocalDev explicit Store lifetime;
- Verified-after-Trusted scrub required;
- conflict after native managed mutation transitions workspace to
  `ExternalDirtyConflict`; never silently replay against a new head.

## 6. Object-reader performance freeze

The current reusable reader is forbidden as the final product path because one
object load performs redundant metadata queries, three BLOB passes and another
in-memory validation.

Required API:

```text
with_authenticated_canonical(id, callback):
    SELECT kind, canonical_length, canonical_bytes once
    borrow row bytes while alive
    authenticate ObjectId once
    strict role/profile decode once
    callback before advancing/dropping row

for_each_authenticated_payload_batch(ids, max=64, callback):
    one ordered query per <=64 references
    preserve duplicates and requested order
    reject missing/wrong-role/mismatch exactly
    expose one borrowed authenticated object at a time
```

Terminal invariants:

```text
separate object-length query                          0
authentication passes per fetched canonical row      1
decode passes per fetched canonical row              1
payload batch maximum references                    64
100 MiB / 5,284 occurrences expected payload batches 83
```

SQLite BLOB-per-object is the selected complete AppleWorkspaceV1 backend.
Packed carriers remain deferred: retained carrier/packed-CAS evidence was
negative or below promotion threshold. Reconsider only with new product-path
evidence.

## 7. Correct namespace/inode complexity

Let `I` be inode-table entries.

```text
resolve path = sum_i [O(log D_i) + O(log I)]

regular-file content edit after resolution:
    file rope O(B + K + log E)
    one InodeRecord
    one inode-table path O(log I)
    NamespaceRoot
    zero directory-tree changes

create/unlink in parent p:
    parent directory path O(log D_p)
    parent directory InodeRecord
    affected inode record(s)
    bounded inode-table paths O(log I)

same-parent rename: one directory tree
cross-parent rename: exactly two directory trees
```

Stable `InodeId` means a descendant content change does not rewrite ancestor
directory name maps.

## 8. Safe universal native ports

Two narrow ports exist:

```text
layerfs-vfs::ProjectionDriver       native workspace semantics
layerfs-engine::StoreGenerationDriver durable Store-selector installation
```

`layerfs-os` implements both. Adding a platform that fits the frozen semantics
changes only `layerfs-os` plus platform-specific conformance tests. A genuinely
new semantic capability requires a versioned neutral-port change.

### 8.1 Handle-anchored ProjectionDriver

Use opaque driver types:

```text
NativeObjectToken
NativeHardLinkKey
DirectoryHandle
RegularFileHandle
OwnedTempHandle
```

The neutral port uses boxed erased handles, not associated handle types, so a
single runtime may hold `dyn ProjectionDriver` implementations without knowing
the OS. The compile target is this object-safe shape (method details may only
add arguments/results already listed below):

```rust
use std::io::{Read, Seek, Write};

pub trait DirectoryHandle: Send {}
pub trait RegularFileHandle: Read + Seek + Send {}
pub trait OwnedTempHandle: Read + Write + Seek + Send {}

pub trait ProjectionWorkspace: Send {
    fn root_directory(&self) -> Result<Box<dyn DirectoryHandle>>;
    fn enumerate_at<'a>(
        &'a self,
        parent: &'a dyn DirectoryHandle,
    ) -> Result<Box<dyn Iterator<Item = Result<NativeEntry>> + 'a>>;
    // Every remaining method accepts an erased handle plus one basename.
}

pub trait ProjectionDriver: Send + Sync {
    fn open_workspace(
        &self,
        path: &std::path::Path,
        policy: WorkspacePolicy,
    ) -> Result<Box<dyn ProjectionWorkspace>>;
}
```

This freezes object-safe intent; the step-1 in-memory fault driver is the
compile proof and must exist before OS implementation work proceeds.

Essential semantics:

```text
open_workspace(path, policy) -> ProjectionWorkspace
ProjectionWorkspace.capabilities()        // workspace/volume scoped
root_directory() -> DirectoryHandle
enumerate_at(&DirectoryHandle) -> bounded iterator + '_
open_directory_at(parent, raw_name, expected) -> DirectoryHandle
open_regular_at(parent, raw_name, expected) -> RegularFileHandle
read_link_at(parent, raw_name, expected, sink)
read_metadata_at(parent, raw_name, expected, sink)
create_directory_at(...)
create_empty_temp_at(staging_parent) -> OwnedTempHandle
clone_to_new_temp(source_handle, staging_parent) -> OwnedTempHandle | Unsupported
create_symlink_at(...)
create_regular_hard_link_at(...)
write_all_at(&mut temp, offset, bounded_source)
set_len(&mut temp, len)
replace_metadata_exact(&mut temp_or_entry, metadata_stream)
verify_metadata_exact(...)
sync_file(&mut temp, requested_class) -> achieved_class
atomic_replace(temp /* consumed */, parent, name, expected) -> ReplaceOutcome
unlink_regular_at / unlink_symlink_at / remove_directory_at
sync_directory(&DirectoryHandle) -> achieved_class
stat_handle / stat_entry_at / revalidate_entry
```

Every nested operation is parent-handle + one validated basename; never
enumerate a path and reopen a multi-component string. Clone creates a new temp;
it does not target an already-created file.

Errors distinguish:

```text
Unsupported
ConfirmedBeforeVisibility
Conflict
VisibilityAmbiguous
DurabilityAmbiguous
```

Fallback is allowed only from `Unsupported` or confirmed pre-visibility
failure. Potentially visible outcomes reconcile first.

### 8.2 Workspace-scoped capabilities

```text
name comparison/preservation profile
runtime name/path limits
atomic replace / rename-excl / rename-swap
whole-file clone
regular-file hard links / symlinks
xattrs-no-follow / ACL / user flags
stable file ID / change generation
durability classes
sparse allocation observation (diagnostic only)
```

### 8.3 Durability vocabulary

```text
ProcessCrashReconciled
HostCrashOrdered
DeviceFlushRequested
PowerLossQualified
```

Ordinary `fsync` does not prove stable media. `F_FULLFSYNC` requests a device
flush but remains best effort. AppleWorkspaceV1 requires
`ProcessCrashReconciled`; stronger classes are separately requested/reported.

Minimal file publication:

```text
write/clone/patch content
replace exact metadata; restrictive flags last
one final file sync
atomic rename
parent directory sync
fresh identity/metadata construction verification
```

Owned construction proof does not reread the whole file. Full content
verification is for reopened, external, substituted or ambiguous state.

## 9. Workspace API and quiescence

```text
ManagedWorkspace
├── private LayerFS-owned native location
├── no resolvable public path accessor
├── managed reads/edits
├── capture(&mut self)
└── into_external(self) -> ExternalWorkspace

ExternalWorkspace
├── path()
├── capture_quiescent(&mut self)
└── discard(&mut self)
```

A caller-known destination is External from creation. Capture errors do not
consume the workspace.

`capture_quiescent` contract:

- caller attests the tree remains unchanged during scan;
- evaluator proves its owned process group and writer leases are gone;
- VFS rejects known registered writers;
- driver performs no-follow before/after identity/metadata/digest checks;
- independently launched, escaped or hostile same-UID writers are excluded;
- exact fast arbitrary Bash capture requires future FSKit/mount/write
  interception or snapshot authority.

Conflict after managed native mutation:

```text
ManagedDirty --expected-head conflict--> ExternalDirtyConflict
```

Caller may inspect, discard/rebuild, or explicitly full-scan against a newly
selected base; never automatic replay.

## 10. AppleWorkspaceV1 native profile

Qualified first host/volume:

```text
current arm64 test host and recorded macOS version
LayerFS-created dedicated workspace
local writable APFS
current-user ownership
same-volume mode-0700 staging
runtime name/capability profile
cooperative exclusive session
```

Required:

- regular/empty files, nested/empty directories;
- relative, absolute and dangling symlinks without NUL;
- regular-file hard links wholly contained in workspace;
- read/write/append/truncate/create/unlink/mkdir/rmdir/rename;
- Bash/editor/compiler and writable mmap after flush/unmap/process exit;
- mode + mtime; exact supported xattr/ACL/user-flag set;
- cold stream, exact live no-op, optional clone, same-offset patch, full
  length-changing fallback;
- cooperative full scan, reopen, history/fork/rollback, offline compaction.

Typed exclusions:

- device/FIFO/socket, directory/symlink hard links;
- hard-link groups extending outside workspace;
- setuid/setgid, uid/gid preservation, privileged/internal flags;
- unreadable/system-managed metadata;
- exact sparse-hole layout;
- live database consistency, hostile/unregistered writers;
- cross-volume replacement, whole-tree atomicity, power-loss guarantee;
- mounted/write-intercepted fast arbitrary writes.

APFS clone is selected from runtime volume capability, not the string “APFS.”
It is one syscall with unspecified physical/latency complexity. Clone copies
metadata subject to Apple rules; exact metadata set reconciliation is required.

Per-file rename provides old/new destination-path lookup, not whole-workspace
atomicity or switching of already-open descriptors. Qualified whole-tree
`RENAME_SWAP` remains a future route.

## 11. Exact external-capture accounting

Group hard links first in a LayerFS-owned temporary SQLite scratch table:

```text
key   = opaque scan-only hard-link key
value = representative, observed count, stable native count, InodeId
```

RAM remains bounded; scratch disk `O(paths)`. Read one representative per group.

Simple correct two-pass changed-file algorithm:

```text
T_external = Theta(
    paths
  + unique current regular bytes for digest
  + changed current bytes reread for CDC/CAS
  + uncached prior logical bytes for prior digest
  + represented metadata bytes
  + indexed hard-link grouping
)
```

Do not add a candidate-data spool before measurement.

Required counters:

```text
native_digest_pass_bytes
native_changed_cdc_pass_bytes
prior_digest_stream_bytes
unique_regular_inodes_scanned
hard_link_paths / groups / scratch_bytes
metadata_list/value calls and bytes
```

## 12. Fragmentation, mapping and resources

```text
ideal_target_extents = max(1, ceil(F / 16 KiB))
fragmentation_ratio  = E / ideal_target_extents
```

Report the ratio. Expose explicit `repack_file`, `Theta(F)`. A ratio above 2
with at least 256 extents may produce a diagnostic recommendation only; it is
not canonical and does not block edits automatically.

Selected mapping gate:

```text
file mapping / logical bytes <1%
only for files >=1 MiB in the frozen deterministic CDC population
```

Report absolute tiny-file overhead separately.

Connection/resource shape:

```text
one writer connection
at most two explicit query-only readers
no pool; busy_timeout=0
cache_size=1280 pages each with observed page_size=4096 for Apple profile
configured cache budget reported separately from RSS/Q
Q <=8 MiB; individual buffer <=1 MiB; terminal owned state zero
```

Offline compaction preflights available space for the new sibling generation's
allocated upper bound, disk-backed mark database, candidate SQLite
rollback-journal/temp high-water bound, `CURRENT.tmp` and a safety margin.
Report total peak storage separately as old generation + new generation + mark
database + candidate journal/temp + selector temporary bytes.

## 13. Crash-safe Store generations

```text
store/
├── LOCK
├── CURRENT
├── generation-0000000000000007.sqlite
└── owned candidate generations
```

`CURRENT` and `CURRENT.tmp` contain exactly one selector record:

```text
magic[8]          = "LFSCUR1\0"
version:u16be     = 1
generation:u64be
filename_len:u16be = 34
filename[34]      = "generation-" + 16 lowercase hex digits + ".sqlite"
schema_version:u32be
store_id[32]
profile_id[32]
checksum[32] = BLAKE3(
  "layerfs/store-current/v1\0" || every prior selector byte
)
EOF immediately after checksum
```

The filename's hex value must equal `generation`; uppercase, alternate width,
slash, NUL, suffixes and trailing bytes are rejected. Genesis creates
generation zero and its selector under the Store initialization lock using the
same protocol: create generation zero with O_EXCL, commit/close/verify/sync it,
write and sync `CURRENT.tmp`, atomically install `CURRENT`, sync the Store
directory, then reopen only through `CURRENT`. Any ambiguous selector result
uses the same fresh reconciliation vocabulary. Missing `CURRENT` with any
generation file present is `SelectorMissing`; recovery never guesses. A valid
selected old generation wins over every valid but unselected candidate.

Selector replacement reports `SelectorInstalled`, `SelectorNotInstalled`, or
`SelectorVisibilityAmbiguous`. After an ambiguous result, a fresh selector read
and full selected-generation verification resolves installed versus old; any
other state remains ambiguous and preserves both generations. Under the
maintenance lock, `CURRENT.tmp` may be removed only after valid `CURRENT` is
verified and the temp's checksum, `store_id`, profile and nonselected candidate
ownership all match; unknown residue is preserved and reported. Old generation
removal is permitted only after the new selector and reopened generation pass.

Compaction:

1. acquire external Store-directory maintenance lock;
2. require zero readers/writers/workspaces/recovery pins;
3. copy into fresh O_EXCL generation file;
4. preserve StoreId, next inode serial, schema/profile, refs/generations;
5. commit, verify retained closure, close SQLite, reject journal/temp residue;
6. sync generation file;
7. write/sync checksummed `CURRENT.tmp` with generation, filename,
   schema/profile;
8. atomically replace `CURRENT` through `StoreGenerationDriver`;
9. sync Store directory;
10. reopen only through `CURRENT` and verify;
11. retain old generation until selector and new generation are verified;
12. recovery trusts only a checksummed selector + complete verified generation,
    never highest filename.

One-COMMIT means one publication COMMIT per canonical ref transition.
Compaction has a separate candidate-generation transaction and selector install.

## 14. Fast implementation order

1. Apply Cargo inversion and compile both neutral ports with in-memory drivers.
2. Freeze mode-free file, namespace, inode and metadata bytes/goldens.
3. Implement file/namespace/inode differential algorithms.
4. Implement one-fetch/one-auth batch-64 reader and sole `Publication` with
   StoreId/inode allocator, refs, trust and reconciliation.
5. Implement Apple full-stream materialization and full-scan capture.
6. Expose ManagedWorkspace/ExternalWorkspace and run Bash/mmap flow.
7. Add hard-link topology and frozen Apple metadata.
8. Add generation-selector offline compaction and crash matrix.
9. Add APFS clone/patch behind the correct route.
10. Run one closure and stop.

## 15. Handoff hard gates

- [ ] no conflicting older passage remains;
- [ ] exact codecs/profile preimages/goldens frozen;
- [ ] no platform syscall/cfg above `layerfs-os`;
- [ ] handle-anchored driver and Store-generation ports compile with fault drivers;
- [ ] sole Publication and transactional inode allocation;
- [ ] one-fetch/one-auth batch-64 object reads;
- [ ] `ManagedWorkspace` cannot expose a resolvable native path;
      `ExternalWorkspace` may expose one and never carries managed fast authority;
- [ ] hard-link closure and metadata set authority exact;
- [ ] no G5 private ownership xattr reaches product output;
- [ ] durability class explicit; no power-loss overclaim;
- [ ] external capture formula/counters honest;
- [ ] namespace/inode complexities corrected;
- [ ] compaction selector recovery exact;
- [ ] AppleWorkspaceV1 ledger is 100% green before the “99% complete” label.
