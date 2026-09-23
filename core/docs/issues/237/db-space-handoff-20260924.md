# #237 database-space handoff after exact-source release comparison

> **Status: Research; informative and not a product contract.** This
> handoff preserves the one-shot receipts and the open acceptance gates.

## Prompt for the next agent

Work from the committed #237 pack-space branch and **read the retained
evidence before editing or sampling**:

1. The [exact-source v0.1.6 release reference](v016-exact-100k-storage-result-20260924.md)
   is a single 3.779070375-s public Init on the same SHAKE 100k/500-MB
   manifest as Core. Its closed Store is 514,879,488 B apparent /
   520,110,080 B allocated, and full 101,001-path readback passed. Its
   complete command and verifier exceeded the repository's 15-s and
   under-10-s expectations. Product `crates` tree equals peeled v0.1.6
   tag; benchmark-only harness is in the isolated
   `issue237-v016-exact-100k` worktree at `61d1eb10d`. The old
   historical #152 row used different bytes and is not the control.
2. Core's retained release control is 554,098,688 B apparent /
   558,145,536 B allocated and has 35,168,077 B reserved pack tail.
   [C1](pack-space-c1-result-20260924.md) saved little.
   [C2](pack-space-c2-result-20260924.md) exact-closed every pack and
   removed the tail but created 1,985 extra pooled packs.
   [C3](pack-space-c3-result-20260924.md) retained pooled reuse while
   exact-closing payload lanes: 520,003,584 B apparent /
   523,399,168 B allocated, full oracle PASS. It is the best retained
   **dense** space row, only 3,289,088 B allocated above v0.1.6.
3. The [C5 result](pack-space-c5-result-20260924.md) trimmed the
   last pooled pack inside each Save: a matched 17-Save release probe
   fell 4,608,000 → 483,328 B allocated with full 17-object readback.
   Its dense 100k Store + History, however, rose to 530,206,720 B
   allocated, missing the frozen C3+1-MB cap. Do not hide that failure.
4. [C6](pack-space-c6-result-20260924.md) keeps C5's sparse trim only
   at ≤half pack occupancy. Its 17-Save release Store is again 483,328 B
   and hash-identical to C5, with full readback. The dense pooled pack
   was **not** trimmed: used 133,675 B >131,072 B; all 16 v12 BLOBs
   match C3. The C6 SQLite Store is 15 pages smaller than C3, but
   filesystem allocated-minus-apparent excess rose 2,482,176 B; combined
   allocation is 525,819,904 B, **1,420,736 B above the frozen limit**.
   The C6 dense rule is FAIL and remains one-shot. Source and closed
   geometry do not causally assign that filesystem excess to the trim.
   Do not resample an unchanged arm, relax the rule or relabel the row.
5. The full [#229 sparse-pack guard](sparse-pack.md) remains open.
   Existing `history-stride10` complete commands take 38–40 s versus
   15/25-s limits; previous matched stride1 runs stopped before a
   state root at `Integrity("dependency encoded work")`. The current
   history verifier samples paths and even an all-declared-path run
   cannot exclude extras. The 17-object Store probe is a mechanism
   diagnostic, **not** an admission substitute.

First, inspect C3/C5/C6 closed Store files **read-only** using the linked
geometry, `dbstat`, page counts, freelist and `st_blocks`; separate
SQLite file length from filesystem allocation slack. No candidate
resample is allowed. Then decide whether a *new*, prospectively frozen
space selection is justified. The highest-value open proof is a
same-policy #229 control/treatment with a completed public history
operation and a verifier that reopens every root and rejects missing
**and extra** paths, without enlarging timeouts or shrinking the corpus.
Diagnose the existing dependency error from its retained receipt and
source before proposing a fix. If that gate cannot run within the
existing bound, report it `NOT_RUN` or `INCOMPLETE`, rather than
claiming C6 accepted.

Only after sparse correctness/space is understood, consider the
remaining dense payload-directory vacancy. C3's exact-closed payload
packs contain 5,182,500 B of unused fixed directory slots. Compacting
them would require distinct pack versions and backward reader support,
because v9/v15/v17 derive fixed body offsets. It is a format change
for a 3.29-MB measured allocated gap; do not make it speculatively.
Any new treatment must freeze an exact source/build/fixture identity,
one sample per arm, release binaries, zero-resident payload pages,
complete readback, file/page/pack geometry, CPU/RSS and all nonpassing
rows before it runs.

## Custody

Curated JSON/text receipts live beside each linked result. Complete
raw Stores, source copies and hash manifests remain in the worktree-local
`benchmark-results/` directories named by those reports. The active
Core branch preserves all C1/C2/C3/C5/C6 commits and does not claim a
release or fully cold admission PASS. The benchmark-only hex formatting
cleanup at `f43c10db2` happened **after** the three matched sparse
probe receipts; their shared harness SHA remains
`0ff00e0b259e2d71020528de0199f6182a496c0d36164a9d588dd5ffc589094f`.
