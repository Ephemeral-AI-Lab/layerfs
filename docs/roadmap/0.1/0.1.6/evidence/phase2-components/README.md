# Phase 2 component checks — implementation in progress

These are development component results, not a completed phase, supported FUSE
snapshot acceptance, or a sealed benchmark candidate. No benchmark/#122 scenario
was executed. The source manifest records the staged source which was tested;
subsequent ownership integration is a separate change and invalidates only its
affected checks. Native toolchain: rustc 1.96.0 / Cargo 1.96.0, macOS host.

| Check | Evidence | Result |
| --- | --- | --- |
| Owned piece cursor, path-bounded memory, mutation after acquisition, compact forms | `cursor-attempt02.log` | 1 PASS |
| Atomic leased-root install, disjoint stale writer, captured changes, retained root, tombstone cleanup, failed preparation, replay | `overlay-attempt02.log` | 4 new overlay tests PASS |
| Private disk COW index: splits/merges/range seeks, overflow values, retained roots, exact quota, torn write, root admission, foreign ownership, reclamation | `overlay-attempt02.log` | 3 new index tests PASS |
| Existing failed spool operation preserves overlay | `overlay-attempt02.log` | 1 existing regression PASS |
| Existing compact wire round trips and sequence bound after cursor substitution | `wire-cursor-attempt02.log` | 3 existing regressions PASS |
| Existing interleaved spool/truncate/open-unlinked/retained-reader behavior | `cursor-caller-attempt01.log` | 1 existing regression PASS |

Retained unsuccessful attempts: `cursor-attempt01.log` and
`wire-cursor-attempt01.log` selected zero tests because their filters did not match
the module-qualified names. They are **INVALID_EVIDENCE**, despite Cargo returning
zero. Corrected selectors executed the counts above. `overlay-attempt01.log`
failed compilation because an index header-decoder closure inferred an `i32`
index; explicitly typing `usize` fixed it, and attempt02 executed 8 tests.

Commands:

```sh
cargo test -p layerfs-workspace-core file_edit::tests::piece_cursor_retains_old_ranges_with_only_a_tree_path_resident -- --exact --nocapture
cargo test -p layerfs-workspace --lib overlay -- --nocapture
cargo test -p layerfs-fuse --lib live_wire::compact_pending_tests -- --nocapture
target/debug/deps/layerfs_workspace-7a70d4db173d6a6f file_io::tests::shared_segments_keep_interleaved_rollback_reads_and_open_unlinked_lifetime --exact --nocapture
```

The last invocation reused the already-built test binary from overlay-attempt02,
SHA-256 `e2834f14f456fc8d626b96038cc89e201108fc4de07a28639ed7d7debc690c08`.
It exercised the changed `PreparedFileEdit::backing_ranges` caller without
recompiling or testing another in-progress component. No passing check was rerun
only because repository HEAD changed.

Current implementation: one physical immutable B+tree supplies distinct typed
inode/binding and dual change indexes through key domains; a constant-sized root
bundle is swapped only against its exact leased source. Binding masks survive
covered-tracking cleanup. Prepared roots remain private until the whole closure
succeeds. Replay reserves fixed request/result capacity before installation,
preserves unresolved results, rejects stale requests and never treats reconnect
as authorization to reapply. Existing core/wire paths now traverse owned piece
cursors instead of building a full piece vector.

Remaining: connect real fixed-size inode/range records and payload ownership to
the root graph; host-authoritative ordinary-operation integration; fair bounded
root retry/admission; complete snapshot reader; V1; provenance/C2; independent
attempt lifecycle and consumer cleanup. The initial index reports interior freed
slots as reusable but still physically charged; only complete empty-arena
truncation is currently a physical-release proof. No full phase completion is
claimed and no existing Commit freeze has yet been removed.
