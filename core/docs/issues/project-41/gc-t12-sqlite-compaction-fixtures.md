# T12.1: SQLite compaction fixtures and the golden receipt

> Acceptance fixture for the frozen [`LayerFS-GC-spec.md`](LayerFS-GC-spec.md)
> §11 (report), §12 (acceptance) and §6.4 (live-heads digest). It adds no
> contract: the spec and the matrix win; `§N` and `AC-nn` cite them.

## What the fixture is

`engine/crates/layerfs-persistence/tests/sqlite_compact_fixtures.rs` builds one
Store through the public APIs only (`Fixture::put`/`record`/`inode`, the
`HistoryCatalog` methods, `SqliteHandles::compact`). It is the first fixture
that exercises the §7 rebuild with every §11 report component non-trivial:

| Fixture part | What it pins |
|---|---|
| Two Stacks (one live, one `mark_stack_deleted`) | retained history and a project collected whole |
| A published Branch retired but still named by its retained Layers | `branch_retired` marker survives; name stays taken |
| An unmerged Branch retired and named by nothing | retired Branch row and marker collected |
| Commits with known `statistics.logical` (120/240/360/70/40) | `logicalDiscarded` is the exact history `logical` delta (160) |
| 12 unreferenced whole files, one per pack | wholly dead packs and `deadBytesBound` |
| A depth-2 PREFIX chain `base2 <- base1 <- target` with only `target` a retained root | `keptWaste.deltaBases` references both bases; pack ids survive (AC-17) |
| One retained pack with a reached group and an unreached group | `keptWaste.deadInRetainedPacksBytes`; a 2.0 rebuild keeps the group |
| A pooled inode leaf inside every tree | `keptWaste.inodeBlocksBytes` (all `inode_value_block` rows are kept) |
| Two `file_digest` rows, one root retained and one collected | AC-18 |
| A retired Branch with no head and a fixed-id digest Store | §6.4 golden vector (AC-56, SQLite half) |

The transcript covers three outcomes:

- `emptyNone`: an empty Store is `none` with `zeroReason: empty-store`, no
  generation change, no artifact.
- `noDeadNone`: a Store whose every pack holds a live object is `none` with
  `zeroReason: no-waste`; `deadBytesRefined == deadBytesBound` and the digest
  and generation are unchanged.
- `rebuild`: `G -> G+1`; live objects (including the delta target through its
  chain) read, dead objects and packs disappear, the retained pack keeps its
  unreached group, collected history rows and their markers disappear, a
  retained `branch_retired` marker stays, the full doctor passes and the
  `live_heads_digest` is unchanged.

## Defect found by the fixture

The rebuild report's `residue.bytes` counted the bytes of every artifact the
settle *removed*, not only the ones that remain, so a fully successful rebuild
reported a nonzero residue with `artifacts: []` and no `reason` — contrary to
§11 ("a nonzero residue is explained") and the T8 note ("residue reports
anything that could not be removed"). `clear_artifacts` now adds a file's size
only when its removal failed; the fixture asserts `residue.bytes == 0` and the
golden pins it. `sqlite_compact` still passes.

## Golden transcript

`engine/crates/layerfs-persistence/tests/fixtures/sqlite_compact_golden.json`
is the byte-for-byte golden (`Json::render`, every object sorted by key). It
contains the three receipts, the mark tuple (digest, object count, packs), the
history markers and the fixture identities. A change to any value fails the
test; update it deliberately:

```
UPDATE_GOLDEN=1 cargo test -p layerfs-persistence --test sqlite_compact_fixtures
cargo test -p layerfs-persistence --test sqlite_compact_fixtures
```

The second run recompiles the embedded golden and checks it.

## Matrix rows covered (SQLite half)

| Case | Evidence |
|---|---|
| AC-04 | deterministic fixture: no live object reaches a collected row; closure compared before/after |
| AC-05 | full doctor (includes `diagnostic/scan.rs`) on the compacted Store |
| AC-17 | depth-2 chain copied; `pack_id` preserved on the bases and the target |
| AC-18 | collected digest row gone, retained one kept |
| AC-25 | retained `branch_retired` survives; collected `branch_retired`/`layer_stack_retired` go |
| AC-38 | the successful artifact set is removed and no stray name remains |
| AC-40 | SQLite `live_heads_digest` is identical before and after; a retired head moves it |
| AC-47 | `none` leaves no `.compact`; a successful settle leaves no artifact |
| AC-48 | empty Store (0/0) is `none` with `zeroReason`; the fixture is a `rebuild` arm |
| AC-52 | every §11 field on both paths, with units and sampling point, pinned by the golden |
| AC-56 | fixed-id digest vector with out-of-order ids, a retired Branch and stack, and an absent head (SQLite half) |
| AC-58 | `none` leaves no `.compact`/`.compact-visited`; `rebuild` settles to no artifact |

Not covered here, by design (T12.2/T12.3): crash failpoints and `kill -9`
(AC-01, AC-03, AC-33, AC-46, AC-47 crash arm, AC-58 mid-run arm, AC-59), the
9%/10%/30% boundary arms and property tests (AC-04 at E3, AC-48 boundary,
AC-57), the PostgreSQL halves (AC-02, AC-28..AC-44, AC-60), admission races
(AC-07, AC-08, AC-49), and the report/`store.status` op projection (AC-51).
The PostgreSQL digest agreement (AC-40) and the shared literal vector (AC-56)
are proven on SQLite only here.
