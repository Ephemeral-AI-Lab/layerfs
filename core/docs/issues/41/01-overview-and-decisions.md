# Overview & Design Decisions

## Issue #41: LayerFS Garbage Collection

Deliver explicit LayerFS garbage collection for SQLite and PostgreSQL through:
- Soft Branch removal (`branch.remove()`)
- Soft project deletion (`project.delete()`)
- Logical historical file/folder removal (`project.remove_file()`)
- Explicit Store compaction (`store.compact()`)

## Committed Design Decisions (D1-D11)

1. **D1**: Store-level GC granularity
2. **D2**: Logical-first approach (no physical reclamation except compact)
3. **D4**: Exclusive access during store.compact()
4. **D5**: Cross-project safety guarantees
5. **D8**: Writer-exclusive fence protocol (SQLite)
6. **D9**: Name reuse with timestamp (removed_ms/deleted_ms)
7. **D10**: Immutable ID protection after file removal
8. **D11**: PostgreSQL remote compaction required
9. **D12**: No automatic collection in this release
10. **D13**: No branch pruning in v2.0
11. **D14**: Reference counting strategy

## Pending Decisions (6)

- [ ] D3: PostgreSQL fence strategy selection
- [ ] D6: Garbage collection algorithm details
- [ ] D7: Cross-project Pack protection strategy
- [ ] (+ 3 additional)

**Authority**: Ephemeral-AI-Lab/layerfs#511 (Release Contract)

See full details in:
- README.md - Complete specification
- C8b-N3-C9-Resolution-Plan.md - Critical issues
- RESOLUTION-SUMMARY.md - Key decisions
