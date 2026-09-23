# Physical writing: codec, framing, placement and assembly

> **Status:** Research; source-backed description of the current tree. Informative,
> not a product contract.

The pending issue #192 schema 7 changes are described in
[save ownership and publication](15-multi-writer-storage.md), based on
`0819f3f39833d477d9ed6d878a50691c3c046a83`. That description supersedes the
older exclusive-save, prefix-publication, shared-private-cache and cleanup rules
below, and adds the returned-read byte bound and bounded ordinal window. The
older source-pinned sections remain historical descriptions; they do not qualify
the pending implementation or a performance change.

Part of the [replacement-core architecture](README.md) set. Source pin
`ce2d738ff`; the placement running total added by #178 **P2-8** (2026-09-18) is
marked in place and carries its own commit. Scope, method, measurement status and
upkeep are stated in the [index](README.md).

The #237 bounded multi-group admission amendment in §18.4.2 describes the
product source in **this document's commit**, which also changes
`cas/placement.rs`, `cas/save.rs` and `cas/selection.rs`. Earlier sections retain
their historical source pins where they describe older behavior.
The #237 pack-space C1 amendment in §18.4.3 describes product source at
`bbb0281bc`, and C2 in §18.4.4 describes source at `2ba19aad8`.
The selective pooled-lane amendment in §18.4.5 describes product source at
`4b94e9131`. The final pooled-tail amendment in §18.4.6 describes product
source in this document's commit; earlier section pins stay historical.

Chapter numbers are global to the set: this paper holds **chapter 18**.

---

## 18. Physical writing

### 18.1 Where this sits

[§6.7](05-storage.md) describes the five **framings** a pack can carry. This paper
describes how bytes actually get *into* them:

```text
   C1 emits a FinalizedObject
        │
        ▼
   ┌── CODEC ────────────────────────────────────────────────┐
   │  §18.2  CodecProfile · static contexts · workspaces      │
   │         zstd level 3 (payload) / level 19 (group)        │
   └───────────────────────┬─────────────────────────────────┘
                           ▼
   ┌── FRAMING ──────────────────────────────────────────────┐
   │  §18.3  frame_group · framed_group_length                │
   │         count + end offsets + payloads                   │
   └───────────────────────┬─────────────────────────────────┘
                           ▼
   ┌── PLACEMENT ────────────────────────────────────────────┐
   │  §18.4  LanePlacement::select_many — append or new pack  │
   │         decided by EXACT measured fit, before assembly   │
   └───────────────────────┬─────────────────────────────────┘
                           ▼
   ┌── ASSEMBLY ─────────────────────────────────────────────┐
   │  §18.5  assemble / assemble_consuming — one pass,        │
   │         exactly one write per pack per call              │
   └───────────────────────┬─────────────────────────────────┘
                           ▼
                 UPDATE/INSERT object_packs.data
```

The ordering rule that governs the whole pipeline, from `assemble.rs`:

> Placement decides what goes into the write **before any bytes are assembled**, so
> no candidate pack is built and then discarded.

Every stage below is arranged to preserve that.

---

## 18.2 The codec

### 18.2.1 Profiles

```text
   CodecProfile { raw_limit, frame_limit, window_log }

   native()               chunk payloads
        32 KiB raw, window log 20

   whole_file(capacities) whole-file payloads, DERIVED FROM THE POLICY
        raw/frame/window follow the accepted construction cutoff, so a larger
        cutoff gets the window its payload needs while the default cutoff keeps
        its frozen parameters exactly

   group bodies           GROUP_LEVEL = 1, GROUP_WINDOW_LOG_MAX = 16
                          GROUP_LIMIT = 65,536 raw, GROUP_FRAME_LIMIT = +1024
```

The payload parameters, in the module's own words: *"level 3, a role-specific
window log, a content-size field, a checksum, **no dictionary id and no workers**."*
Nothing in the codec is configurable at run time beyond the policy-derived
whole-file profile.

**The group level is 19, and the payload level is 3, and both were chosen by
measurement rather than inherited.** A payload-level sweep on the retained-history
lane (`history-stride10`) measured the marginal return per CPU-second as
**L3→L5 1,408,000 · L5→L7 1,176,879 · L7→L9 84,176** B/CPU-s: the knee is at 7 and
level 9 — which the codec's own instrument had suggested — is the worst-value point
on the lane curve. Payload 3 is kept, and it clears the storage gate at the lowest
CPU measured. The group level is a separate constant: raising it 1→19 bought
**270,336 B for about 1.0 s** on the same lane, which is what carries the gate.

**The encode workspace is 16 MiB because 2 MiB is not enough.** A static context at
level 9 returns `ZSTD_error_memory_allocation`, and the widest construction policy
the public API accepts needs 13,100,048 B. The product's own
`codec_frames::a_frame_beyond_the_profiles_window_is_refused` caught this as
`Integrity("bounded Zstandard workspace unavailable")`.

### 18.2.2 Contexts live in caller-owned memory

This is the structural decision in the codec, and it is not an optimisation detail:

```text
   CompressionWorkspace   ENCODE_WORKSPACE_BYTES = 16 MiB, aligned
   DecompressionWorkspace DECODE_WORKSPACE_BYTES = 1 MiB, aligned

   ZSTD_initStaticCCtx / ZSTD_initStaticDCtx
        ⇒ the zstd context lives INSIDE the caller's region
        ⇒ a codec call cannot grow an allocator-backed context
        ⇒ every allocation is charged before the call
```

Both workspaces are shared by **every role of one save** (encode) or **one read**
(decode) — one region, reused, not one per object. `CompressionWorkspace::new` and
`DecompressionWorkspace::new` can therefore fail on allocation, and
`workspace_bytes()` reports what was actually reserved.

The consequence for a receipt is that a codec call's memory is **declared and
fixed**, not proportional to the payload: compressing a 16 MiB object and a 16 KiB
object both use the same 16 MiB region.

### 18.2.3 Prefix state never outlives its call

```text
   ZSTD_CCtx_refPrefix / ZSTD_DCtx_refPrefix     supply the delta base
   ZSTD_CCtx_reset / ZSTD_DCtx_reset             cleared on SUCCESS and on FAILURE

   ⇒ no prefix outlives the call that supplied it
   ⇒ a failed call never selects another codec
```

The second line is the no-fallback rule at the codec layer: a codec failure is
returned as a failure, and there is no trial of a second algorithm.

### 18.2.4 The unsafe boundary — 14 blocks, all FFI

`encoding/codec.rs` is the crate's **single audited `unsafe` module**
([§6.6](05-storage.md)), because `forbid(unsafe_code)` cannot be relaxed for one
module (E0453). The whole surface is zstd FFI, and the imports are the inventory:

```text
   contexts    ZSTD_CCtx · ZSTD_DCtx · ZSTD_initStaticCCtx · ZSTD_initStaticDCtx
               ZSTD_CCtx_reset · ZSTD_DCtx_reset
   parameters  ZSTD_CCtx_setParameter · ZSTD_DCtx_setParameter
               ZSTD_CCtx_setCParams · ZSTD_CCtx_setFParams · ZSTD_getCParams
   prefixes    ZSTD_CCtx_refPrefix · ZSTD_DCtx_refPrefix
   calls       ZSTD_compress2 · ZSTD_decompressDCtx
   bounds      ZSTD_compressBound · ZSTD_estimateCCtxSize_usingCParams
   frames      ZSTD_findFrameCompressedSize · ZSTD_getFrameHeader
               ZSTD_FrameHeader · ZSTD_FrameType_e
   errors      ZSTD_isError · ZSTD_getErrorCode · ZSTD_ErrorCode
```

Two entries are there for a reason worth recording, because their absence was a
real defect once: `ZSTD_compressBound` and `ZSTD_estimateCCtxSize_usingCParams` are
the **bounding** calls — the ones that let the codec size its region and refuse an
oversized frame *before* writing. Commit `1884e3eca` in this repository exists
because a verification pass found them missing from the module's declared
inventory. An FFI inventory that omits the bound checks is not a safety argument.

`ZSTD_getErrorCode` is also used to distinguish an allocation failure from any
other error, so the two do not collapse.

`parse_frame_header` is the one `unsafe fn` in the module — it returns a
`ZSTD_FrameHeader` borrowed from the frame, which is why the read path parses
before decoding.

---

## 18.3 Group framing

```text
   frame_group_bounded(records, limit)

   framing = 4 + 4 × records
             │      └── one u32 END OFFSET per record
             └── one u32 record count

   body = framing ‖ record₀ ‖ record₁ ‖ …

   GROUP_LIMIT        = 65,536   ordinary / native / whole-file / pooled
   RECORD_COUNT_LIMIT =  8,191   records per group
```

End offsets rather than per-record lengths: a record's start is the previous
record's end, so the array is one u32 per record and the first record begins
immediately after it. That is why the *framed* length is not a sum of per-record
framed lengths.

### 18.3.1 The projection that decides sealing

```rust
/// The exact framed length this group WOULD have with `records` records and
/// `payload` payload bytes.
pub fn framed_group_length(records: usize, payload: usize) -> StorageResult<usize>
```

The seal decision in `cas/selection.rs` (moved there from `cas/owner.rs` by #178
**P2-0**, 2026-09-18) calls this projection rather than summing
per-record lengths:

```rust
occupied && framed_group_length(
    self.groups[index].records.len() + 1,
    self.groups[index].payload_len + record.record.len(),
)? > GROUP_TARGET
```

`policy.rs` records why: summing per-record framed lengths *"would count the shared
count and end offsets once per record."* The projection is exact, so the decision
never requires assembling a candidate group to measure it.

`assemble_consuming(lane, groups)` is the release-aware twin of
`assemble(lane, groups)` — it consumes the groups and frees each body as it is
copied, for the case where the retained tail is about to be replaced (§18.5).

---

## 18.4 Placement — exact fit, decided before assembly

```text
   struct LanePlacement { open: Option<OpenPack> }        ONE open pack per lane
   struct OpenPack { pack_id, groups, assembled }         assembled = the tail's
                                                          running total (#178 P2-8)

   select_many(lane, groups, next_pack_id) -> Vec<SelectedWrite>

        for each group, in order:
             │
             ├── append_fits(lane, open.assembled, open.groups.len(), &group)?
             │     EXACT assembled length + group count
             │     the running total is READ, not re-measured (P2-8)
             │     the open tail and the incoming group are never copied
             │     the LANE'S OWN limits apply, not another lane's maxima
             │
             ├── fits    ──► append to the open pack
             │
             └── !fits   ──► the open pack is full
                              • flush any pending write for it, CLOSING it
                                (its tail is dead ⇒ assemble_consuming)
                              • take next_pack_id, open a NEW pack
                              • the new pack's write is marked CREATED
```

**Object rows are written with one statement per bound chunk (#178 P2-2,
2026-09-18).** Rows from one or several groups are inserted together, `k` at a time, with `k`
derived from the connection's own `SQLITE_LIMIT_VARIABLE_NUMBER` and
`SQLITE_LIMIT_SQL_LENGTH` and capped at 128 - read back from the engine, never
copied as a constant - and SQLite applies each multi-row `INSERT` atomically, so a
chunk's rows all land or none do. The statement text differs only in how many
placeholder groups it carries, so the prepared-statement cache holds one entry per
chunk size and every later chunk is a cache hit. `SaveOutcome::statements` counts
the statements, not the rows; `sqlite_master` is untouched, so the pinned schema
identity is unchanged.

Two properties the source states and the code enforces:

**"Every pack that receives a group in this call produces exactly one write,
assembled once, after the last group that landed in it."** A single call placing
40 groups across 3 packs produces **3** writes, not 40 — and each is assembled
after its final group, so no pack is assembled twice.

**Pack identifiers are dense and monotone.** `next_pack_id` is checked positive and
bumped with `checked_add`, so an overflow is an error rather than a wrapped
identifier.

### 18.4.1 The retained tail holds framed groups, not bytes

```text
   OpenPack { pack_id, groups: Vec<EncodedGroup> }      ← FRAMED, not assembled
```

`layout.rs` states the consequence, and it is the reason the whole-pack rewrite in
[§16.5.4](11-optimization-study.md) is safe:

> The retained tail holds framed groups, not assembled bytes, so a write
> re-assembles only the groups the selected pack actually contains and **existing
> group/record ordinals never move**.

A pack appended to in three separate calls is re-assembled three times — and
produces byte-identical bytes for the groups it already held, so `group_number` and
`record_number` are stable across appends. Locators survive.

### 18.4.2 Bounded multi-group admission during one save wave (#237)

`MutationOwner` now retains one FIFO of **sealed** ordinary, native or whole-file
groups while a preparation wave holds the Store's arbitration and transaction.
The FIFO contains only one lane at a time, at most **256 KiB of encoded group
bodies** and at most **512 locator rows**. A lane switch or projected bound
flushes it. Pooled-metadata and singleton groups continue through the immediate
path. The wave's existing 512-object/4 MiB limits, transaction cadence, SQLite
page size and physical pack grammar are unchanged.

A flush moves the queued groups into the existing `LanePlacement::select_many`.
That placement returns one increment per receiving pack; `write_pack` still
invalidates cached pack bytes for each increment, and the existing engine-limited
multi-row SQL writer inserts all resulting locators. The row is visible only after
its pack bytes and locator have both been written in the same transaction.

The queue also belongs to the dependency boundary. An exact reuse or same-save
read that asks for a queued identity flushes before its locator lookup; a
whole-file predecessor or content-index candidate flushes before delta-base
selection. Direct-reference validation may recognize a queued identity as
accepted, because the queue is drained before the wave's collision check,
candidate-index flush and COMMIT. Failed placement, SQL or collision work fails
the save through its existing rollback/cleanup path. No queued group survives a
wave or the final publication. This is a bounded change to *when* groups are
placed, not a new format or a path that moves cold source or Store work outside
the caller's operation.

### 18.4.3 Exact capacity for a new pack closed in one placement call (#237)

This was the C1 treatment at `bbb0281bc`; §18.4.4 supersedes its open-tail
rule. Its [one-shot 100k result](../issues/237/pack-space-c1-result-20260924.md)
saved 622,930 B of 35,168,077 B reserved pack tail and passed full readback.

`LanePlacement::select_many` can receive enough groups to create a pack and
then displace it with the next pack **before any of that call's writes reach
SQLite**. That created-and-closing pack can never be appended later, so its
`SelectedWrite.capacity` equals its final declared `used` length. A newly
created pack still open at the call boundary reserves the lane's full 256-KiB
limit, as does any pack already inserted by a prior call. The existing
`zeroblob(write.capacity)` plus bounded incremental BLOB writes and reader
grammar are unchanged; only the length of a row known final before insertion
differs. This avoids a full-BLOB rewrite and adds no pending payload buffer.

This is the narrow C1 mechanism from the [prospective #237 treatment plan](../issues/237/pack-space-treatment-plan-20260924.md).
Its physical saving and any sparse-history effect require measured receipts;
it does not claim to remove the unused tail of a pack first inserted while
still open. The #236 benchmark selection and historical receipts are unchanged.

### 18.4.4 Close the selected tail at every flush (#237)

The historical C2 treatment closes the last pack of each `select_many` call too, and
releases its open placement state before another call. A later group always
starts a new pack rather than appending to an already inserted row. Because
every pack has its final length **before its first SQLite INSERT**, its
`SelectedWrite.capacity` equals `used`, while the same reserved directory
grammar and incremental BLOB writer continue to apply. A demanded same-Save
read still flushes the queue and writes pack bytes before its object locators
in the same transaction; no visibility or publication work moves outside the
operation. The existing bounded queue is unchanged and no extra pending body
buffer is introduced.

This changes physical pack granularity, not canonical object identities or
reader grammar. More pack IDs, rows, directory reservations and SQL work may
offset the saved BLOB tails. The [prospective C2 rule](../issues/237/pack-space-treatment-plan-20260924.md)
requires measured Store apparent/allocated bytes, pack count, full reopened
readback, time and RSS before accepting it. Existing larger-capacity packs
remain readable. The #229 sparse-history guard remains a separate requirement.

### 18.4.5 Reuse the pooled lane's open pack (#237)

The C3 treatment retains C2's exact-length close for Ordinary, Native and
WholeFile packs, which carried almost all of the original reserved tail.
PooledMetadata keeps one open pack across placement calls and uses the
existing incremental append path. A newly created pooled row therefore
reserves 256 KiB; later groups in that lane write their directory entry,
body and control area into that row before the corresponding catalogue row
is inserted. Same-Save reads still seal and see the accepted values. A full
pooled pack is displaced and the next pack takes a new ID.

The C2 100k run created one pooled pack for each of 2,001 groups, adding
8,178,200 B of headers and fixed directories compared with the previous
16-pack grouping. C3 recovers that grouping without a format change or an
extra body queue; the expected saving is a prospective calculation in the
[C3 plan](../issues/237/pack-space-c3-plan-20260924.md). The closed Store,
resource cost, readback and sparse-history result determine whether it is
accepted. The retained C2 receipt is not relabelled.

### 18.4.6 Finalize the last pooled row within Save (#237)

During a Save, PooledMetadata still reuses one open 256-KiB row across
placement flushes and writes each accepted group before its catalogue row.
After the final lane seal, the owner consumes that open state and, inside
the Save's existing final transaction and before publication, shortens
only its final pooled BLOB to the header-declared used length. The SQL
UPDATE checks the owned pack ID, unpublished Save, v12 header, original
capacity and declared length and must affect exactly one row. A full row
needs no update. No later append can follow this boundary; group ordinals,
body offsets and locators do not move.

This is one bounded whole-BLOB rewrite of at most 256 KiB per Save, not
a per-append rewrite or a post-operation compaction. A definite failure
follows the existing rollback and Save cleanup; an uncertain COMMIT keeps
its existing quarantine. SQLite's `auto_vacuum=NONE` profile can reuse
freed overflow pages for later Saves, while pages freed by the final Save
can remain in the closed file's freelist. The
[prospective sparse-tail plan](../issues/237/pack-space-c5-sparse-tail-plan-20260924.md)
requires matched release evidence for that effect. The dense Init and
the full #229 history guard remain distinct proofs.

---

## 18.5 Assembly

```text
   assemble(lane, groups)            borrows; the caller still owns the bodies
   assemble_consuming(lane, groups)  consumes; releases each body as copied

   header   PACK_MAGIC ‖ control area (HEADER_LEN = 16)
   directory  one entry per group
              ordinary/native: 16 B   whole-file: 4 B (starts only)
   bodies   the framed groups, in order
```

The `closing` flag in `assemble_write` chooses between them:

| Situation | Call | Why |
| --- | --- | --- |
| the pack stays open | `assemble` | a later group may still append, so the tail must survive |
| the pack is full and being replaced | `assemble_consuming` | *"its tail is dead: the assembly consumes the groups and releases each body as it is copied instead of holding both"* |

So the two paths exist to avoid holding two copies of a pack body at the moment a
pack is retired — the one point where a naive implementation would double its
transient memory.

---

## 18.6 Complexity

`R` = records in a group, `B` = body bytes, `P` = assembled pack bytes, `G` = groups
placed in one call, `W` = packs receiving groups in that call.

| Stage | Time | Peak memory | Notes |
| --- | --- | --- | --- |
| `compress` / `decompress` | O(B) | **O(1)** — the 16 MiB / 1 MiB region | static context, charged up front |
| `frame_group` | O(R + B) | O(B) — the body | `4 + 4R` framing |
| `framed_group_length` | **O(1)** | O(1) | pure arithmetic on two numbers |
| `assembled_length` | **O(groups)** | O(1) | loops summing `body_size`; the canonical predicate |
| `append_fits` | **O(1)** | O(1) | reads the open state's running total + one `body_size` (P2-8) |
| `retained_bytes` | **O(1)** | O(1) | the same running total (P2-8) |
| `select_many` | O(G) | O(G) decisions + the open tail | one write per pack, not per group |
| `assemble` | O(P) | O(P) | borrows bodies |
| `assemble_consuming` | O(P) | **O(P) with bodies released** | avoids holding two pack copies |

Three rows are worth noting, and one of them corrects an easy assumption.

**`framed_group_length` is O(1)** — pure arithmetic over `(records, payload)` — which
is what makes a *seal* decision free.

**`append_fits` is O(1) as of #178 P2-8 (2026-09-18).** It used to call
`assembled_length`, which loops over the open pack's groups summing `body_size`,
so a fit decision cost **O(open groups)** and one `select_many` call cost up to
O(G · 256). The open-lane state now carries the tail's assembled length
(`OpenPack::assembled`, maintained as groups land and reset when a closing
assembly consumes the tail), so a fit decision is one read plus one `body_size`
for the incoming group. "Measured, never copied" remains true in both forms: no
body is copied to decide fit. `assembled_length` stays the canonical predicate,
and the placement case in `crates/layerfs-storage/tests/pack_locator.rs`
recomputes the running total from it at every boundary probe, so a drift between
the maintained total and the canonical length fails there rather than in a stored
pack.

**The codec is O(1) in memory**, which is what makes a payload's peak independent
of its size.

`W ≤ G`, and in a steady append workload `W = 1` — so a call placing `G` groups
usually produces one write of the whole pack.

## 18.7 What is not established

- **No measurement.** Every figure is a declared constant or arithmetic over one.
  Nothing here claims a throughput or a memory figure for a real save.
- **The whole-pack rewrite on append** is a shared cost with the reference, not a
  core regression, and it is recorded in
  [`11-optimization-study.md` §16.5.4](11-optimization-study.md) and
  [#176](https://github.com/Ephemeral-AI-Lab/layerfs/issues/176). The *locator
  stability* argument in §18.4.1 is what makes it a bandwidth cost rather than a
  correctness one.
- **Compression ratios are not characterised.** `GROUP_TARGET` (49,152) against
  `GROUP_LIMIT` (65,536) is analysed arithmetically in
  [§16.4.2](11-optimization-study.md); whether the smaller group costs ratio is a
  measurement.
- **The FFI inventory is a reading, not an audit.** §18.2.4 lists what the module
  imports and states why two entries matter. It is not a memory-safety argument;
  that belongs to the boundary guard and to semantic review.
