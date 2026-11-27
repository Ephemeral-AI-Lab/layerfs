# Encoding

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How one canonical object becomes a stored record and back: codec frames,
FULL, STORED and PREFIX selection, chain decoding, pooled inode leaves,
program pages and the pack cache, within [Physical storage](README.md).

| | |
| --- | --- |
| Source | [`encoding/`](../../../../core/crates/layerfs-storage/src/encoding): [`codec.rs`](../../../../core/crates/layerfs-storage/src/encoding/codec.rs), [`full.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/full.rs), [`decode.rs`](../../../../core/crates/layerfs-storage/src/encoding/decode.rs), [`delta/`](../../../../core/crates/layerfs-storage/src/encoding/delta), [`pool/`](../../../../core/crates/layerfs-storage/src/encoding/pool), [`cache.rs`](../../../../core/crates/layerfs-storage/src/pack/cache.rs), [`units.rs`](../../../../core/crates/layerfs-storage/src/pack/units.rs) |

## codec.rs and full.rs

```text
 caller-owned aligned region (zeroed once)
 +----------------------------------------------+
 | static ZSTD_CCtx  (encode, 16 MiB region)    |
 | static ZSTD_DCtx  (decode, 1 MiB region)     |
 +----------------------------------------------+
   no allocator-backed context; a call that does not fit fails

 canonical object --role--> lane
   ordinary roles --> FULL [0][canonical bytes]       (program pages: pool/)
   whole file / chunk --> raw payload
     len > 1 KiB and a 1 KiB probe saves <= 1/32 --> STORED [2][raw]
     compress(raw); frame >= raw              --> STORED [2][raw]
     otherwise                                --> FULL   [0][frame]
     one eligible base B?  --> PREFIX [1][B id][frame vs B], if shorter
```

| Frame | Level | Window log | Decode checks |
| --- | --- | --- | --- |
| Chunk payload | 3 | 20; raw ≤ 32,768, frame ≤ 33,024 | content size = declared raw length, window within profile, no dictionary id, checksum present |
| Whole-file payload | 3 | `whole_file_window_log` (18 or 20); raw ≤ canonical limit − 23 | same |
| PREFIX payload | 3 | as its role | same, against the exact base payload |
| Ordinary group body | 1 | ≤ 16 | decoded length = the directory's declared length |

`codec.rs` is the only module allowed `unsafe`; its header lists the whole
zstd FFI inventory and each block has a SAFETY note. A failed codec call is
returned; no other codec is tried. Finished workspace regions go to a
process-wide idle list, at most one per available CPU per kind.

| Record per lane (LE `u32` lengths) | Layout |
| --- | --- |
| chunk | `tag | raw_len | [base 32] | frame` |
| whole file | `tag | [base 32] | frame` (lengths dropped in the pack) |
| singleton | `tag | raw_len | frame_len | [base 32] | frame` |
| ordinary | `0 | canonical bytes`, or `1 | program` for program pages |

`plan_lane` sends a whole-file record to the singleton lane when its compact
form would not fit a 256 KiB pack.

## delta/

**Selection** (`select` in
[`delta/select.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/select.rs)),
for one whole-file or chunk payload:

| Step | Rule |
| --- | --- |
| 1 Prepare FULL | always encoded first |
| 2 Depth cap 0 | the role's delta depth is 0: FULL |
| 3 Candidate | chunk: the first advisory predecessor only; whole file: the advisory list, else the content-signature ring (`Candidates::find`) |
| 3a Losing base | a chunk base whose last 8 trials in this Save lost is skipped, then retried after 1, 2, 4, … offers (at most 2^16); a win saving less than 1/8 of the FULL record is stored but counts as a loss |
| 4 Eligibility | stored, same role, below the depth cap; else FULL |
| 5 Chain budget | from stored record sizes; a delta candidate that does not fit is replaced by its chain's independent root; still over budget: FULL |
| 6 One trial | the base is read and authenticated, then one PREFIX frame is built; chain plus object must still fit |
| 7 Compare | PREFIX wins only when its record is shorter than FULL; a PREFIX against a re-based root (step 5) also has to be at most half the FULL record, else FULL (`rebased_delta_kept`, #758 fix 4) |

**Re-based delta limit (#758 fix 4).** A re-based trial (step 5) runs
against a root that can be any number of versions older than its target, so
as an object drifts the delta against it grows with history, and every read
of the chain pays it. The trial keeps its PREFIX (or a program page its
program) only when `2 × delta record ≤ FULL record`, both the stored record
lengths the step 7 comparison already uses (`REBASED_DELTA_DIVISOR` = 2 in
[`delta/select.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/select.rs)).
Bound: the anchor of a re-based chain (root plus the one delta against it)
costs a read at most the root's FULL plus half the target's FULL, 1.5 FULL
records for equal sizes instead of up to two, and a member at depth `d` at
most that plus its `d − 1` inner deltas (depth caps 16 / 8 / 8). The rule
limits only the re-based delta: an inner delta, or a first delta tried
directly against a FULL candidate, is held only by step 7 (below the
target's FULL). Against the caller's named predecessor it is one version's
change; against a base the signature ring proposed (a whole file at a new
path) it is not. Storing the FULL costs `full − delta` more bytes once and
saves about `delta` on every later read through it, so it repays itself
within one read exactly past half.

Storage cost: a firing stores `F` where a delta in `(F/2, F)` would have been
stored, so it adds less than `F/2`. It can only happen at a re-base, and a
lineage re-bases at its effective chain length `L`, the deepest member it
reaches before its candidate is capped or no longer fits step 5:
`L = min(C, 1 + largest d whose candidate fits)`. With `S` the canonical
length of each version, `E` the FULL record, `e_i` the delta records and `K`
the cutoff, a depth-`d` candidate fits when `(d + 2)·S ≤ 4K` and
`E + Σ_{i≤d} e_i + S ≤ 2K` (whole file and chunk; a program page of `P`
bytes: `(d + 2)·P ≤ 65,536` and records plus the page's whole record
`≤ 139,281`). After a firing the new FULL is depth 0 and the next re-base
comes `L + 1` versions later, so a lineage adds less than `F / (2(L + 1))`
per version. A depth-0 candidate is never re-based, so `L ≥ 1` and the worst
case is `F / 4` per version. At the 128 KiB default:

| Case | Arithmetic | `L` | Added per version |
| --- | --- | --- | --- |
| Incompressible whole file near the cutoff | depth 1: `E + e_1 + S ≈ 2S + e_1 > 256 KiB` | 1 | `< F/4` |
| Compressible whole file, `S ≤ K` | `4S ≤ 512 KiB` (and `5S > 512 KiB` above `4K/5`) | ≥ 3 | `< F/8` |
| Compressible whole file at the cutoff, `S > K` (envelope up to 23 bytes) | depth 1: `3S ≤ 512 KiB`; depth 2: `4S > 512 KiB` | 2 | `< F/6` |
| Whole file reaching the cap | depth 15: `17S ≤ 512 KiB`, `S ≤ 30,840` bytes, encoded budget holding | 16 | `< F/34` |
| Chunk (≤ 32 KiB), every record a full chunk | canonical fits past the cap; depth 5: `7 × ~32.8 KiB ≤ 256 KiB`, depth 6: `8 × ~32.8 KiB > 256 KiB` | ≥ 6 | `< F/14` |
| Chunk, small deltas | cap | 8 | `< F/18` |
| Program page of 8,192 bytes | depth 6: `8P ≤ 65,536`; depth 7: `9P > 65,536` | 7 | `< F/16` |
| Program page of ≤ 7,281 bytes | depth 7: `9P ≤ 65,536`; depth 8 is the cap | 8 | `< F/18` |

A firing on a re-based ring proposal is not a version of a lineage: it adds
less than `F/2` for that object, with no per-version spreading. These figures
count only the firing's own bytes against the delta it replaced; later
records of the lineage differ from that alternative and are not bounded by
it.

The fraction is fixed: no branch on size, count, history or engine. A trial
against the candidate itself (named by the caller or proposed by the ring,
even a FULL one) is not limited; pooled leaves never re-base and are
unchanged. This is a write-behaviour change for every Store, SQLite
included; reads and the format are unchanged (a FULL is an existing record).
A firing (only a decision the rule changed: a PREFIX or program that would
have been kept but was over half) counts in `full_losses` and
`DeltaCounters::rebased_full` (`BranchCounters::rebased_full` for program
pages); for a chunk the losing-base memo notes it as a loss.

A whole-file object stored FULL enters the signature ring: 8,192 slots,
`INDEX_BYTES` 704 KiB, rows persisted through `signatures`
([`delta/candidates.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/candidates.rs)).
When no PREFIX can be narrower than FULL (`prefix_cannot_win`: the base id
plus a zstd frame of at least 14 bytes), the ring proposal is dropped and no
seal is forced ([`delta/record.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/record.rs)).

A ring proposal the Save itself stored FULL and has not published is never
published early for its trial (#810,
[`select/representation.rs`](../../../../core/crates/layerfs-storage/src/save/select/representation.rs),
[`prepare/deferred.rs`](../../../../core/crates/layerfs-storage/src/save/prepare/deferred.rs)).
Selection may use a base only once its first-wins representation is
acknowledged, so the Save used to publish the proposal at once: one
transaction, reservation and pack per similar file (614 of them for an npm
install of 10,002 files). Now:

- an object whose declared predecessor selection acquires never asks the
  ring (the declared list is tried first), so its proposal is dropped;
- otherwise the trial runs first against the base's canonical bytes, which
  the Save keeps for the whole-file objects it stored FULL
  (`SAVE_PENDING_BASE_BYTES`, 256 KiB). A PREFIX that does not beat FULL
  stores FULL at once, counted as the trial and loss it is: tiny
  near-identical files, where the 32-byte base id alone outweighs the
  saving, end here;
- a PREFIX that wins holds the object (`SAVE_DEFERRED_BYTES`, 1 MiB) until
  the end of its preparation wave. The wave seals the held objects' bases
  with its own publication, then selects each held object as before, its
  proposal now tried after its declared candidates. If an incoming object
  directly references a held root or declares it as a predecessor, the Save
  publishes the cohort's bases and places its held objects before admitting
  the incoming object. Same-Save reads and exact reuse see objects in the
  hold. The last wave publishes once more before the sealed tail when it
  holds any.

Every selection, depth and budget is the one the old order made: the trial
of a held object runs after the publication, against whatever
representation won, so a concurrent winner is re-based as before. Measured
at e11d8c8d0f (one sample per cell, SQLite and PostgreSQL alike): records,
roots and depth histograms are identical in B1, B4, B6, B6e, B6n and A2;
fewer packs leave stored bytes equal or lower (B1 −16,144 B, B6e −18%, B6n
−22%, B4 −64 B, B6 and A2 equal).

**Decoding** ([`delta/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/read.rs)):

```text
 locator root R --base_of--> B1 --base_of--> B2 (FULL)
   walk: read each record's base id, locate it, check role and depth
 decode base first:
   B2: FULL frame       -> canonical(B2); hash == B2 ? else Integrity
   B1: frame vs raw(B2) -> canonical(B1); hash == B1 ?
   R:  frame vs raw(B1) -> canonical(R);  hash == R  ?
```

| Check | Failure |
| --- | --- |
| Pack id not above the read's ceiling, before any cache | `VisibilityCeiling` |
| No id repeated; length within the role's depth | `Integrity("dependency cycle" / "dependency chain depth")` |
| Base has the same role | `Integrity("dependency role")` |
| Canonical and encoded chain work within budget | `Integrity("dependency canonical work" / "dependency encoded work")` |
| Group body and rebuilt object lengths | `Integrity("group body length")`, `Integrity("… record length")` ([`decode.rs`](../../../../core/crates/layerfs-storage/src/encoding/decode.rs)) |
| Rebuilt object hashes to its id | `Integrity("dependency identity")` |

The walk is iterative, so stack depth does not grow with chain length. The
base id inside a PREFIX record is the only stored copy of a dependency edge.
Decoded ordinary group bodies are kept in a `GroupCache` of 512 KiB.

## pool/

```text
 pooled leaf body:  prefix 44 B || (serial u64, ordinal u32) * rows
 record:  0 | body                                 (FULL)
          1 | base id | out_len | count | program  (COPY/INSERT)
 ordinal --> 73-byte inode value, one stored row (PackPersistence::inode_values)

 program page (same ordinary record grammar, tag 0 = canonical page):
   InodeBranch, DirectoryLeaf, DirectoryBranch, ExtentLeaf, ExtentBranch
```

| Part | Rule | Source |
| --- | --- | --- |
| COPY/INSERT | 4,096-bucket table over 16-byte seeds, 128 KiB match budget (an exhausted budget yields no program); program ≤ 64 KiB | [`pool/delta.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/delta.rs) |
| Leaf record | ≤ 8,192 bytes | [`pool/leaf.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/leaf.rs) |
| Program pages | candidate is the first advisory predecessor (the replaced page, or for extent pages the stored page at the same level and position) with the same role; one trial; kept only when shorter, and for a directory page only when it saves at least a quarter | [`pool/program_page.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/program_page.rs), [`delta/program_page.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/program_page.rs) |
| Re-base | a candidate chain at its depth cap or budgets restarts at the whole page at its root (depth one) instead of storing the page whole; that program is kept only when it is at most half the whole record, else the page is stored whole (#758 fix 4) | [`delta/program_page.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/program_page.rs) |
| Metadata chain budgets | depth 8 by default; 65,536 canonical and 139,281 encoded bytes | [`src/policy.rs`](../../../../core/crates/layerfs-storage/src/policy.rs) |
| Read | rebuild the body from the chain, look up all ordinals in one call, rebuild the leaf, check its id; a demand of several leaves prepares up to 128 leaves (1 MiB of bodies) ahead and looks up their ordinals together | [`pool/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/read.rs) |
| Chain memo | off unless the reader's owner asks (`PoolReader::with_chain_memo(bytes)`). The reader then keeps each chain element it rebuilt and checked against its identity (a program page's canonical bytes, a pooled leaf's body) with the depth, canonical bytes, record bytes and highest pack of the chain below it, and a later chain stops at the first element held. The depth cap, the chain budgets and the visibility ceiling are applied to the kept figures, so every answer and refusal is that of a full walk. Two generations of half the byte limit each; the old one is dropped when the young one fills | [`pool/memo.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/memo.rs), [`pool/program_page.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/program_page.rs), [`pool/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/read.rs) |

The locator's role decides how tags 0 and 1 are read, so a Store written
before program pages reads unchanged. Ordinary packs that may hold a
directory or extent program are written at version 26, which an older reader
refuses by version rather than inside a decode.

## pack_cache.rs and pack_units.rs

```text
 PackCache (one per Reader, ReadCache or Save)
   (pack_id, None)    -> whole body
   (pack_id, Some g)  -> one encoded group (PackUnit)
   retained <= 2 MiB and <= 4,096 entries; least recently touched first
   a body over 2 MiB (singleton) is admitted alone after evicting the rest
```

| Property | Value |
| --- | --- |
| Owner | one reader, kept read cache or Save; never shared |
| Changed bytes for a cached id | `Integrity("immutable cached pack changed")` |
| Another descriptor for a cached pack id | `Integrity("immutable cached descriptor changed")` |
| Acquisition | `acquire_groups` fetches only missing groups, as one body or selected units; `admit_groups` admits an acquisition already in hand (a group read, #758) under the same checks; `held` names the entry holding a group and its bytes, which a group read's page counts |
| Work counts | `PackCacheWork`: evictions, evicted bytes, peak bytes |

Pack ids only grow and published packs never change, so a kept body cannot
go stale; eviction costs a later read, never correctness.

| Type ([`units.rs`](../../../../core/crates/layerfs-storage/src/pack/units.rs)) | What it is |
| --- | --- |
| `PackAcquisition::Whole` | a complete pack body, with its descriptor when it came from the Store |
| `PackAcquisition::Units` | complete encoded groups cut from a ranged read; the directory is checked and each used record is authenticated by its id |
| `PackUnit` | one complete group; a range that splits a group or names the singleton lane is refused |
| `GroupSlice` | a borrow of one validated group from either form, handed to the decoder |

## Related

- [Physical storage](README.md): packs, Saves, reads and ports around these records.
- [Objects](../objects/README.md): roles, canonical forms and advisory predecessors.
- [Files](../files/README.md): where chunk hints and extent-page hints come from.
- Tests: [`base_budget.rs`](../../../../core/crates/layerfs-storage/tests/encoding/base_budget.rs),
  [`losing_chunk_bases.rs`](../../../../core/crates/layerfs-storage/tests/encoding/losing_chunk_bases.rs),
  [`program_pages.rs`](../../../../core/crates/layerfs-storage/tests/encoding/program_pages.rs)
  (`cargo +1.85.1 test --locked -p layerfs-storage --test encoding program_pages`, any platform).
