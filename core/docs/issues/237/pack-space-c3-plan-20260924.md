# #237 prospective C3: pooled-lane reuse with exact payload packs

> **Status: Research; informative and not a product contract.** Frozen after
> the single retained C2 result and before a C3 product edit or sample.

## Observed reason for this treatment

The exact-source v0.1.6 Store is 514,879,488 B apparent / 520,110,080 B
allocated. Core's retained control was 554,098,688 B apparent / 558,145,536 B
allocated. C1 saved little; the one C2 release row closed every pack at each
placement flush, passed full readback, and reached 528,216,064 B apparent /
534,118,400 B allocated for Core Store plus History. C2 removed all
35,168,077 B of the control's declared pack tail but still exceeds v0.1.6
by 13,336,576 B apparent / 14,008,320 B allocated. These are same-source
file sizes, not a speed comparison between unlike public routes.

C2 grew from 2,082 to 4,282 packs. The added 2,200 rows consist of 1,985
pooled-metadata v12, 102 native v15, 102 whole-file v17 and 11 ordinary
v9. Used BLOB bytes increased by 8,750,615 B; the v12 portion alone is
8,178,200 B, exactly 1,985 times its 24-B header plus 4,096-B reserved
directory. The pooled lane still has 2,001 groups, so this is pack framing
rather than additional source payload. Control had 16 pooled packs and only
142,988 B of tail in that lane. C2 had 2,001 exact-length pooled packs.

## One product difference and predicted effect

Keep C2's exact closing at each `select_many` flush for Ordinary, Native
and WholeFile payload lanes. For **PooledMetadata only**, retain the C1
cross-flush open pack: its final selected write reserves the ordinary
256-KiB capacity, later pooled groups append in place, and the open state
persists until displaced. This is the previously exercised placement path,
with one bounded open pack in that lane; it changes neither pack version,
header grammar, worker count, public Save boundary, nor SQLite page policy.
The selected pooled pack is still written before its metadata group
catalogue row and any same-Save read. Singleton behavior is unchanged.

If this source produces the control's 16 pooled packs again, the projected
BLOB-capacity reduction against C2 is **8,035,212 B**:
12,229,516 B C2 pooled BLOBs minus 16 × 262,144 B. This is a
source-backed projection, **not** a measured C3 result. It leaves at most
the control's ~142,988 B pooled tail on this fixture, still far below the
C2 plan's 7,033,615-B total tail ceiling. SQLite page and filesystem
allocation savings must be measured; do not infer them from BLOB subtraction.

## One-shot evaluation

Commit the product, external pooled/payload pack tests and physical-writing
architecture together. Use the same sole Core SDK runner and exact SHAKE
100k manifest `23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`
in locked Cargo **release** mode, fresh independent byte copy and fresh
output. Preserve the zero-resident source-payload preflight/recheck; inode
and directory metadata residency remains unqualified. Take **one** C3
candidate call; do not rerun C2, C1, the Core control or v0.1.6. Keep the
15-s complete-command and under-10-s independent verifier expectations,
and report misses rather than changing watchdogs.

The covering tests must show pooled groups from separate flushes sharing
one open pack while reopened readback and same-Save visibility hold, and
payload lanes still closing at exact length. The full 101,001-path/500-MB
independent oracle must pass. Report public-call time, complete command,
CPU/RSS, Store and History apparent/allocated bytes, 4-KiB page/freelist
counts, pack rows and version/lane counts, BLOB capacity/declared used/tail,
and all identities. C3 improves the space treatment only if combined
allocated bytes fall by at least **7,000,000 B** from C2's 534,118,400 B
without a correctness failure or material time/RSS regression. Passing this
threshold does **not** establish parity with v0.1.6; report any remaining
same-source gap. The separate #229 matched sparse-history space/full-readback
guard remains open until specifically proved.
