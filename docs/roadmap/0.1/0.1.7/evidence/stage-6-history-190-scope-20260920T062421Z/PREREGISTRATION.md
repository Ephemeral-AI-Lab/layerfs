# Pre-registration: one scope change to the pooled metadata reader

> Status: Research; diagnostic evidence, not release admission. Written **before**
> the first sample of this round, at 2026-09-20T06:41Z. The mechanism reading it
> tests is [`mechanism.txt`](mechanism.txt), produced from the retained campaign's
> own counters before any product line changed.

## The treatment

**The pooled metadata reader's lifetime becomes its calling operation's.**

Source fact today: `encoding/delta/read.rs::Resolver::resolve_charged` builds
`PoolReader::new()` once per resolved inode leaf, so the reader's pack cache
(`DEPENDENCY_PACK_CACHE_BYTES`, 4 MiB) and its decoded value cache
(`POOLED_VALUE_CACHE_BYTES`, 512 KiB) are discarded with every leaf. The measured
consequence: 176x (stride10) and 444x (stride3) the whole pack space copied by one
run's filesystem read provider, with 99.7% / 99.9% of fetches re-reading a pack the
same state had already read.

The change is scope and nothing else:

* **No bound moves.** 4 MiB pack cache, 512 KiB value cache, both still released
  wholesale when the next body would cross them. No retained-bytes increase: one
  reader is live at a time either way.
* **No instrumentation is added.** The candidate arm is the retained harness
  (unmodified) linked against the treated product crates, so both arms publish the
  same counters. The binary differs; the instrumentation does not.
* **No other cache changes.** The ordinary-lane pack cache stays wave-scoped, the
  decoded ordinary-group cache stays operation-scoped, and the single construction
  worker, the corpus, the selections, the ceilings, the timeouts and the phase
  declaration are untouched.
* **The invalidation contract is the existing one.** A pack at or below the ceiling
  a read was authorized under is immutable (a save creates its packs above the
  baseline publication watermark and publishes only by advancing it), and every
  pooled consult checks the location's pack against the current ceiling before the
  cache answers. A **writing** owner keeps releasing its own reader's pack cache on
  every pack write (`cas::placement::MutationOwner::write_pack`), which it already
  did for the pooled lane and the depth walk; the treatment points the save's
  `resolve_location`, `read_batch` and selection at that same reader.

## The experiment

| | |
|---|---|
| baseline arm | the retained tree at `c4f757514`, binary `190424195506d4d5…` |
| candidate arm | `c4f757514` + this one scope change |
| cases | `history-stride10`, then `history-stride3` |
| samples | **one per case per arm**, fresh outputs, append-only receipts |
| order | baseline10, candidate10, baseline3, candidate3 |
| caps | the established #190 diagnostic caps, 120 s / 240 s — not promoted |
| budget classes | NOT_RUN: 37.5 s / 75.0 s complete command fits no frozen class |
| admission | INELIGIBLE (OS cache state uncontrolled); O3 INCOMPLETE |

**Equivalence gate.** Every `history.state.N.root` identity equal between the arms,
the canonical object count and the value-group inventory equal, and the saved Store
**byte-identical**: stride10 `4af37932aa3391b12269de8130b9c66dc504f64ca78fc3e585f7afddabed8487`,
stride3 `f5c7ff5a6b4f0821aa9a21ac5250335c4c3fb889637a0c5345c5278caadc2a9e`. The
retained campaign's own Stores hash to those values, so the gate is checked against
a recorded constant, not against a new one.

**Retain/reject rule, fixed now.** Retain only if stride10 improves by **≥ 1 s**
(complete-command wall and operation both reported) with correctness preserved and
stored bytes unchanged; a stride3 **regression** rejects the treatment; a
sub-second stride10 movement is reported as a rejected treatment with its evidence
retained.

**Prediction, and what refutes it.** If the term is scope, `pack_fetches`,
`pack_bytes` and `value_group_decodes` per state fall sharply in the late states
while `physical_record_calls` and `chain_edges` are unchanged, and stride10's
operation drops by the pre-registered 2.1-3.9 s order. If the repetition is wider
than any bounded window, the counters move little and the reading in
[`mechanism.txt`](mechanism.txt) is **refuted**, not confirmed - the counters say
how much repetition exists, not how close together it is.

**What this cannot establish.** One sample per case per arm; within-run and
between-arm comparisons only; no cold claim (corpus residency is measured and
reported, never enforced); no stride1 sample, so the *scaling* claim is not
asserted here - stride10 and stride3 carry the depth-term claim.

## Identities

Isolated worktree `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope`, branch
`codex/190-pooled-scope`, base `c4f757514` (the scaling handoff on top of
`80e6b9868`). Rust 1.85.1, `--locked`, corpus manifest SHA256
`03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`, tip
`b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`. All eight behavioural history switches
unset, `LAYERFS_HISTORY_PHASES=1`, `LAYERFS_CONSTRUCTION_WORKERS=1`.
