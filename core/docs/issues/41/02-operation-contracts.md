# Operation Contracts

## branch.remove(name)

**Purpose**: Soft-remove a Branch from visibility

**Contract**:
- Changes visibility only (logical removal)
- No physical data reclamation
- Other Branches in same project remain usable
- Existing Commits remain accessible via immutable IDs
- Once removed, cannot create new mounts

**Admission**: No mounts on the selected Branch

**Error Codes**:
- `MountsActive` - Active mounts exist on Branch
- `Busy` - Conflicting operations in progress

See `LayerFS-GC-spec.md` §5 C6 (terminal tombstone, name reuse) and §5 C10
(branch-level exclusion); the call surface is §8, and §3 I5 keeps history and ids
intact.

## project.delete(name)

**Purpose**: Soft-delete a project

**Contract**:
- Changes visibility only (logical deletion)
- Store remains addressable for later compaction
- No physical data reclamation
- Other projects in same Store remain usable
- Existing files remain accessible via immutable IDs until Store.compact()

**Admission**: All Sandboxes/clients in target project must close

**Error Codes**:
- `MountsActive` - Active mounts exist in project
- `Busy` - Conflicting operations in progress

See `LayerFS-GC-spec.md` §5 C6 (shared and exclusive tombstones), §5 C10
(project-level exclusion) and §8 (the Store stays addressable).

## project.remove_file(...)

**Purpose**: Logically remove a file/folder across project history

**Contract**:
- Changes visibility only (logical removal)
- No physical data reclamation
- File becomes inaccessible via normal paths
- File remains accessible via immutable IDs (with visibility=Removed)
- History entries preserved with removal markers
- Cross-project sharing unaffected (other projects' Packs protected)

**Admission**: All Sandboxes/clients in target project must close

**Error Codes**:
- `MountsActive` - Active mounts exist in project
- `Busy` - Conflicting operations in progress

See `LayerFS-GC-spec.md` §3 I5 (visibility only, ids unchanged), §5 C9 (the
catalog methods that keep an immutable id readable) and §9.3 (byte accounting).

## store.compact(name)

**Purpose**: Physically reclaim discarded storage space

**Contract**:
- Physically deletes logically-removed data
- Requires exclusive access to entire Store
- No concurrent operations allowed
- Protects all shared content and dependencies
- Reports physical bytes reclaimed separately from logical deletion
- Works on SQLite and PostgreSQL (including remote servers)

**Admission**: Exclusive access - no mounts or operations on any project in Store

**Error Codes**:
- `MountsActive` - Any mounts exist in Store
- `Busy` - Other operations in progress

**Backend Support**:
- SQLite: Full support with 5-stage incarnation protocol
- PostgreSQL: Full support, including remote servers (not stub)

See `LayerFS-GC-spec.md` §7 (Sweep), §7.2 for SQLite, §7.3 for PostgreSQL, and §8
for the operation surface.
