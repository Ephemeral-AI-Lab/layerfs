# Deferred: delta hints across the whole-file / chunked size transition

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

**Tracking issue:**
[#185 — v0.1.7 deferred: delta hints across the 128 KiB whole-file ↔ chunked size transition](https://github.com/Ephemeral-AI-Lab/layerfs/issues/185)
(label `deferred`; it carries the revisit condition and the exit criteria).

This paper records one **deferred** item: offering a stored object of the
*previous* file representation as the hinted physical delta base for the objects
of the *new* representation when a file crosses the construction cutoff `T`
(`small_file_threshold_bytes`, default 131,072).

It is diagram-led because the same mechanism exists in three different states at
once — implemented in the reference tree, declared but never produced in `core/`,
and declined by the v0.1.7 design. The diagrams exist to keep those three states
apart; a single merged picture is how they get confused.

**This paper holds no chapter number.** Chapter numbers are global to the
[replacement-core architecture set](../README.md), and this is not one of its
thirteen descriptive papers: it describes no shipped behaviour.

## Claim labels

Reused from the [proposal folder](../proposal/README.md), so the two kinds of
guidance read the same way:

| Label | Meaning |
| --- | --- |
| **holds today** | read from source; true of the tree at the pin |
| **proposed** | does not exist in `core/`; a design to be argued with |
| **open — required** | a prerequisite for something else here |
| **deferred** | explicitly out of scope; an owner amendment must precede it |

## Source pins and method

- Every citation was re-read from the working tree at commit `0654d3afd`
  (`core/crates/`, `crates/` and `core/docs/` were clean at that commit; the
  unrelated `core/benchmark/` harness was dirty and is not cited here).
- The set's own pins are unchanged: `1884e3eca` for this architecture set
  ([index](../README.md)) and `ce2d738ff` for the
  [optimization study](../11-optimization-study.md).
- Method: source reads only. No build, test, benchmark or code change was made
  for this paper, and **it produces no performance number**. The single measured
  comparison quoted under G6 is attributed to the evidence that recorded it.

## 1. The case

A file crosses `T` in either direction. The old representation's objects stay on
disk (nothing is reclaimed), and the new representation's objects are new. The
only question here is whether the old representation may be *offered* — as a
bounded, advisory hint — to the delta selector for the new objects:

```text
   small → large   base is ONE whole-file object; result is CDC chunks
   large → small   base is a chunked mapping tree; result is ONE whole-file object
```

Three states hold at the pin, and they are not the same claim:

1. **The reference attaches and uses a hint for a chunked base.** The previous
   root is attached as an advisory predecessor in both directions, and a
   positional cursor supplies the base extents overlapping each newly emitted
   chunk — but only when the base itself is chunked.
2. **`core/` carries the field and drops the producer.** `AdvisoryPredecessors`
   crosses the C1/C2 boundary with four slots and provenance tags, and a
   whole-file result is hinted at the base root; the chunk route hardcodes `None`,
   and `PredecessorProvenance::ReusedRange` is never constructed by product code.
3. **Neither tree admits a cross-role base**, so *at the transition itself* the
   hint cannot be used: a `FileState` base is ineligible for a `WholeFile` target
   and vice versa. The v0.1.7 design declines this on purpose and requires an
   owner amendment before any cross-representation encoding.

State 1 is the subject of the [positional-hint proposal](../09-delta-hints.md)
(chapter 14); state 3 is the part this paper records as **deferred**.

## 2. The diagrams

### G1 · The four transition cases — dispatch is on the RESULT, never on the base

```text
                    RESULT representation
                  WHOLE_FILE          CHUNKED
              ┌─────────────────┬─────────────────┐
   WHOLE      │ small → small   │ small → large   │  ← size transition
   FILE       │ assemble        │ re-chunk all    │
   (base)     ├─────────────────┼─────────────────┤
   CHUNKED    │ large → small   │ large → large   │  ← size transition
              │ assemble        │ splice + CDC    │
              └─────────────────┴─────────────────┘
```

**holds today** — `core/crates/layerfs-content/src/file/edit/apply.rs:92` (whole-file
arm), `:132` (chunked arm), dispatched from `apply_edits` at `:38`; described in
[§13.1](../08-representations.md#131-the-transition-matrix).

### G2 · Case C — large → small

```text
   base    [ c0 ][ c1 ][ c2 ][ c3 ][ c4 ][ c5 ]   chunks + mapping tree
            └── kept ──┘  └ dropped ┘  └── kept ──┘
                          never read

   result  [============== ONE whole object ==============]   < T bytes

   hint    candidate = B  (the old FileState root)
             [ref]  small_predecessor = Some(root.0)      objects.rs:3318
             [core] explicit(view.root())                  apply.rs:121
             ✗ role FileState ≠ role WholeFile  →  ineligible  →  FULL
             ✓ fallback: session signature cache, same role only

   storage  +1 whole object; the old chunks stay on disk
```

**holds today** on both sides — retained ranges only (`apply.rs:153`
`assemble_into`, `:192` the `Segment::Retain` arm); the hint attachment
(`apply.rs:121`); the role rejection
(`core/crates/layerfs-storage/src/encoding/delta/select.rs:366`, in `eligible`
at `:356`); the same-role cache fallback (`select.rs:257`); the reference's
attachment and delivery (`crates/layerfs-layerstack-store/src/objects.rs:3318`,
`:2836-2840`) and its SmallContent-only anchor
(`crates/layerfs-layerstack-store/src/objects/read.rs:583`, `:640`). Core pins
the outcome in `core/crates/layerfs-storage/tests/edit_pipeline.rs:641`
(`counters.edges == 0`).

### G3 · Case B — small → large

```text
   base    [========== ONE whole object, < T ==========]
                              + inserted bytes
   result  [ch][ch][ch][ch][ch][ch][ch][ch]   re-chunked from scratch
            ?   ?   ?   ?   ?   ?   ?   ?
            └──── no candidate is offered to any chunk ────┘

   hint    candidate = B  (the old whole-file object)
             [ref]  set but INERT — only a small-content result consumes it
             [core] nothing at all: push_chunk(chunk, None)   build.rs:326

   storage  every chunk is FULL on a first growth (CAS has nothing to match)
```

**holds today** — core's `stream_combined` (`apply.rs:397`) calls
`build_streaming` (`core/crates/layerfs-content/src/file/mapping/build.rs:319`),
which passes `None` for every chunk at `:326`; the reference's
`set_physical_predecessor` sets the small predecessor at `objects.rs:3318` and
then returns before attaching the cursor when the base is a whole-file object
(`:3319-3324`), and the delivery path applies that predecessor only to
small-content objects (`:2836`).

### G4 · The hint producer — two coordinate spaces

```text
   BASE   (original)  0 ────[ e0 ]────[ e1 ]────[ e2 ]────►  extents, base order
   RESULT (current)   0 ──[ ch ]──[ ch ]──[ ch ]──[ ch ]──►  CDC offsets

   the hint must answer: which BASE extent does this RESULT chunk correspond to?

   cursor   hints(start, len) ─► extents overlapping [start, start+len)
            forward-only: start < last_end ⇒ InvalidRecord("predecessor span order")
            ≤ 4 ids, deduped, 4,096-descriptor cap, exhaustion is not an error

   [ref]   PredecessorCursor  rope/read.rs:368  + first_span per emitted object
   [core]  absent — the planned file/mapping/predecessor.rs was never written
```

**holds today** in the reference (`crates/layerfs-content/src/file/rope/read.rs:368`,
`hints` at `:395`; attached from
`crates/layerfs-workspace/src/changes.rs:1837`, filled at
`crates/layerfs-layerstack-store/src/objects.rs:2855`). **proposed** in `core/`:
[chapter 14](../09-delta-hints.md#142-the-gap) states the gap and
[§14.9](../09-delta-hints.md#149-what-would-finalize-this-paper) the test that
would settle it. The planned core file that owned the coordinate distinction —
`file/mapping/predecessor.rs`, "bounded reference cursor with original/current
coordinate distinction" — is recorded in
[the Stage 4 file plan](../../../../docs/roadmap/0.1/0.1.7/component-decoupling/stages-3-4-file-plan.md)
and was never created.

### G5 · The per-object selection gate — same on both lanes

```text
   every emitted object
        │
        ▼
   exact CAS match? ──yes──► reuse the row (no new record, no trial)
        │ no
        ▼
   candidate eligible?   role == role ∧ depth < cap ∧ chain within budget
        ├─ absent / ineligible / none ──► FULL   (policy outcome, counted)
        └─ eligible ──► ONE prefix trial ──► compare complete framed cost
                              ├─ prefix cheaper ──► DELTA (base = required dependency)
                              └─ else           ──► FULL
   codec / read / SQL failure ──► ERROR, never a silent FULL
```

**holds today** — the chunk arm considers exactly the first candidate
(`core/crates/layerfs-storage/src/encoding/delta/select.rs:247`); the other
payload roles take the advisory list in order and then the content-keyed session
cache (`:254-262`); eligibility is role equality plus the role's depth cap
(`:356-372`). The reference's one-trial rule is the same.

### G6 · Today vs improved, side by side

```text
   TODAY                                     IMPROVED — proposed
   ─────────────────────────────────────     ─────────────────────────────────────
   B · small → large                         B · small → large
   ┌────┐┌────┐┌────┐┌────┐                  ┌────┐┌────┐┌────┐┌────┐
   │FULL││FULL││FULL││FULL│                  │PRFX││FULL││PRFX││PRFX│
   └────┘└────┘└────┘└────┘                  └────┘└────┘└────┘└────┘
   no candidate                              the base is already streaming through
                                             the operation → one trial per chunk
                                             against it; no extra read

   C · large → small                         C · large → small
   ┌──────────────┐                          ┌──────────────┐
   │ FULL object  │                          │ DELTA vs one │
   │              │                          │ chunk base   │
   └──────────────┘                          └──────────────┘
   role-ineligible candidate                 needs the cross-role pair admitted
                                             + a bounded base choice (one trial)
```

**proposed**, both columns of the right-hand side. The B improvement is the strong
case: the new chunk's bytes lie inside the old whole-file payload, and that
payload is already streaming through the operation, so the candidate costs no
extra read. The C improvement is plausible but unmeasured, and needs a bounded
choice of *which* retained chunk to offer. The concrete recorded instance of the
C gap is the issue100 cutoff crossing — a file shrinking from 133,273 B (CDC) to
129,991 B (SmallContent), where the reference stored a 50,626-byte FULL frame
while Git stored a 5,492-byte depth-one delta. Those figures are recorded in
[the issue100 gap study](../../../../docs/roadmap/0.1/0.1.5/issue100/layerfs-gap-study.md)
and [its direction list](../../../../docs/roadmap/0.1/0.1.5/issue100/git-gap-directions.md);
they are not measured by this paper and are not a saving forecast.

### G7 · Where each piece lives

```text
                          core/ today        crates/ today         status
   cursor, chunked base    absent             PredecessorCursor     ch. 14 proposal
   per-chunk hint          left neighbour     overlapping extent    ref-only
   whole-file base hint    None               set, but inert        usable in neither
   cross-role candidate    rejected           rejected              open amendment
```

## 3. Claim ledger

| # | Claim | Label | Evidence |
| --- | --- | --- | --- |
| 1 | Transition dispatch is on the result representation | holds today | `core/.../file/edit/apply.rs:92`, `:132`; [§13.1](../08-representations.md#131-the-transition-matrix) |
| 2 | large → small reads retained ranges only, never the discarded ones | holds today | `apply.rs:153`, `:192`; [§13.2](../08-representations.md#132-what-a-transition-costs) |
| 3 | small → large hardcodes `None` for every chunk | holds today | `core/.../file/mapping/build.rs:326`; [§13.4](../08-representations.md#134-transitions-take-no-delta-base--in-either-direction) |
| 4 | large → small attaches the base root, and the role check rejects it | holds today | `apply.rs:121`; `select.rs:356-372`; `edit_pipeline.rs:641` |
| 5 | The four-slot carrier and its provenance tags cross the C1/C2 boundary | holds today | `core/.../object/predecessor.rs:14`, `:24` |
| 6 | `ReusedRange` is never constructed by product code | holds today | only `core/crates/layerfs-storage/tests/delta_payload.rs:131` |
| 7 | The reference has a bounded positional cursor over a chunked base | holds today | `crates/.../file/rope/read.rs:368`, `:395`; `objects.rs:2855` |
| 8 | The reference attaches the previous root as the small predecessor in both directions | holds today | `objects.rs:3318`; delivered at `:2836-2840` |
| 9 | The reference's cursor is not attached for a whole-file base | holds today | `objects.rs:3319-3324` |
| 10 | Core lacks the cursor; the planned file was never created | holds today | [§13.7](../08-representations.md#137-how-the-reference-tree-compares-v016); Stage 4 file plan |
| 11 | A usable cross-role base at the transition | **deferred** | declined in the v0.1.7 design, below |
| 12 | Core should consult the cursor per emitted chunk | **proposed** | [chapter 14](../09-delta-hints.md) |

## 4. What would finalize this

1. **Implement the cursor in `core/`** behind the existing four-slot type, with no
   storage-format change — [chapter 14 §14.9](../09-delta-hints.md#149-what-would-finalize-this-paper).
2. **Measure the two arms** on the four cases chapter 14 §14.6 states, one sample
   per case per arm, accounting for the whole retained graph chronologically.
3. **For the cross-role half only:** an owner amendment is required first. The
   v0.1.7 design states the current ruling — *"a first whole-to-chunked conversion
   can reuse identical existing chunks but otherwise stores new chunks as FULL
   without a suitable chunk base. A chunked-to-whole conversion needs an eligible
   whole-file base or selects FULL … no cross-role delta trick is required"*
   ([content-storage-design.md §4](../../../../docs/roadmap/0.1/0.1.7/component-decoupling/content-storage-design.md)),
   and the same boundary is restated for the replacement in
   [file-content.md §7](../../../../docs/roadmap/0.1/0.1.7/component-decoupling/file-content.md)
   (*"at representation transitions, no cross-role base or chain continuity is
   promised"*). The issue100 amendment reaches the same conclusion from the other
   side: *"the crossing remains ineligible in this design … a cross-CDC
   implementation needs a separate concrete amendment after evidence justifies
   it"* ([bounded-predecessor-amendment.md](../../../../docs/roadmap/0.1/0.1.5/issue100/bounded-predecessor-amendment.md)).
   Any such amendment must declare the candidate count (one), the trial count
   (one), the role pair it admits, and the depth/closure/encoded budgets it spends.
4. **Then either promote this paper** to a description of shipped behaviour — in
   which case it moves out of `deferred/` and into the set, with a chapter number —
   **or delete it** and record the rejection here.

Until step 4 this paper stays a proposal, and any number read from it is an
estimate.

## 5. Related documents and upkeep

- [Positional delta hints](../09-delta-hints.md) — chapter 14, the proposal this
  paper's diagrams serve.
- [Representations](../08-representations.md) — §13.1–13.4 the transition matrix
  and its storage economics, §13.7 the reference comparison.
- [Proposal folder](../proposal/README.md) — the label vocabulary reused above.
- Issues: [#185](https://github.com/Ephemeral-AI-Lab/layerfs/issues/185) (this
  paper's deferred tracking issue), [#169](https://github.com/Ephemeral-AI-Lab/layerfs/issues/169)
  (size transitions), [#175](https://github.com/Ephemeral-AI-Lab/layerfs/issues/175)
  (this document set), [#176](https://github.com/Ephemeral-AI-Lab/layerfs/issues/176)
  (complexity and round-trip register), [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160)
  (design).

**Upkeep.** If a source change makes the cursor exist in `core/`, admits a
cross-role base pair, or removes the reference mechanism, this paper changes in the
same commit — or is deleted, with the rejection recorded. Advancing the pin without
a content change is allowed only when the change touches none of those, and must be
said rather than done silently.
