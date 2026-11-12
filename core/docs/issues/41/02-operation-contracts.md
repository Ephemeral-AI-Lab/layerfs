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

See § 5.1 in LayerFS-GC-spec.md

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

See § 5.2 in LayerFS-GC-spec.md

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

See § 8 in LayerFS-GC-spec.md

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

See § 7 in LayerFS-GC-spec.md
