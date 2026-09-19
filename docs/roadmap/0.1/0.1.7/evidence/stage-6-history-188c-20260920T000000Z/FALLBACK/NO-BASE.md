# Why do objects have no base — and does v0.1.6 have the same problem?

**Measured.** Both Stores hold **44,141 whole-file objects / 348,460,295 B canonical — identical.**

## D1 · Why an object is offered no base

````text
  the object's PATH has no earlier version inside the 17-state selection
      - the file is NEW between two selected checkpoints
      - the file was RENAMED   (delete at the old path + add at the new)
      - the file was REMOVED and re-ADDED
                |
                v
  the CALLER declares nothing for it  (there is no previous version to name)
                |
                v
  the only other source is the STORE's content-similarity cache
````

**This is not a defect and not a failure.** It is the normal situation for a new or renamed path, and
the selector handles it by policy: no candidate -> FULL, counted in `delta.no_candidate`.

## D2 · The one thing that differs between the generations

````text
   v0.1.6                                      core, as it stands
   +---------------------------------+         +-----------------------------------+
   | content-similarity cache        |         | content-similarity cache          |
   |                                 |         |                                   |
   | PERSISTS ACROSS SAVES           |         | OWNED BY ONE SAVE                 |
   | schema.rs:77, on StoreInner     |         | lifecycle.rs:90 -> store.rs:392   |
   | asserted by                     |         |                                   |
   | small_candidate_tests.rs:256    |         |                                   |
   +---------------------------------+         +-----------------------------------+
   consulted when the declared                 consulted when the advisory list
   predecessor is ABSENT                       yields nothing
                |                                          |
                v                                          v
   finds a CROSS-PATH base                     can only match within the SAME save
   from anywhere in its history                -> finds NOTHING across saves
````

**That is the entire difference: the lifetime of one cache.** Not the declaration rule, not the object
set, not the codec.

## D3 · The count — and it is the opposite of what you would expect

````text
                         objects   with base   NO base    base-less canonical     stored
   v0.1.6                  44,141      35,904     8,237            62,974,073   22,672,881
   ours, fallback OFF      44,141      34,439     9,702            75,113,968   27,809,279
   ours, fallback ON       44,141      37,797     6,344            51,548,521   18,839,086
````

**It DOES happen to v0.1.6 — 8,237 times.**

- With the fallback **off**, we have **9,702** base-less objects: **1,465 MORE than v0.1.6**, carrying
  75,113,968 B canonical against its 62,974,073 B.
- With the fallback **on**, we have **6,344**: **1,893 FEWER than v0.1.6**, carrying 51,548,521 B against
  its 62,974,073 B.

**So v0.1.6 does not avoid the problem — it has it more than we do once the fallback is on.** The
fallback is not rescuing us from something v0.1.6 escapes; **it is what puts us ahead of v0.1.6 on this
exact axis.**

## D4 · One line

````text
   "objects with no base"  =  new and renamed paths. Both generations have them.
   v0.1.6 has 8,237. We have 6,344 with the fallback, 9,702 without.

   the difference is not a rule or a capability we lack —
   it is that v0.1.6's similarity cache outlives one save and core's does not.
````

**And that is precisely why the fallback arm is harness-supplied**: the harness maintains the
cross-save index that core's per-save `Candidates` cannot be. Building it in the product is the
legitimate way to have it — a persisted, cross-save content index.
