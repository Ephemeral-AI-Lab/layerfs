# Init, commit and concurrency — operations against the Store

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
>
> **DRAFT — NOT FINALIZED.** Nothing in this document is implemented or measured.
> §1–§4 and the parts marked *read from source* describe code that exists. §5–§10
> describe a concurrency design that does **not** exist and would require the
> changes listed in §8. Every "safe" claim is scoped to the configuration it is
> stated under.

Parent: [`core/docs/architecture/`](../README.md). Source pin `ce2d738ff`.

**Current sequencing:** this is pair 2's operational design input. Under the
[2026-09-20 implementation order](README.md#implementation-order-pair-3-then-pair-1-then-pair-2),
pair 3's service/transport is implemented first, pair 1's Workspace/FUSE second,
and pair 2's history third. Early acknowledgement/allocation definitions inform
the service contract; they do not require parallel implementation. The older
concurrency sketches below remain proposals subject to the owner's no-retry,
no-automatic-rebase and no-added-durability rules, not implementation instructions.

---

## 0. Why this document exists

Two operations write to the Store — **init namespace** and **workspace commit** —
and both end, in different ways, at the same history. Everything else (FUSE
traffic, file edits, reads) happens against a local overlay and never touches the
database.

This document states the pipeline, the exact DB operations at each phase, and what
happens to safety when more than one commit runs at once.

---

## 1. The two entry operations

```text
                        INIT NAMESPACE              WORKSPACE COMMIT
   ────────────────     ──────────────────────      ─────────────────────────
   source               a real directory tree,      FUSE overlay diffs
                        or Empty
   base                 None  → build_filesystem    Some → update_filesystem
   workers              MULTI — documented          SINGLE — mandatory
                        exception, 2.7 s target
   frequency            once per namespace           many per workspace
   ends at              HISTORY (creates it)         STAGED (waits for merge)
```

The worker difference is an explicit ruling and not an implementation detail:

> **One construction worker — for every case except namespace init.** Commit,
> capture and snapshot run with a single worker […] **`init_namespace` is the only
> exception:** its initialization path […] legitimately uses multiple
> workers/threads and keeps its **2.7 s cold Init** target — do not collapse it to
> one.

**What they share** — and should be built once:

```text
   INIT NAMESPACE                                WORKSPACE COMMIT
        │                                                │
        └─────────────────┬──────────────────────────────┘
                          ▼
        ┌──────────── SHARED PIPELINE (build once) ────────────┐
        │  ① construct_stream per file      (§3 phase ①)       │
        │  ② attribute roots: mode / mtime                     │
        │  ③ FilesystemInput — sorted, unique, complete        │
        │  ④ save in bounded chunks                            │
        └──────────────────────┬───────────────────────────────┘
                               │
              init: initial layer + LayerStackId
              commit: a `workspace_stages` row
```

**One producer, two front ends — with two different concurrency policies.**

---

## 2. The five phases

```text
   ① PREPARE        concurrent per commit · PRIVATE state
        membership lookup · delta base acquisition · encode · group assembly

   ② COMMIT POINT   SQLite: ONE writer, ONE transaction
        allocate pack ids · write packs · insert object rows · insert metadata

   ③ PUBLISH        the watermark advances — one write, global, monotone

   ④ STAGE          per-WORKSPACE row  (commit)   │   CREATE  (init)

   ⑤ MERGE          per-BRANCH CAS     (commit)   │   implicit (init)
```

**② and ③ are the only places that require the single writer.** ① is parallel,
④ touches a row nobody else shares, ⑤ is a single-row compare-and-swap.

---

## 3. The full path

```text
╔══════════════════════════════════════════════════════════════════════════════╗
║ WORKSPACE  (consumer — FUSE, any environment)                                ║
║                                                                              ║
║    FUSE mount ──►  OVERLAY + SNAPSHOT                                        ║
║                      ▲                                                       ║
║                      └── EVERY write lands here. NO database on this path.   ║
║                                                                              ║
║    read:  overlay hit ──► serve locally                                      ║
║           overlay miss ─► snapshot (one DB read wave)                        ║
║                                                                              ║
║    on COMMIT: build content objects (C1, local CPU) + FilesystemInput        ║
╚═══════════════════════════════════╤══════════════════════════════════════════╝
                                    │ transport: logical ops + object batches
╔═══════════════════════════════════▼══════════════════════════════════════════╗
║ OWNER                                                                        ║
║                                                                              ║
║  ┌─ ① PREPARE ─── concurrent · PRIVATE ────────────────────────────────────┐ ║
║  │    membership lookup      reads, filtered by the watermark ceiling      │ ║
║  │    delta base acquire     reads                                         │ ║
║  │    encode FULL / trial    CPU                                           │ ║
║  │    assemble group bodies  CPU                                           │ ║
║  │                                                                         │ ║
║  │    [PRIVATE]  Candidates · DepthCache · LanePlacement                   │ ║
║  │    a stale or partial cache ⇒ a MISSED delta, never a wrong answer      │ ║
║  └─────────────────────────────────┬───────────────────────────────────────┘ ║
║                                    │                                         ║
║  ┌─ ② COMMIT POINT ─── ONE writer, ONE transaction ───────────────────────┐ ║
║  │    1  SELECT MAX(pack_id)          allocate private pack ids            │ ║
║  │    2  INSERT / UPDATE object_packs write pack BLOBs                     │ ║
║  │    3  INSERT objects               ◄── PK CONFLICT ⇒ REUSE              │ ║
║  │    4  INSERT metadata_value_groups pooled rows                          │ ║
║  │                                                                         │ ║
║  │    Availability::validate every `reference`:                            │ ║
║  │       each must be already published, or PENDING IN THIS WAVE           │ ║
║  │       ⇒ CHILD BEFORE PARENT, enforced not assumed                       │ ║
║  └─────────────────────────────────┬───────────────────────────────────────┘ ║
║                                    │                                         ║
║  ┌─ ③ PUBLISH ────────────────────────────────────────────────────────────┐ ║
║  │    UPDATE store_policy SET retained_pack_ceiling = …                    │ ║
║  │    ═════ THE ONLY MUTABLE VALUE IN THE STORE ═════                      │ ║
║  │    monotone · global · forward-only                                     │ ║
║  └─────────────────────────────────┬───────────────────────────────────────┘ ║
║                                    │                                         ║
║  ┌─ ④ STAGE ──────────────────────────────────────────────────────────────┐ ║
║  │    workspace_stages { workspace_id PK, branch_id, root_id }             │ ║
║  │    ═════ per-WORKSPACE row · ZERO branch contention ═════                │ ║
║  └─────────────────────────────────┬───────────────────────────────────────┘ ║
╚════════════════════════════════════╪═════════════════════════════════════════╝
                                     │
╔════════════════════════════════════▼═════════════════════════════════════════╗
║ ⑤ MERGE  (separate operation · per BRANCH · O(1) — see §9)                   ║
║                                                                              ║
║     SELECT   load_add_snapshot(branch_id)                                    ║
║                                                                              ║
║     check    UpToDate   → already merged, no-op                              ║
║              HeadMoved  → commit base ≠ branch base     ◄── REFUSED          ║
║              NoChanges  → commit root = base root                            ║
║                                                                              ║
║     INSERT   layers { root_id = commit_root_id }   ◄── SAME OBJECT ID,       ║
║                                                         no copy              ║
║     UPDATE   layer_stacks SET head_layer_id = ?2                             ║
║               WHERE layer_stack_id = ?1 AND head_layer_id = ?3               ║
║     ═════ THE CAS: one winner · zero rows ⇒ HeadMoved ═════                  ║
╚══════════════════════════════════════════════════════════════════════════════╝
```

---

## 4. Every DB operation, by phase

```text
   TABLE                     OPERATION                    PHASE   CONTENTION
   ─────                     ─────────                    ─────   ──────────
   objects                   SELECT  (membership)         ①      none (read)
   objects                   INSERT                       ②      per OBJECT_ID
   object_packs              SELECT MAX(pack_id)          ②      — (in txn)
   object_packs              INSERT / UPDATE              ②      per PACK_ID
   metadata_value_groups     INSERT                       ②      per GROUP
   store_policy              UPDATE ceiling               ③      GLOBAL, serial
   workspace_stages          INSERT / UPDATE              ④      per WORKSPACE
   commits                   INSERT                      ⑤      per BRANCH
   layers                    INSERT                      ⑤      per BRANCH
   layer_stacks              UPDATE … WHERE expected     ⑤      per BRANCH (CAS)
```

**Read from source, and load-bearing:** there is **no `UPDATE objects`** anywhere in
the crate. The only two `UPDATE` statements in the entire storage layer are the
pack append and the watermark. Objects are immutable; packs are append-only.

---

## 5. The concurrency substrate

```text
   SHARED — must be exact                 PRIVATE — approximation is CORRECT
   ──────────────────────                 ────────────────────────────────
   the schema                             Candidates    (128 KiB, per writer)
   the watermark (③)                      DepthCache    (4,096 entries)
   object rows / locators                 LanePlacement (open packs)
   pack rows
   workspace_stages / layers / commits
                                          a private cache can only cause a
                                          MISSED delta ⇒ a FULL record instead
                                          of a PREFIX. Bigger. Still correct.
```

**This is the property that makes concurrency affordable.** Dedup and delta are
*optimisations*, not correctness: identity is content-derived and re-verified on
every read, so a writer with a stale view produces bigger objects, never wrong
ones.

---

## 6. The three concurrency cases

### 6.1 Case 1 — two concurrent commits from the **same branch**

```text
   W1 ──► ①prepare ──► ②objects ──► ③publish ──► ④stage(W1) ──┐
   W2 ──► ①prepare ──► ②objects ──► ③publish ──► ④stage(W2) ──┤
                                                              │
   ┌──────────────────────────────────────────────────────────┘
   │
   ├─ ④ DIFFERENT ROWS       workspace_id is the PRIMARY KEY
   │                          ⇒ ZERO contention at staging
   │
   ├─ ② MAY COLLIDE          identical content ⇒ same object_id
   │                          ⇒ PK conflict ⇒ REUSE ⇒ loser's pack bytes
   │                            become unreferenced garbage  (space, not safety)
   │
   ├─ ③ SERIALIZES           the watermark is one global counter;
   │                          each save holds a short transaction
   │
   ▼
   W1 merge ──► CAS(expected = L0) ──► head = L1        ✓ WINNER
   W2 merge ──► CAS(expected = L0) ──► 0 rows           ✗ HeadMoved
```

```text
   BEFORE                                    AFTER
   head = L0                                 head = L1
   W1 commit root ⊃ base L0                  W2's staged root ⊃ base L0
   W2 commit root ⊃ base L0                  ⇒ W2 is STALE. Its tree does not
                                               contain W1's changes.
                                             ⇒ promoting it would REVERT W1.
                                               Hence: REFUSED, not queued. (§9)
```

**Safe because:** the two commits write no shared row except on identical content
(where reuse is byte-identical by construction), and the two merges are arbitrated
by one atomic compare-and-swap.

**The failure is `HeadMoved`, not corruption.** W2's objects are stored and stay
stored; only the pointer move is refused.

### 6.2 Case 2 — two concurrent commits from **different branches**

```text
   W1 ──► ①──► ②──► ③──► ④stage(W1) ──► MERGE(B1)
                                          UPDATE layer_stacks
                                           WHERE layer_stack_id = <B1 row>
   W2 ──► ①──► ②──► ③──► ④stage(W2) ──► MERGE(B2)
                                          UPDATE layer_stacks
                                           WHERE layer_stack_id = <B2 row>
                                                ║
                              ══ DIFFERENT ROWS ⇒ ZERO CONTENTION ══

   the ONLY shared structures:
     objects / object_packs  ── content-addressed ⇒ reuse, never conflict
     the watermark           ── monotone ⇒ forward-only, cannot be corrupted
```

**Safe because there is no shared mutable state to race on.** Everything
branch-level is a different row.

### 6.3 Case 3 — two concurrent commits from **different projects**, one Store

```text
   PROJECT P1 ──► workspace W1 ──► branch B1 ──► scope S1
   PROJECT P2 ──► workspace W2 ──► branch B2 ──► scope S2

   ONE STORE
   ┌──────────────────────────────────────────────────────────────────────────┐
   │ objects                SHARED ─ content-addressed ⇒ IDENTICAL FILES       │
   │                                 ACROSS PROJECTS DEDUPE  ← the point       │
   │ object_packs           SHARED ─ append-only                               │
   │ metadata_value_groups  SHARED ─ pooled, content-keyed                     │
   │ store_policy           SHARED ─ ONE watermark, monotone                   │
   │ workspace_stages       PER WORKSPACE                                      │
   │ layers / commits       PER BRANCH                                         │
   │ layer_stacks           PER BRANCH (the CAS target)                        │
   └──────────────────────────────────────────────────────────────────────────┘
```

**What dedupes across projects, and what does not — by construction:**

| Object role | Carries scope? | Dedupes across projects? |
| --- | --- | --- |
| `Chunk` | no | **yes** |
| `WholeFile` | no | **yes** |
| `ExtentLeaf` / `ExtentBranch` | no — references only | **yes** |
| `FileState` | no — profile + mapping root | **yes** |
| `AttributeLeaf` | no | **yes** (shared mode/mtime) |
| `DirectoryLeaf` / `InodeLeaf` | rows carry a **serial**, per-scope allocated | only if serials coincide |
| `FilesystemRoot` | **yes — carries `InodeScope`** | **no** |

```text
   THE SEPARATION THAT MAKES ONE STORE CORRECT FOR MANY PROJECTS

   CONTENT (expensive)              NAMESPACE (cheap)
   chunks · whole files             directory pages · inode pages · root
   extent trees · file states
        │                                 │
        └── NO scope ⇒ identical          └── scope-tagged ⇒ two projects'
            bytes are ONE object              roots NEVER spuriously collide
        ⇒ cross-project dedup             ⇒ no false sharing of tree state
```

**Safe because** the only cross-project shared mutable value is the watermark, and
it can only move forward. Nothing one project writes can make another project's
committed content unreadable or different.

### 6.4 The safety invariants

| Hazard | Why it cannot happen | Status |
| --- | --- | --- |
| A read returns wrong bytes | id = `BLAKE3(canonical)`; reuse **byte-compares** rather than trusting | **holds** |
| A lost race yields a wrong row | PK conflict ⇒ the winner's row stands; byte-identical by construction | needs §8.1 |
| A parent is stored without its child | `Availability::validate` ⇒ `MissingDependency` | **holds** |
| A commit corrupts another branch | objects are immutable — no `UPDATE objects` exists | **holds** |
| A merge loses an update | the CAS: one winner, zero rows ⇒ refused | **holds** |
| A project corrupts another project | trees are scope-tagged; content is immutable | **holds** |
| A read sees a half-written save | the watermark advances only at ③, after ② commits | **holds only while single-writer** |

That last row is the one that constrains any change: it is true *because of the
prefix watermark*, and it is exactly what §8.3 must preserve.

---

## 7. What concurrency costs

You have accepted **duplicated bytes** for identical concurrent submissions. The
exact shape:

```text
   WRITER A                                WRITER B
   prepares object X                       prepares the SAME object X
   encodes into pack 5                     encodes into pack 9
   INSERT row X → (5, 2, 0)                INSERT row X → (9, 1, 3)
                                           └─ PK CONFLICT ─► re-read ─► REUSE

   RESULT
     objects:  ONE row, pointing at pack 5
     pack 9:   holds X's bytes, referenced by NOTHING
     cost:     wasted pack space + B's wasted encode
     safety:   unaffected — nothing reads the garbage
```

Bounded per race: **at most one pack's worth of unreferenced bytes.** And it only
happens for **identical** content — divergent edits produce different ids and never
race at all.

**No GC exists**, so this accumulates. It is one of three sources of unreferenced
bytes in this design, alongside representation transitions
([§13.3](../08-representations.md)) and chunked-import intermediate roots.

---

## 8. The three open races — prerequisites, not options

### 8.1 Insert-or-reuse

```rust
// TODAY — sqlite/write.rs
let affected = connection.execute("INSERT INTO objects (…) VALUES (?1, …, ?7)", …)?;
if affected != 1 { return Err(StorageError::Integrity("object insert cardinality")); }
```

A PK conflict surfaces as a **failure**, so a lost race fails the save. It must
become: conflict ⇒ re-read the winner's row ⇒ reuse ⇒ count as `reused`.

**No schema change.** Required for §6.1 and §6.3 to be safe rather than merely
described.

### 8.2 Private packs per writer

`LanePlacement.open` is one open pack per lane, mutated in place. Two writers
sharing it interleave groups into one pack, and can clash on
`objects_locations UNIQUE(pack_id, group_number, record_number)`.

**Private packs per writer removes both** — each writer owns its ids, packs and
ordinals. **No schema change.**

### 8.3 Publication order — per-**save**, not per-pack

**This is the one that breaks correctness, and the one I initially got wrong.**

```text
   SAVE A admits X, and X references Y. Both are in A's wave.
   Availability::validate(X) passes because Y is PENDING IN THIS WAVE.

   NOW PUBLISH A's PACKS ONE AT A TIME:

      pack 1 (holds X) published ──► X VISIBLE
      pack 2 (holds Y) not yet   ──► Y NOT VISIBLE

      a reader resolving X follows its reference to Y  ──► MISSING   ✗
```

```text
   THE PREFIX WATERMARK          ┌─────────────────────────────────────────┐
   today: pack_id <= ceiling     │ one integer covers a save's whole range  │
                                 │ ⇒ SAME-SAVE ATOMICITY IS FREE            │
                                 │ ⇒ but publication order must equal       │
                                 │   pack-id order ⇒ a slow save blocks     │
                                 └─────────────────────────────────────────┘

   PER-PACK FLAG                 ┌─────────────────────────────────────────┐
   (WRONG)                       │ breaks the dependency invariant above   │
                                 └─────────────────────────────────────────┘

   PER-SAVE PUBLICATION          ┌─────────────────────────────────────────┐
   (CORRECT)                     │ a save's packs publish in ONE            │
                                 │ transaction ⇒ atomicity EXPLICIT         │
                                 │ ⇒ arbitrary publication order ⇒ no block │
                                 │ ⇒ the read predicate gets more expensive │
                                 └─────────────────────────────────────────┘
```

**Schema change required** — a save identity on packs, and the reader filters on
published saves rather than on a prefix. **This is the only one of the three that
touches the schema**, and it is a correctness prerequisite, not an optimisation.

---

## 9. The merge: why it rejects, and why a queue is not the fix

### 9.1 The merge is O(1) — read from source

```rust
let layer = LayerRecord {
    id: LayerId::derive(layer_stack_id, Some(parent_layer_id), commit_root_id),
    parent_layer_id: Some(snapshot.branch_base_layer_id),
    root_id: snapshot.commit_root_id,            // ◄── REUSED VERBATIM
    …
};
INSERT INTO layers(…)                             // 1 row
UPDATE layer_stacks SET head_layer_id = ?2 WHERE … AND head_layer_id = ?3   // CAS
```

```text
   THE ENTIRE MERGE
   ────────────────
   1 SELECT   load_add_snapshot
   1 INSERT   layers       root_id IS the commit's root_id — no copy
   1 UPDATE   the CAS      one row
   ─────────────────────────────────────────────────────
   zero objects created · zero objects read · zero bytes copied
   LayerId is DERIVED, not computed over content
```

**Staging already did everything expensive.** Construction, chunking, hashing,
encoding, placement, object rows — all at commit time. **The per-branch "writer"
costs about three row operations and is not a throughput concern.**

### 9.2 A queue without rebase is *data loss*

```text
   L0 = a.txt(v1), b.txt(v1)                       branch head

   W1 stages a.txt(v2) ──► merge ──► head = L1 = a.txt(v2), b.txt(v1)
   W2 stages b.txt(v2) ──► merge ──► base (L0) ≠ head (L1)  ⇒ HeadMoved

   IF A QUEUE "JUST APPLIED IN ORDER":

      apply W2's commit
        new layer root_id = W2's root
        W2's root was computed AGAINST L0 ⇒ a.txt(v1), b.txt(v2)

      RESULT   head = a.txt(v1), b.txt(v2)
               ══════ W1's a.txt(v2) IS SILENTLY GONE ══════
```

**A stale commit reverted by time is still stale.** The staleness check is not
conservatism — it is what prevents silent data loss.

### 9.3 Where the queue belongs

```text
   ✗ STORE-SIDE QUEUE          "apply merges in arrival order"
                               ⇒ silent reverts.  WRONG.

   ✓ RUNTIME-SIDE QUEUE        for each pending merge:
                                   1. re-read the branch head
                                   2. REBASE this workspace's edits onto it
                                   3. CAS merge; on HeadMoved, loop with backoff

   ⇒ the store CANNOT do this: it holds a staged ROOT, not the EDITS.
     Rebasing requires the caller's edit stream, which lives in the consumer.
```

**The unit of work is "rebase, then merge" — never "merge".** That is why a merge
queue is a runtime feature, not a storage one.

---

## 10. Races versus conflicts

```text
   RACES                                    CONFLICTS
   ─────                                    ─────────
   PK conflict on identical content          the branch head moved
   shared placement state                    two changes to the same region
   publication order vs the prefix watermark
   ──────────────────────────────           ──────────────────────────────
   BOUNDED · IDENTIFIABLE                   REQUIRES JUDGEMENT
   each has ONE correct fix                 what counts as a conflict?
   ⇒ engineering                            who wins? how is it reported?
   ⇒ prerequisites                          ⇒ product decision
```

**Both directions of the current design are wrong, and both stem from one missing
piece — there is no diff:**

```text
   STALENESS (the CAS)                     DIVERGENCE (nothing)
   ───────────────────                     ────────────────────
   detects: head moved                     detects: NOTHING
   ✓ never corrupts                        ✗ a rebase silently resolves two
                                             overlapping edits
   FALSE POSITIVE:                         FALSE NEGATIVE:
     W1 changed a.txt, W2 changed b.txt      both changed a.txt lines 40–60
     ⇒ NO real conflict, yet REFUSED         ⇒ rebase overwrites one, SILENTLY
```

**This is the substantive problem** — not the races, which are mechanical. It is
also exactly what #164 defers:

> Owner decision, 2026-09-16: **omit the proposed Comparison/Reconciliation
> component and its user-facing workflows from the v0.1.7 replacement.**

**Known semantic gap to document for callers:** a rebase resolves overlapping edits
**silently**. The result is deterministic and readable — it may simply not be what
was intended, and nothing reports it.

---

## 11. Deferred and open

```text
   v0.1.7 SCOPE — deferred to #164 (v0.2.0)
     logical diff between roots · three-way reconciliation
     structured conflict evidence · conflict-resolution APIs

   PREREQUISITES FOR CONCURRENCY (§8)
     8.1 insert-or-reuse                  no schema change
     8.2 private packs per writer         no schema change
     8.3 per-save publication             SCHEMA CHANGE — correctness

   MEASUREMENT — unknown, and it decides the design
     ✗ save hold duration                 the blocking unit's size
     ✗ commit rate at target branch count whether one writer suffices
     ✗ pack-byte share of the hold        everything stays in SQLite, so this
                                          is the dominant, immovable term
     ✗ lost-race frequency                sizes the wasted-bytes accumulation

   UNRESOLVED, NOT IN SCOPE HERE
     ✗ no GC — three sources of unreferenced bytes accumulate
     ✗ the 4,096-binding walk ceiling — a directory over it can never be renamed
     ✗ cache pooling — the reference pools Candidates/DepthCache at store level;
       core creates them per save, so short saves currently cost delta quality
```

---

## 12. Decision summary

| # | Decision | State |
| --- | --- | --- |
| 1 | Everything in SQLite; packs not externalised | **decided** |
| 2 | Duplicated bytes accepted for identical concurrent submissions | **decided** |
| 3 | Objects immutable, packs append-only, one mutable value | **holds today** |
| 4 | Commit stages; merge is a separate per-branch CAS | **holds today** |
| 5 | Per-branch serialization costs ~3 row operations | **verified** |
| 6 | Init namespace is the multi-worker exception, 2.7 s cold Init target | **holds today** |
| 7 | Reject stale merges; never queue-and-apply | **holds today** |
| 8 | Merge queue belongs in the runtime, unit = rebase + merge | **proposed** |
| 9 | Insert-or-reuse (§8.1) | **open — required** |
| 10 | Private packs (§8.2) | **open — required** |
| 11 | Per-**save** publication (§8.3) | **open — required, schema** |
| 12 | Divergence detection | **deferred to #164** |
