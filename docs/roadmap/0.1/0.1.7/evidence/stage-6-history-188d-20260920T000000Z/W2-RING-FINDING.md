# The ring-size sweep E2 never ran — and it corrects R1a

**Reported by W2, measured.** Recorded here immediately because it is a direct hit on this round's
mandate (*bounded memory at no arithmetic cost*) and because it **corrects a change already in the tree**.

## The finding

The per-save similarity index is **self-limiting**: an object enters it **only when no candidate was found
for it**. On the retained-history lane that means it **saturates at 7,614 entries**, so the ring **never
binds above 8,192 slots**. Measured coverage over the whole-file objects:

@```text
  ring   1,024  refs 65,536   20,995 objects
  ring   4,096  refs 65,536   34,494
  ring   8,192  refs 65,536   36,544    <- the knee
  ring  32,768  refs 65,536   36,544    <- what is in the tree today
  unbounded     refs 65,536   36,544
@```

**Coverage is identical from 8,192 upward.** So today's @SLOTS = 32,768@ — which the T1 squad set under
ruling A — holds **24,576 slots that buy exactly zero coverage**, about **2.25 MiB of resident memory for
nothing**.

W2 is shipping **8,192 slots plus a 32-bit hash fold**, reported byte-identical in coverage on all 44,148
objects, for a **declared bound of 704 KiB instead of 4 MiB**.

## Why this is exactly what the mandate asks for

@```text
  before   SLOTS 32,768, 64-bit signatures   ~4 MiB resident   coverage 36,544
  after    SLOTS  8,192, 32-bit fold        704 KiB resident   coverage 36,544
  --------------------------------------------------------------------------
  memory  -3.3 MiB   (-83 %)                 bytes saved  0
@```

**The same storage, 83 % less resident memory.** It is a pure win and it was invisible until the sweep was
actually run — E2 measured only the two endpoints (1,024 slots and unbounded) and nobody had swept between
them.

## This is the campaign's pattern, again

It is the **fifth** time a plausible inference has been corrected by a measurement: the Stage 7 diagnosis,
the "empty advisory list", the "defective encoder", the four-slot ordering, and now the ring size. In every
case the fix was the same — run the sweep, read the counter.

## Still to be confirmed (asked of W2)

1. The **saturation mechanism** stated as a measured claim, naming the counter that governs admission —
   not just the observed plateau.
2. The **memory arithmetic** spelled out: entries x bytes-per-entry at 8,192 and at 32,768, and the fold's
   effect on the per-entry width.
3. **Both arms**: the similarity source ON changes the admission pattern, so the 7,614 saturation and the
   8,192 knee must be confirmed in the declared-only arm AND the similarity-on arm, with the sizing arm
   named.

## A live sequencing hazard, resolved

W2 and W3 were both editing @sql/schema.sql@, @policy.rs@ and @sqlite/schema.rs@. W3 landed the
@base_object_id@ removal and took @SCHEMA_VERSION = 5@; W2 appended its own table and took @6@. **Without two
entries in @sqlite/schema.rs@, @Store::create@ fails with @Integrity("unexpected table in Store")@ and every
lane run fails.** W3 owns that file and has been directed to add them; W2 has been told to stay out of it.
Recorded because it is exactly the kind of silent cross-squad breakage that costs an hour to diagnose.
