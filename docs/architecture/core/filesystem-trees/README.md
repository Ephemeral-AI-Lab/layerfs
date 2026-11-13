# Filesystem trees

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

A complete namespace as canonical objects: one inode table and one directory
tree per directory, with attributes, symlinks and hard links.

| | |
| --- | --- |
| Layer | [Core](../README.md) |
| Crates | `layerfs-content` |
| Source | [`filesystem/`](../../../../core/crates/layerfs-content/src/filesystem) |
| Uses | [Objects](../objects/README.md), [Files](../files/README.md) |
| Used by | [Compare and merge](../compare-and-merge/README.md), [Engine: Project Init](../../engine/project-init/README.md), [Engine: Commit](../../engine/commit/README.md), [Engine: Workspace](../../engine/workspace/README.md), [Engine: Store calls](../../engine/store-calls/README.md) |
| Flows | [Project Init](../../flows/init.md), [A Commit](../../flows/commit.md), [Fork and merge](../../flows/merge.md) |

## Picture

```text
 FilesystemRoot (LFS6FSR) -----> inode table: B+tree keyed by serial
   profile, scope, root serial     (LFS6INT) leaf row: serial -> InodeValue
                                        | content_root, by kind
          +-----------------------------+------------------------+
          v                             v                        v
  Directory: B+tree of           RegularFile: file      Symlink: target
  name -> serial (LFS6NSP)       root (Files)           object (LFS4LNK)

 caller (Project Init, Commit, merge): sorted bindings + inode values
   --> update: validate --> directories --> references --> inodes --> root
   reads via caller's AuthenticatedObjects; pages to caller's consumer
```

An operation never opens a store: one
[`FilesystemObjects`](../../../../core/crates/layerfs-content/src/filesystem/objects.rs)
value carries the caller's provider and consumer.

## What it does

### Directories and inodes

```text
 inode leaf rows                                directory "/"  (LFS6NSP)
 1  Directory   count 0  -> dir tree of "/"     ".git" -> 5 | "src" -> 2
 2  Directory   count 1  -> dir tree of "src"   directory "src"
 3  RegularFile count 2  -> file root           "a.rs" -> 3 | "b.rs" -> 3
 4  Symlink     count 1  -> LFS4LNK target
 resolve("src/a.rs"): root serial -> "/" lookup "src" -> 2
                      -> lookup "a.rs" -> 3 -> RegularFile value
```

| Behaviour | Detail | Source |
| --- | --- | --- |
| Names bind serials | A name never points at an object; it points at a serial, and the serial's row says what the inode is. Serials belong to the scope stored in the root; two scopes are not comparable | [`identity.rs`](../../../../core/crates/layerfs-content/src/filesystem/identity.rs) |
| Inode value | 73 bytes: kind, `namespace_ref_count`, `content_root`, metadata ([Objects: Inode leaves](../objects/README.md#inode-leaves)) | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| Inode kinds | `RegularFile`, `Directory`, `Symlink`; any other kind code is refused | same |
| No filter | Every supplied binding is stored, including `.git`, ignored files and dependencies; no ignore rule is read | [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs) |
| Byte order | Names compare as raw bytes (`PathName::cmp`), so uppercase sorts first; serials as `u64`; attributes by domain then key | [`path.rs`](../../../../core/crates/layerfs-content/src/filesystem/path.rs) |
| Resolution | `FilesystemRead` loads the checked root once and resolves one component at a time; never the whole inode table. An unbound name is `PathNotFound`. Lookups of several names share ancestor reads; a provider may keep decoded pages in a bounded LRU | [`read.rs`](../../../../core/crates/layerfs-content/src/filesystem/read.rs), [`decoded.rs`](../../../../core/crates/layerfs-content/src/filesystem/decoded.rs) |
| Listing | Paged by entry count and bytes; resumes after the last delivered name | [`directory/read.rs`](../../../../core/crates/layerfs-content/src/filesystem/directory/read.rs) |

### Pages and the root

```text
 page = LFSO envelope 13 + node header 31 + rows
 header: magic 8 | ver u16 = 1 | role | level | flags = 0 | count u16
         | subtree count u64 | subtree bytes u64
 FilesystemRoot value (116 B): "LFS6FSR\0" | ver 1 | role 6 | flags 0
         | profile ID 32 | scope ID 32 | root serial u64 | inode table ID 32
```

| Page | Magic, role | Row | Fill (non-root) | Source |
| --- | --- | --- | --- | --- |
| Directory leaf | `LFS6NSP`, 1 | name len u16, name, serial u64 | ≥ 3,277 canonical bytes (2/5 of 8,192) | [`sorted/format.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/format.rs) |
| Directory branch | `LFS6NSP`, 2 | name len, last name of child, child ID | same | same |
| Inode leaf | `LFS6INT`, 7 | serial u64, `InodeValue` (81 B) | 50 to 100 rows | [`inode/codec.rs`](../../../../core/crates/layerfs-content/src/filesystem/inode/codec.rs) |
| Inode branch | `LFS6INT`, 8 | serial u64, child ID (40 B) | 64 to 127 children | same |
| Attribute leaf / branch | `LFS4MET`, 9 / 10 | `37 + domain + key` / `36 + domain + key` bytes | ≥ 3,277 bytes | [`attributes/codec.rs`](../../../../core/crates/layerfs-content/src/filesystem/attributes/codec.rs) |

Page sizes are exact arithmetic (`44 + sum of row widths`); nothing is
encoded to test a fit. Root pages are exempt from the fill rule. An empty
directory is one empty leaf; an empty inode page is never valid. `profile` is
the ID of the frozen `layerfs/namespace-profile/scoped-inline/v1` description;
`scope` is `scope_for_seed(seed)`, the ID of `layerfs/inode-scope/v1\0` plus a
32-byte seed ([`root.rs`](../../../../core/crates/layerfs-content/src/filesystem/root.rs)).

One sorted B+tree engine mutates every page format through the `Format` trait.
A merge reads only the children its changes reach, the neighbour an underfull
page joins, and the other children of a branch that splits or joins, at most
256 per batch; every unread child keeps its ID
([`sorted/page.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/page.rs)).

### Attributes

| Kept attribute | Grammar | Source |
| --- | --- | --- |
| `portable/mode` | 4 bytes; files `0o777` bits, directories `0o1777` (sticky allowed), symlinks exactly `0o777` | [`attributes/portable.rs`](../../../../core/crates/layerfs-content/src/filesystem/attributes/portable.rs) |
| `portable/mtime` | seconds `i64`, nanoseconds `u32` below 10^9 | same |
| Other domains | domain 1–64 bytes UTF-8, key ≤ 255, value 1–32,768 bytes, opaque; `portable` accepts only `mode` and `mtime` | [`attributes/keys.rs`](../../../../core/crates/layerfs-content/src/filesystem/attributes/keys.rs) |

Two forms of `metadata_root`: **inline** (kind bit `0x80`: mode and mtime
inside the inode value, no tree) and **root** (an attribute-tree ID whose rows
point at extent-only file values). Project Init writes inline
([`import/namespace.rs`](../../../../engine/crates/layerfs-project/src/import/namespace.rs));
Commit writes inline unless the stored tree has keys beyond the portable two,
then patches that tree with sorted `Set`/`Remove` requests
([`construction/namespace/inodes.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/inodes.rs),
[`attributes/patch.rs`](../../../../core/crates/layerfs-content/src/filesystem/attributes/patch.rs)).
Owner, group, atime, ctime and device numbers are not kept; no production
caller writes a non-portable key, and FUSE answers extended-attribute calls
with `ENOSYS` ([`request/callbacks.rs`](../../../../engine/crates/layerfs-fuse/src/request/callbacks.rs)).

### Symlinks and hard links

```text
 symlink value: "LFS4LNK\0" | ver 1 | role 5 | flags 0 | len u16 | target
 directory "a": "x" -> 3      directory "b": "y" -> 3
 inode 3: RegularFile, namespace_ref_count 2
 add binding -> +1   remove binding -> -1   0 -> row removed
```

| Case | Result | Source |
| --- | --- | --- |
| Symlink target | opaque bytes ≤ 4,096, no NUL; never resolved, normalized or UTF-8-checked; broken and absolute targets stored as given | [`symlink.rs`](../../../../core/crates/layerfs-content/src/filesystem/symlink.rs) |
| Two names, one serial | one inode, one content root, count 2 (regular files and symlinks only) | [`references/reduce.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/reduce.rs) |
| Count of a new inode | the bindings this operation retained; the supplied count is ignored | [`references/record.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/record.rs) |
| Count of an existing inode | stored count plus the signed effect the merge observed; aliases off the changed paths are not read | [`references/reduce.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/reduce.rs) |
| A move | addition observed before removal, so a count never passes through 0 | `observe` in [`update.rs`](../../../../core/crates/layerfs-content/src/filesystem/update.rs) |
| A directory reaches 0 | its entries lose one reference each, a page at a time, stopping at an inode that still has an alias | [`references/release.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/release.rs) |

Equal bytes in two files do not make a hard link: a link is a shared serial.
Directory parentage stays singular. Older binaries may reject a tree with a
multiply bound symlink; keep CLI, daemon, worker and bundle builds matched.

## Invariants

| Rule | Enforced by |
| --- | --- |
| A tree is complete: no ignore rule or path filter in construction | [`update.rs`](../../../../core/crates/layerfs-content/src/filesystem/update.rs) applies every supplied row |
| Rows are strictly ascending; a duplicate or unsorted row is refused, never normalized | encoders and `reject_unordered` in [`sorted/format.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/format.rs); `DirectoryUpdate::check` in [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs) |
| Only the profile `scoped-inline/v1` is accepted, before any mutation | `FilesystemRoot::decode` in [`root.rs`](../../../../core/crates/layerfs-content/src/filesystem/root.rs) |
| Serials are 1 to `i64::MAX`; Content never allocates them | `InodeIdentity::new` in [`identity.rs`](../../../../core/crates/layerfs-content/src/filesystem/identity.rs) |
| Root is a directory with count 0; other directories have count 1; files and symlinks ≥ 1 | `InodeValue::validate` in [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| Every final count is derived from retained bindings | [`references/reduce.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/reduce.rs) |
| No directory cycle and no unreachable new directory | [`validate/cycles.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate/cycles.rs) |
| An unchanged subtree keeps its ID and is not read | [`sorted/merge.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/merge.rs) |
| A failed operation emits no root and releases its ordering backing once | `run_state` in [`update.rs`](../../../../core/crates/layerfs-content/src/filesystem/update.rs) |

Equal input rows give equal pages and IDs; identity also depends on scope and
serials. Whether two update histories reaching the same rows always give the
same page partition is not established from source.

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Name component | 1–255 bytes, UTF-8, no NUL, `/` or `\`, not `.` or `..`; `InvalidPath`, `InvalidUtf8`, `PathLimitExceeded` | [`path.rs`](../../../../core/crates/layerfs-content/src/filesystem/path.rs) |
| Path | 4,096 bytes, 256 components | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Tree page, level | 8,192 bytes, 31 | same |
| Entries per directory, inodes per tree | no cap: they grow as pages | [Validation](validation.md) |
| Foreign profile or scope | `UnsupportedProfile`, `ScopeMismatch` | [`root.rs`](../../../../core/crates/layerfs-content/src/filesystem/root.rs), [`validate.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate.rs) |
| Malformed page | `UnsupportedFraming`, `TrailingBytes`, `LengthMismatch`, `NonCanonicalOrdering`, `InvalidRecord` | [`sorted/format.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/format.rs) |

Every named limit is in [Validation](validation.md#limitsrs).

## Demonstration

No Docker, root, feature or environment variable; any platform. Each test
command gets a wall timeout of at most 120 s.

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`filesystem_codec.rs`](../../../../core/crates/layerfs-content/tests/filesystem_codec.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test filesystem_codec` | any | Every tree grammar matches sealed fixtures under `tests/fixtures/filesystem/`; wrong roles, flags, trailing bytes and an over-full leaf are refused |
| [`filesystem_hardlinks.rs`](../../../../core/crates/layerfs-content/tests/filesystem_hardlinks.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test filesystem_hardlinks` | any | Aliases across directories reach one derived count; moves never drop it to zero |
| [`filesystem_updates.rs`](../../../../core/crates/layerfs-content/tests/filesystem_updates.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test filesystem_updates` | any | Restating a binding keeps the root ID; content-only changes touch only the inode table |
| [`filesystem_primitives_candidate.rs`](../../../../core/crates/layerfs-content/examples/filesystem_primitives_candidate.rs) | `cargo +1.85.1 run --locked -p layerfs-content --example filesystem_primitives_candidate -- --files 2000 --changes 200` | any | Sorted directory and inode merges over a generated tree, printing IDs and page partition (timings diagnostic) |

## Pages

| Page | What it covers |
| --- | --- |
| [Updates](updates.md) | `update.rs`, input and backed state, the reference reducer, statistics. |
| [Validation](validation.md) | `validate/`, `qualify/` and every named limit. |
