# Pre-registration — #219 round 13: memoise absence in validation

Written **before** the first run of this arm and before any edit. This is a **product** change in
`layerfs-content/src/filesystem/validate.rs`.

Control: **M1** `ns19-M1-sites-20260921T083052Z` (`fff509f3d`): `build_validate_ns` 228.42 ms,
`validation_inode_pages_read` **27,662**, `validation_read_waves` 27,657,
`validation_pages_bindings` **13,718** (49.6 %), `validation_pages_cycles` **13,718** (49.6 %),
`validation_pages_prefetch` **10**, `validation_pages_allocation` 216, `inode_demands` = `objects_read`
**17,777**, `operation_work_ns` **1681.30 ms**, CPU 1683.80, 13/13 gates, 14/14 pins.

## The one difference

**A serial that validation has shown to be absent from the base is remembered as absent**, in the
same per-operation state that already remembers the records it finds. `ValidationState` gains an
`absent` set; `prefetch` records absence for every demanded serial `lookup_many` answered `None` for,
and `lookup_optional` answers from it.

Today absence is **deliberately not memoised**, and the state's own documentation says why: *"so an
absent serial's accounting is bit-identical."* The reason is accounting, not correctness. This row's
batches bind serials they are *allocating*, so those serials are absent from the base by
construction, and round 12 measured what that costs: **27,436 of `validate`'s 27,662 reads are
absent-serial descents, and half of them are the same descent bought twice** — once by the binding
loop and once by the cycle walk, both asking the identical serials, with the grouped prefetch
answering almost nothing (10 pages).

**The demand charge is preserved.** An absent memo hit charges one demand exactly as today's descent
charges one (`lookup_many` charges `work.demands` per serial answered at a leaf, `inode/read.rs:120`),
so `inode_demands` and `objects_read` must come out **identical**; only the physical pages and waves
may fall. That is the work-neutrality this round is registered on, and it is what keeps every pinned
counter and every other row's published validation figures where they are.

**Why absence is stable here, stated as the correctness argument.** The memo lives for one `check`
call, and the base it reads is immutable for that call: `FilesystemTopology::load` binds one base
root and every site asks the same `InodeTable`. A serial absent once is absent throughout, so
remembering it cannot change a verdict — and a verdict is all this phase produces.

## Prediction, in the instruments' own units

| instrument | now | predicted | derivation |
| --- | ---: | ---: | --- |
| `validation_pages_bindings` | 13,718 | **0** | the prefetch already read every child and can now record absence |
| `validation_pages_cycles` | 13,718 | **0** | the walk asks the same serials |
| `validation_pages_prefetch` | 10 | **≤ 250** | it must read the leaves for the absent serials once, grouped |
| `validation_inode_pages_read` | 27,662 | **216–460** | allocation 216 + one grouped prefetch |
| `validation_read_waves` | 27,657 | **≤ 60** | one wave per level per grouped demand |
| `inode_demands` = `objects_read` | 17,777 | **17,777 unchanged** | charges preserved |
| `build_validate_ns` | 228.42 ms | **≤ 45 ms** | the reads collapse; the loop's own CPU remains |
| `operation_work_ns` | 1681.30 ms | **≤ 1520 ms** | −160 to −200 ms |

## What would refute it

1. `validation_inode_pages_read` >= 2,000: the descents are not absent-serial lookups and the round-12
   reading is wrong. Reported as a refutation, not repaired by re-running.
2. `inode_demands` or `objects_read` moves off **17,777**: the treatment changed the accounting rather
   than only the reads, which is the thing it is registered not to do.
3. Any of the 14 pinned counters moves, `pipeline.commits` != 284, the root digest changes, or the row
   is not PASS 13/13. `fs_build.validation_entries` is pinned at 4,096 for four other rows and this
   treatment must not touch `entries_examined`.
4. `build_validate_ns` >= 180 ms, or `operation_work_ns` >= 1681 ms: the mechanism is not what round
   12 measured.
5. Any product test fails: `cargo test -p layerfs-content`, `-p layerfs-storage` and the whole core
   workspace must be green.

## What is not claimed

Nothing about v0.1.6 and nothing about the store. This is one phase of the row's C1 half; if it pays
as predicted the row's work figure lands near 1.48 s, and the store-side treatments
(incompressible-payload frames, whole-file lane grouping) remain unstarted.
