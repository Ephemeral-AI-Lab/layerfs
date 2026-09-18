# verify-p2-5 — one presence query per wave, one pooled reader per save

> **author-verified** (single-agent Phase 2: no second reviewer, no subagent).
> Round: [`receipt.md`](receipt.md). Tree: `7db87bb8b`, arm [`after/`](after/);
> before arm [`../v8/after/`](../v8/after/) and [`before-references/`](before-references/)
> on `12527f477`.

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 7db87bb8b \| tar -x -C /tmp/verify-p2-5`, plus this round's client | 0 | clean P2-5 tree |
| R2 | the probe client built in the archive's own `…/client` | 0 | — |
| R3 | `phase0client c2-references 64` | 0 | `dependents presence_queries 1` (parent arm: 64) |
| R4 | `phase0client c2-references 512` | 0 | `1` (parent: 512) |
| R5 | `phase0client c2-references 4096` | 0 | `8`, one per 512-object wave (parent: 4,096) |
| R6 | `cargo test -p layerfs-storage --test metadata_pool a_pooled_reader_releases` | 0 | `1 passed` |
| R7 | `python3 compare_arms.py rounds/v8/after rounds/p2-5/after --skip Y2,Y3,Y4` | 1 (by design) | `steps compared: 37, differing: 1` — D22's `presence queries` 0 → 1 |
| R8 | `python3 pack_bytes_census.py rounds/v8/after rounds/p2-5/after` | 0 | 12 stores, identical |

## 2. Falsification answers

**2a — does the new test fail on the parent tree?** Two answers, both recorded.
`a_pooled_reader_releases_pack_bodies_when_the_store_writes` fails to compile on
the parent (`PoolReader::release_packs` did not exist) — the honest form for a new
accessor. The *movement* this item claims is measured, not tested: the nominated
rows read 64/512/1,024/4,096 before and 1/1/2/8 after, with the before arm
collected on the parent tree in this round's own evidence directory.

**2b — did the counter move in the predicted direction and magnitude?** Yes on the
nominated rows: exactly one presence query per preparation wave. On the frozen set
the movement is **one row, and it costs**: D22 reads 0 → 1 because the wave-level
seed asks for references the lazy path never needed to ask about there. Everything
else is bit-identical and the 12 stores are byte-identical.

**2c — parity green and unchanged?** 35/35 sealed-oracle tests. `git diff
12527f477..7db87bb8b -- '*tests*'` adds the metadata_pool case and touches nothing
else.

**2d — single-variable?** `git show --stat 7db87bb8b`: `cas/dependencies.rs` (the
seed), `cas/save.rs` (the wave call), `cas/owner.rs` (the charge helper),
`cas/pool_lane.rs` (the shared reader), `cas/placement.rs` (the release on write),
`encoding/pool/read.rs` (the release), the new case and two architecture papers.
One variable: when availability asks, and which reader a trial uses.

**2e — the item's named risks, one by one.** *Availability semantics*: the seed only
records engine answers; a reference the query does not find stays unknown and
`validate` refuses as before (`cas_reuse.rs`'s control case and the missing-object
cases stay green). *Reader state across trials*: the pack-body staleness the plan
names is real and pinned by the new case; the reset happens on every pack write, so
a trial can never read a pack as it was before this save's write.

**2f — is elapsed a gate anywhere here?** No; the gate is `presence_queries` and
the unchanged counters.

## 3. UNVERIFIED

* **The reader-sharing half has no counter.** Reusing the pooled reader removes pack
  re-reads and decode work that nothing measures today; this receipt claims the
  correctness of the sharing (the release) and does not claim a magnitude.
* **The staleness that motivated the release is not observed end-to-end.** The new
  case constructs the hazard at the reader level (retained body, replaced pack,
  release); no frozen row interleaves a base trial and a pack write, so the
  save-level sequence is argued from the write path, not measured.
* **The seed's extra query on D22 is a cost, not a saving.** It is one per wave and
  bounded, but on shapes where the lazy path asked nothing the batch asks once; the
  receipt records it rather than presenting the item as free.
