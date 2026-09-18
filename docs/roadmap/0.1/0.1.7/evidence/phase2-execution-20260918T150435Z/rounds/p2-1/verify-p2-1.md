# verify-p2-1 — the 32 MiB / spilling-OFF profile: measured and declined

> **author-verified** (single-agent Phase 2). Round: [`receipt.md`](receipt.md).
> Trees: parent `0a593084c`; candidate `/tmp/p21-profile` = that tree plus
> [`attempt/candidate.patch`](attempt/candidate.patch). No product commit exists,
> by design.

## 1. Reproduction

| # | Command | Exit | Output |
| --- | --- | --- | --- |
| R1 | `git archive 0a593084c \| tar -x -C /tmp/p21-profile`, apply `attempt/candidate.patch` | 0 | candidate tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples` | 0 | examples built |
| R3 | the client built in the candidate tree's own evidence `…/client` | 0 | — |
| R4 | `phase0client c2 8191 default` | 0 | `cache_size -32768 cache_spill 0` (profile changed), every work counter identical to `../p2-2/after` |
| R5 | `phase0client c2 1023 default` | 0 | the same |
| R6 | `measure_edits --mode pipeline --case chunked/small-to-large/large-to-small/batch` | 0 | `save:` line, `readback bytes` and `readback group decodes` identical to `../p2-2/after` |

## 2. Falsification answers

**2a — does a new test fail on the parent tree?** No test exists and none is
proposed: the item is declined, so there is nothing to land. The falsification here
is the reverse — the candidate's own read-back *did* confirm the profile changed
(`cache_size 2000 → -32768`, `cache_spill 20000 → 0` on the save's own connection),
so the A/B is a comparison between two genuinely different profiles, not between
two copies of the same one.

**2b — did any counter move in the predicted direction?** No counter moved at all,
in either half. The plan's prediction was a configuration difference, and `AGENTS.md`
is explicit that a configuration difference is not an effect.

**2c — parity?** Untouched: no product byte changed, so the sealed-oracle set and
every pinned expectation are exactly as the parent commit left them.

**2d — single-variable?** The candidate changes two pragmas and their read-back,
nothing else (`attempt/candidate.patch`).

**2e — the item's named risks.** *Per-connection memory*: declared as the pragma
(32 MiB against 2 MiB); not measured as a resident set, and the receipt says so.
*A runtime knob*: none — the constants are compile-time, as plan §4f requires.
*The write-path claim*: not made; V7's spill half is blocked and P0-2 measured zero
spills under both profiles.

**2f — elapsed as a gate?** No; the decline is on the absence of any work-counter
movement, not on a wall time.

## 3. UNVERIFIED

* The candidate's performance (wall time, page reads, resident memory) is
  unmeasured; the decline rests on counters and on P0-2's spill control.
* A workload whose read working set exceeds 2 MiB is not in the frozen set, so the
  shape where the profile could matter is not exercised either way.
