# Filesystem trees

[Architecture](../../README.md) › [Core](../README.md) › Filesystem trees

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Directories as canonical objects: entries, attributes, symlinks, hard links, ordering and validation.

| | |
| --- | --- |
| Layer | [Core](../README.md) |
| Crates | `layerfs-content` |
| Source | [`core/crates/layerfs-content/src/filesystem`](../../../../core/crates/layerfs-content/src/filesystem) |
| Uses | [Objects](../objects/README.md), [Files](../files/README.md) |
| Used by | [Compare and merge](../compare-and-merge/README.md), [Engine: Project Init](../../engine/project-init/README.md), [Engine: Workspace](../../engine/workspace/README.md) |
| Flows | [Project Init](../../flows/init.md), [A Commit](../../flows/commit.md) |

## Topics

| Topic | What it covers |
| --- | --- |
| [Representation](representation.md) | Directories, inodes and rows in canonical order. |
| [Attributes and links](attributes-and-links.md) | Kept attributes, symlinks, hard links and references. |
| [Updates](updates.md) | Updating an existing tree. |
| [Validation](validation.md) | Validation, qualification and limits of a tree. |

## The picture

```text
 caller (Project Init, Workspace Commit, merge)
   |  sorted final bindings + typed inode values + new serials
   v
 +------------------------------------------------------------+
 | Updates      validate --> directories --> references -->   |
 |              inodes --> root.encode         (update.rs)    |
 |     |  checks          |  pages            |  counts       |
 |     v                  v                   v               |
 | Validation       Representation     Attributes and links   |
 | (topology,       (directory and     (aliases, derived      |
 |  cycles, limits)  inode B+trees)     counts, attributes,   |
 |                                       symlink targets)     |
 +------------------------------------------------------------+
   |  FinalizedObject (pages, root)     ^  read_canonical(id)
   v                                    |
 caller's FinalizedConsumer        caller's AuthenticatedObjects
```

A filesystem operation never opens a store. It reads canonical bytes from the
caller's authenticated provider and hands every finished page to the caller's
consumer through one [`FilesystemObjects`](../../../../core/crates/layerfs-content/src/filesystem/objects.rs) value.

## What it does

```text
 FilesystemRoot (LFS6FSR) ---> inode table: B+tree keyed by serial (LFS6INT)
   profile, scope,               leaf row: serial -> 73-byte InodeValue
   root serial                       |
                                     | content_root, by kind
           +-------------------------+-------------------------+
           v                         v                         v
   Directory: B+tree of      RegularFile: file root     Symlink: target
   name -> serial (LFS6NSP)  (see Files)                object (LFS4LNK)
```

| Question | Answer | Source |
| --- | --- | --- |
| What is stored | One root object, one inode table per root, one directory B+tree per directory inode, one symlink target object per symlink, and file roots from [Files](../files/README.md). | [`root.rs`](../../../../core/crates/layerfs-content/src/filesystem/root.rs), [`sorted/format.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/format.rs) |
| Which inode kinds exist | `RegularFile`, `Directory` and `Symlink`; any other kind code is refused on decode. | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| What filters the namespace | Nothing. The tree stores whatever bindings the caller supplies, including `.git`, ignored files and dependencies. Git ignore rules are not read anywhere in this module. | [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs) |
| How a hard link is stored | Two names bind the same inode serial; the operation derives `namespace_ref_count` from retained bindings. | [`references/reduce.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/reduce.rs) |
| Which attributes are kept | Portable `mode` (permission bits, plus sticky for directories) and `mtime` (seconds and nanoseconds). No uid, gid, atime, ctime or extended attributes are kept by any production writer. | [`attributes/portable.rs`](../../../../core/crates/layerfs-content/src/filesystem/attributes/portable.rs) |
| How a tree changes | Callers pass final bindings and values; a sorted B+tree merge rewrites only the pages a change reaches. | [`update.rs`](../../../../core/crates/layerfs-content/src/filesystem/update.rs), [`sorted/merge.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/merge.rs) |

Terms such as *inode*, *serial* and *scope* are defined in the
[glossary](../../glossary.md) and in [Representation](representation.md).

## Invariants

| Rule | Where it is enforced |
| --- | --- |
| A tree is complete: no ignore rule or path filter exists in construction. | [`update.rs`](../../../../core/crates/layerfs-content/src/filesystem/update.rs) applies every supplied row; the native scan is [Project Init](../../engine/project-init/README.md)'s |
| Rows in every page are strictly ascending by key (name bytes, serial or attribute key); a duplicate or unsorted row is refused, not normalized. | `reject_unordered` and the encoders in [`sorted/format.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/format.rs); [`DirectoryUpdate::check`](../../../../core/crates/layerfs-content/src/filesystem/input.rs) |
| Only the frozen profile `scoped-inline/v1` is accepted; any other profile fails before mutation. | [`FilesystemRoot::decode`](../../../../core/crates/layerfs-content/src/filesystem/root.rs) |
| Serial 0 and serials above `i64::MAX` are refused; Content never allocates serials. | [`InodeIdentity::new`](../../../../core/crates/layerfs-content/src/filesystem/identity.rs) |
| The root inode is a directory with count 0; every other directory and symlink has count exactly 1; a regular file has count at least 1. | [`InodeValue::validate`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| Every final count is derived from retained bindings, never trusted from the caller. | [`references/reduce.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/reduce.rs), [`references/record.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/record.rs) |
| The result has no directory cycle and no unreachable new directory. | [`validate/cycles.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate/cycles.rs) |
| An unchanged subtree keeps its identity and is not read. | [`sorted/page.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/page.rs), [`sorted/merge.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/merge.rs) |
| A failed operation emits no root object and releases its ordering backing once. | `run_state` in [`update.rs`](../../../../core/crates/layerfs-content/src/filesystem/update.rs) |

Equal input rows give equal page bytes and therefore equal IDs. Identity also
depends on the scope and on the serials the caller allocated, so two trees with
the same names but different serials have different roots. Whether two different
update histories that reach the same final rows always produce the same page
partition is not established from source.

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Name component | 1 to 255 bytes, UTF-8, no NUL, `/` or `\`, not `.` or `..`; else `InvalidPath`, `InvalidUtf8` or `PathLimitExceeded` | [`path.rs`](../../../../core/crates/layerfs-content/src/filesystem/path.rs) |
| Path | at most 4,096 bytes and 256 components | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Tree page | at most 8,192 canonical bytes; level at most 31 | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Symlink target | at most 4,096 bytes, no NUL | [`symlink.rs`](../../../../core/crates/layerfs-content/src/filesystem/symlink.rs) |
| Inode serial | 1 to `i64::MAX` | [`identity.rs`](../../../../core/crates/layerfs-content/src/filesystem/identity.rs) |
| Operation scratch for unfinished pages | 4 MiB (`MAXIMUM_OPERATION_SCRATCH_BYTES`); a reservation that does not fit fails | [`sorted/budget.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/budget.rs) |
| Resident input rows without a backing | refused with `ObjectLimitExceeded` above `ordering_bytes / 16` rows | [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs), [`validate.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate.rs) |
| Foreign profile or scope | `UnsupportedProfile`, `ScopeMismatch` | [`root.rs`](../../../../core/crates/layerfs-content/src/filesystem/root.rs), [`validate.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate.rs) |
| Malformed page | `UnsupportedFraming`, `TrailingBytes`, `LengthMismatch`, `NonCanonicalOrdering`, `InvalidRecord` | [`sorted/format.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/format.rs) |

Huge directories are B+trees of bounded pages, not resident lists. Listing is
paged by entry count and bytes and resumes after the last delivered name
([`directory/read.rs`](../../../../core/crates/layerfs-content/src/filesystem/directory/read.rs)). Huge updates stream their rows
through replayable cursors ([`rows/mod.rs`](../../../../core/crates/layerfs-content/src/filesystem/rows/mod.rs)) and keep reference
state in caller-owned records or spilled runs ([`references/runs.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/runs.rs)).
All limits are in [Validation](validation.md#limits).

## Demonstration

All four run on any platform with no Docker, root, feature or environment
variable. The commands are written from source and were not run while writing
this page.

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`filesystem_codec.rs`](../../../../core/crates/layerfs-content/tests/filesystem_codec.rs) | `layerfs-content` | Every tree grammar (inode, directory, attribute pages, root, symlink) encodes to sealed golden bytes and refuses malformed variants. |
| [`filesystem_hardlinks.rs`](../../../../core/crates/layerfs-content/tests/filesystem_hardlinks.rs) | `layerfs-content` | Aliases in several directories reach one derived count; moves never drop a count to zero. |
| [`filesystem_ordering.rs`](../../../../core/crates/layerfs-content/tests/filesystem_ordering.rs) | `layerfs-content` | The reference reducer's ordering rows, spills, tiers and checked cleanup. |
| [`filesystem_primitives_candidate.rs`](../../../../core/crates/layerfs-content/examples/filesystem_primitives_candidate.rs) | `layerfs-content` | The sorted directory merge, inode merge and root encoding over a generated tree, printing identities and page partition. |

**`filesystem_codec`** builds pages from fixed inputs, compares them with the
fixtures under `tests/fixtures/filesystem/`, and checks that wrong roles, flags,
trailing bytes and an over-full directory leaf are refused.

    cargo +1.85.1 test --locked -p layerfs-content --test filesystem_codec

**`filesystem_hardlinks`** builds a small tree and then updates it:

```text
 /            a/x --+                 a/x --+
 +- a/            +--> file (count 2)       +--> file (count 3)
 +- b/  b/y ------+       add c/z -->  b/y -+
                                       c/z -+
```

It checks the count after adding, moving and removing aliases, that a regular
file keeps at least one binding, and that a declared new inode cannot lose one.

    cargo +1.85.1 test --locked -p layerfs-content --test filesystem_hardlinks

**`filesystem_ordering`** drives `build_filesystem` and `update_filesystem`
with a recording backing under the system temporary directory. It checks the
96-byte record grammar, that crossing the pending ceiling only moves rows into
runs, that growth is reserved before it happens and that every backing failure
fails the operation without a root.

    cargo +1.85.1 test --locked -p layerfs-content --test filesystem_ordering

**`filesystem_primitives_candidate`** takes `--files` (default 2,000),
`--changes` (default 200) and `--samples` (default 1) and prints the base and
updated identities, the page partition and timings. Its timings are diagnostic
only.

    cargo +1.85.1 run --locked -p layerfs-content --example filesystem_primitives_candidate -- --files 2000 --changes 200

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`core/docs/architecture/04-filesystem.md`](../../../../core/docs/architecture/04-filesystem.md) — covered in [Representation](representation.md), [Updates](updates.md) and [Validation](validation.md).
- [`core/docs/architecture/12-attributes.md`](../../../../core/docs/architecture/12-attributes.md) — covered in [Attributes and links](attributes-and-links.md#attributes).
- [`core/docs/architecture/30-namespace-operations.md`](../../../../core/docs/architecture/30-namespace-operations.md) — historical only; Workspace operations belong to [Engine: Workspace](../../engine/workspace/README.md).
- [`core/docs/architecture/39-native-import-links.md`](../../../../core/docs/architecture/39-native-import-links.md) — the target grammar is covered in [Attributes and links](attributes-and-links.md#symlinks); native acquisition belongs to [Engine: Project Init](../../engine/project-init/README.md).
- [`core/docs/architecture/41-native-regular-aliases.md`](../../../../core/docs/architecture/41-native-regular-aliases.md) — the stored form is covered in [Attributes and links](attributes-and-links.md#hard-links); native alias grouping belongs to [Engine: Project Init](../../engine/project-init/README.md).

---

← [Files: Reads and views](../files/reads-and-views.md) · [Up: Core](../README.md) · [Filesystem trees: Representation](representation.md) →
