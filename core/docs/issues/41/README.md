# Issue #41: LayerFS Garbage Collection - Soft Removal & Store Compaction

**Status**: Design phase, implementation pending  
**Release Target**: LayerFS 2.0.0  
**Release Contract**: Ephemeral-AI-Lab/layerfs#511  
**Project Tracking**: Ephemeral-AI-Lab/layerfs-project#41

## Overview

Deliver explicit LayerFS garbage collection for SQLite and PostgreSQL through soft removal and explicit compaction:

- **`branch.remove(name)`** - Soft-remove a Branch
- **`project.delete(name)`** - Soft-delete a project  
- **`project.remove_file(...)`** - Logically remove a file/folder across history
- **`store.compact(name)`** - Physically reclaim discarded storage

## Core Design Principles

1. **Logical-First**: All remove/delete are logical; only `store.compact()` performs physical reclamation
2. **Exclusive Compaction**: `store.compact()` requires full Store isolation
3. **Cross-Project Safety**: All shared content and delta bases remain protected
4. **Immutable ID Protection**: Existing immutable IDs remain safe after file removal
5. **PostgreSQL Remote**: Full compaction support required (not a stub)
6. **Writer Fence**: SQLite incarnation uses atomic swap with writer exclusion

## Key Constraints

| Constraint | Description | Impact |
|-----------|-------------|--------|
| **C1** | Reachability = Store | GC scope boundary |
| **C2** | Pinned object protection | Cross-project safety |
| **C3** | Historical mount inclusion | Closure completeness |
| **C8b** | Incarnation protocol | SQLite atomicity |
| **C9** | HistoryCatalog binding | Immutable ID contract |
| **C10** | Admission matrix | Mount validation |

## Files in This Directory

- **README.md** - This file
- **01-overview-and-decisions.md** - Design decisions (D1-D14)
- **02-operation-contracts.md** - Operation specifications
- **C8b-N3-C9-Resolution-Plan.md** - Deep dive on three critical issues
- **RESOLUTION-SUMMARY.md** - Quick reference for key decisions
- **Full spec**: See pull request tokenaissance/layerfs#70

## Implementation Phases

### Phase 1: Design Finalization (#42)
- Confirm all 11 committed decisions
- Product sign-off on 6 pending decisions

### Phase 2: Infrastructure (#43, #48)
- Admission enforcement
- Shared data reachability

### Phase 3: Soft Removal (#44, #45, #47)
- Branch removal
- Project deletion  
- File removal

### Phase 4: Physical Compaction (#49, #50)
- SQLite compaction
- PostgreSQL compaction

### Phase 5: Integration (#51, #52)
- API/SDK/CLI exposure
- Comprehensive testing

## Admission & Mount Policy

| Operation | No mounts within | Other restrictions | What may remain |
|-----------|------------------|-------------------|-----------------|
| `branch.remove(name)` | Selected Branch | Existing rules | Other Branches |
| `project.delete(name)` | Project + Branches | Close Sandboxes | Other projects |
| `project.remove_file(...)` | Project + Branches | Close Sandboxes | Other projects |
| `store.compact(name)` | Store + all projects | Exclusive access | Other Stores |

## Related Resources

- **Specification PR**: tokenaissance/layerfs#70
- **Release Contract**: Ephemeral-AI-Lab/layerfs#511
- **Architecture Diagrams**: https://app.excalidraw.com/s/9Zq6XgRe98z/
- **Sub-Issues**: #42-#52

## References

- See § 1-9 in complete specification (PR #70)
- Three critical issues (C8b, N3, C9) explained in C8b-N3-C9-Resolution-Plan.md
- Design decisions summary in RESOLUTION-SUMMARY.md

---

**Last Updated**: 2026-11-12
