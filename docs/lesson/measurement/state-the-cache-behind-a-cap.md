# State the cache behind a cap

> **Status:** Lesson; distilled from merged work, informative and not a product contract.
>
> **Category:** measurement · **Applies when:** a target, cap or ceiling is set or judged · **From:** [#816](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816), [#810](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810)

## Rule

Record the cache state and counting method with every target, and judge a result
only against a target counted the same way.

**In short:** a cap is a number plus the conditions it was counted under. Without
the conditions, nobody can tell whether a later result passed or missed it.

## What happened

#816 set whole-Commit caps of 40 PostgreSQL round trips for B5e (overwrite 1,000
random files) and C1 (mixed edits). The caps came from earlier counts in which
v2 measured 52 and 37. Those runs used the old warm-Workspace setup, a warm-up
Commit and then the edits, and never checked cache residency. The final runs
used a checked-cold harness instead. All numbers below are measured, one sample
per arm, on local PostgreSQL.

| Cell | Arm | Warm, unchecked: total | Cold: writer | Cold: readers | Cold: total |
| --- | --- | ---: | ---: | ---: | ---: |
| B5e | v2 `7e6caa86b2` | 52 | 64 | 26 | 90 |
| B5e | Stage 2 | 28 | 28 | 25 | 53 |
| C1 | v2 `7e6caa86b2` | 37 | 50 | 32 | 82 |
| C1 | Stage 2 | 27 | 27 | 31 | 58 |

The warm runs were not split by role. Stage 2's warm totals equal its cold
writer counts, so what the warm caches hid was the reader trips. Stage 2 cut the
writer trips by about half and left the reader trips where they were. Under the
cold harness, v2 itself missed the cap by more than double.

**What "warm" and "cold" mean here.** A Commit touches four caches:

- **Workspace base caches.** `CanonicalCache` in
  `engine/crates/layerfs-workspace/src/base/cache.rs` holds authenticated base
  objects, and its `DecodedPages` (`core/crates/layerfs-content/src/filesystem/decoded.rs`)
  holds decoded inode and directory pages. A hit here means no PostgreSQL
  statement is sent. These are client-side, in the Commit's own process.
- **Overlay file pages.** The Workspace's local Overlay SQLite file, in the
  SQLite pager and the OS page cache.
- **PostgreSQL shared buffers** of the Store's database.
- **OS page cache** under PostgreSQL's relation files.

Round-trip counts change only with the client-side caches; times change with all
four. In the warm setup, the warm-up Commit and the edits left base pages in the
Workspace caches, so the Workspace readers fetched almost nothing.

**How the cold harness enforced its state.** Before the measured Commit, the
harness (untracked, kept with the receipts) did four things:

1. emptied the Workspace base caches through harness-only hooks patched onto
   `CanonicalCache` and `DecodedPages`, not product code;
2. released the Overlay connection's SQLite pager cache, then evicted the Overlay
   file's pages with `posix_fadvise(DONTNEED)` and checked zero residency with
   `mincore`;
3. ran `CHECKPOINT` and `pg_buffercache_evict` on every shared buffer of the
   fixture database, and asserted zero left with the product sessions still idle;
4. evicted and checked the guest file pages of that database's relations.

The run logs record each check (`LFS_FILE_RESIDENCY`, `LFS_PG_BOUNDARY`).

**How close it came.** The handback reported that the cold B5e and C1 totals
exceeded their limits of 40. The follow-up issue
[#824](https://github.com/Ephemeral-AI-Lab/layerfs/issues/824) then listed cold
reader batching as the work that "would bring cold B5e and C1 under 40". That is
new work aimed at trips the change never touched, to meet caps counted another
way. Once the counts were made comparable, the owner waived both caps, and #824
was closed as not planned.

### A second case: B2 in #810

#810 set B2 (delete `node_modules`) ceilings of 100 trips and 0.5 s on
PostgreSQL, and 0.81 s on SQLite. The 0.5 s rested on an estimate of 0.3–0.4 s.
At the v2 tip `056e920166`, cold, B2 took 114 trips and 1,075 ms: a miss on both.

Two steps turned the miss into a finding:

- **Split by phase.** Construction took 92 of the 114 trips, and its update step
  took 955 ms. Deleting 11,439 entries reads the whole removed subtree: 1,417
  directory pages and 1,649 objects in 33 waves, to write 4 objects.
- **Run the control arm under the same state.** v2 `7e6caa86b2`, same harness and
  cold boundary, missed too: 137 trips (construction 105), 981 ms, update step
  844 ms. SQLite, with no network, took 924 ms on v2 and 904 ms at the tip.

So the miss predated the change, and the time was local work above any
PostgreSQL ceiling. The owner waived B2. The trip pattern is the one in
[Round trips follow depth](../store-access/round-trips-follow-depth.md).

## How to spot it

- A target written as a bare number, with no cache state, harness, source or
  roles beside it, or copied from a table that mixes setups.
- A control arm that lands on the other side of the target under the new
  harness, as C1 did: 37 warm, 82 cold.
- A miss that sits entirely in a role or phase the change did not touch, such as
  reader trips after a writer-side change.
- The same cell giving very different counts at nearby sources with different
  setups. B2 read 36 trips at `8135875a19` (cache unchecked) and 114 cold at the
  tip.
- A target derived from an estimate, or below what the same work costs with no
  network at all (B2's SQLite time against its PostgreSQL ceiling).

## What to do

- Record each cap with the block below, and keep it with the cap in the issue.
- Before judging a result, measure the control arm under the same state, in the
  same harness. If it misses the cap by far, the cap was counted another way: say
  so and ask for the cap to be restated, instead of reporting a regression.
- Report trips by role and by phase, so a miss points at the part that moved.

```text
Cap:            <metric> <= <value>            e.g. PostgreSQL round trips per Commit <= 40
Cache state:    <warm | cold | unchecked>; enforced by <which caches, how, how checked>
Harness:        <harness and its hash>; source <commit>; binary <hash>
Counted:        roles <writer, readers>; phases <all, or which>
Backend:        <PostgreSQL | SQLite>, <local | +N ms added RTT>
Control arm:    <source> = <value> under the same state and harness
Samples:        one per arm; host load <1-min load> (recorded, not a reason to rerun)
```

## Limits

- This lesson is about comparing counts. Which cache state may support a timing
  claim is governed by the
  [measurement workflow §1](../../general/agent-measurement-policy.md#1-a-warm-cache-must-never-credit-a-measured-phase):
  a warm cache must never credit a measured phase. Nothing here relaxes that.
  Warm counts are useful for diagnosis; they never qualify a timed result.
- One sample per case and arm stays the rule
  ([AGENTS.md, Measurements](../../../AGENTS.md#measurements);
  [workflow §3](../../general/agent-measurement-policy.md#3-running-a-measurement)).
  The template records host load so a reader can weigh a time. It does not license
  a rerun of an arm whose number came out inconvenient.
- Restating a cap under the right state is not a pass. A cap changes only by an
  owner decision, as with the waivers here.

## Evidence

- Caps and warm counts: [#816](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816),
  [warm candidate counts](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7190447692).
- Cold counts: [handback](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7198507181),
  [results at merge](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7202528418),
  [closing note with the role split and waiver](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7202676223).
- B2 ceilings and waiver: [#810 closing](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810#issuecomment-7203083110);
  what was left: [#824](https://github.com/Ephemeral-AI-Lab/layerfs/issues/824).
- Receipts on the owner's machine, `2026-11-26-commit-batching/`:
  `stage2-816-local-speed.k7ELuhVA` (cold harness) and `bench-810-qualify` (B2).
