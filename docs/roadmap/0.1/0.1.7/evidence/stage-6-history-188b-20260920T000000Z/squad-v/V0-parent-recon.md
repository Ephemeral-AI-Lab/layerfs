# V0 — parent reconnaissance (before the squads launched)

**Diagnostic.** Source pin @66bce8378@ + the uncommitted working tree. No timing.

## 1. The remaining gap decomposes exactly, residual 0

T1 is blocked at **57,749,504 B apparent = 1.1710x v0.1.6**, residual **+8,433,664 B**. Decomposed from
the T1 receipt's own @dbstat@ and lane figures:

| term | ours | v0.1.6 | gap | share |
| --- | --: | --: | --: | --: |
| whole-file lane | 43,873,480 | 38,983,278 | **+4,890,202** | **58.0 %** |
| native (chunk) lane | 5,695,678 | 3,958,057 | **+1,737,621** | **20.6 %** |
| ordinary lane | 1,022,795 | 678,339 | +344,456 | 4.1 % |
| pooled-metadata lane | 2,019,419 | 2,240,958 | **-221,539** | -2.6 % (we win) |
| pack framing | — | — | +118,252 | 1.4 % |
| **pack total** | 53,374,976 | 46,505,984 | **+6,868,992** | **81.4 %** |
| objects table (non-pack) | 4,124,672 | 2,650,112 | **+1,474,560** | **17.5 %** |
| other non-pack | — | — | +90,112 | 1.1 % |
| **total** | 57,749,504 | 49,315,840 | **+8,433,664** | **100 %** |
| | | | **residual 0** | |

**The re-ranking that matters:** the native lane is **20.6 %** of what remains. That is exactly Squad
A1's measured "v0.1.6 had 650 chunk PREFIX records worth 1,737,621 B" — i.e. **the whole native gap is
chunk delta coverage**, which is **#185's cursor half**. Squad E3 ruled **NO** on #185 when it was
**2.78 %** of a 79,548,416 B gap; on the current numbers it is **20.6 %** of 8,433,664 B. **The deferral
decision was made on different numbers and must be revisited** (V2).

## 2. Negative result — v0.1.6's two delta tags are NOT the gap

v0.1.6's whole-file lane decodes as three kinds; ours as two:

| store | FULL (tag 0) | DELTA tag 1 | DELTA tag 2 |
| --- | --: | --: | --: |
| **v0.1.6** | 8,237 | 4,796 | **31,108** |
| registered lane | 25,804 | 18,344 | **0** |
| T1 best (@/tmp/confirm@) | 9,702 | 34,439 | **0** |
| T1 @/tmp/r5@ (four-slot, worse) | 5,912 | 38,229 | **0** |

**Every one of our arms emits tag 1 only.** Core defines just @FULL_TAG = 0@ and @PREFIX_TAG = 1@
(@encoding/delta/record.rs:15,17@).

**Ruled out as the cause.** v0.1.6's reader treats 1 and 2 identically — @compact_record_parts@ maps
@0 => start 1@ and @1 | 2 => start 33@, i.e. both mean "carries a 32-byte base oid"
(@crates/layerfs-layerstack-store/src/objects/delta.rs:20-28@). The tag records **where the base came
from**, not a different frame format. So our tag-1 records include both same-save and cross-save bases;
core simply does not distinguish them in the tag. **Not the gap.**

**What it does establish, and what V1 must explain:** v0.1.6 has **35,904 deltas** (31,108 of them,
87 %, against an earlier-commit base, measured by Squad A4 at **20.387x**); our best arm has **34,439**
deltas and stores **4,890,202 B more** in the same lane over the **same canonical content**
(44,141 objects / 348,460,295 B). **We delta a comparable number of objects and store more bytes —
so our deltas are worse deltas.** The open question is whether that is base *distance*, base *quality*,
or frame size for the same relationship.

## 3. Negative result — the codec parameters are identical at the frame level

Decoded the zstd frame headers of all 44,141 (v0.1.6) and 44,148 (ours) whole-file frames:

@```
  v0.1.6:  single_seg = 1,  checksum = 1,  dict_id = 0   (44,141 / 44,141)
  ours:    single_seg = 1,  checksum = 1,  dict_id = 0   (44,148 / 44,148)
```

**All frames are single-segment**, so no window descriptor is present and the window equals the frame
content size. **windowLog 18 is therefore not a constraint on this lane at all** — every whole-file
object is below the 128 KiB cutoff, so its content is smaller than 2^18 = 256 KiB and the window covers
it either way. This confirms Squad A1's source read from the bytes and closes the "bigger window"
hypothesis for the whole-file lane.

## 4. The uncommitted product change — reviewed, sound

Four files, all small and well-commented; production LOC +7. R3's @cached_edge@ fix matches Squad E4's
described minimal fix exactly (1 on the cache-hit break, 0 on the chain-root break). R1a raises
@INDEX_BYTES@ 128 KiB -> 4 MiB with @SLOTS@ 1024 -> 32,768, and the @SLOTS < NO_ENTRY (65,535)@
assertion still holds at the @u16@ reference limit. R2 removes two indexes and sets
@REQUIRED_INDEXES@ empty rather than bumping @SCHEMA_VERSION@ — the requirement is a floor, so an
existing Store still validates. **No format break.**

## 5. What the squads were told to settle

- **V1** — the whole-file lane's 4,890,202 B (58.0 %): base distance vs base quality vs frame size,
  per object against v0.1.6's Store, including whether "v0.1.6's base is the nearest version" is true.
- **V2** — the native lane's 1,737,621 B (20.6 %) and whether **#185's cursor half** should be
  un-deferred on the new numbers.
- **V3** — T2, T3, the ordinary lane (344,456 B), the objects table (1,474,560 B), and anything else,
  costed for bytes / memory / access / LOC, ranked by bytes-per-unit-of-pain.
- **V4** — synthesis: is the gate reachable at all, by what ordered path, and if not, what should the
  owner rule instead.
