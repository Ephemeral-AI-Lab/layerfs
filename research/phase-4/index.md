# Phase 4 research program

Status: research only; no implementation or profile-selection authority.

Research began on 2026-08-20, branch `codex/empty-worktree`, checkpoint
`83d085bd80e82ae22b4a9766f2fc8aed03501fb8`. A separate concurrent task later
advanced the branch to checkpoint
`d781173a08ab4092eb539c3a0870056e6c6a77ff`; this squad did not edit or revert
that task's source changes. The accepted benchmark source and sealed F4 raw
evidence used here were reverified at SHA-256
`c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158`
and `5241b106a9d1d841e124d73ff247f2abadb2bf27759ef54d62a3ab3af3eb212f`.

## Purpose

This folder is an independent search for the best local algorithms and data
structures for LayerFS's Phase 4 core:

```text
source -> CDC -> canonical objects -> authenticated CAS
       -> bounded radix/COW mapping -> root + delta
       -> atomic durable publication -> verified reads
```

It is deliberately broader than the active F-series. A research candidate may
question the physical layout, indexing method, construction pipeline, or
mapping data structure. It may not present a proposal as a result, silently
weaken an invariant, or rewrite a failed experiment as a success.

## Layout

```text
foundations/  evidence method, invariants, and hypothesis portfolio
core/         one folder each for canonical, CAS, CDC, COW, and pipeline
storage/      SQLite durability/layout and rejected/deferred physical codecs
assurance/    verification, security, exact errors, and resource bounds
handoffs/     later-phase constraints that Phase 4 must preserve
```

The top-level `decision-map.md` is the concise execution handoff. Research
stays here; implementation contracts and terminal milestone reports belong in
`implementation-detail/phase-4`, while raw campaign artifacts belong in
`target/`.

## Evidence discipline

Every substantive statement in these notes is one of:

- **Observed** — directly present in current source, tests, raw evidence, or a
  primary external implementation/paper.
- **Derived** — arithmetic or complexity derived from observed inputs, with
  the equation or reasoning shown.
- **Hypothesis** — a falsifiable proposal that still needs prospective local
  evidence.
- **Unavailable** — not observable from the retained evidence, with the missing
  observation named.

Priority of authority is:

```text
accepted LayerFS specification and sealed local evidence
  > current source and focused tests
  > primary external papers/specifications/implementations
  > derived model
  > proposal
```

External success on another workload is prior information, never a LayerFS
performance result.

## Research tracks

| Track | Report | Central question |
|---|---|---|
| Identity | [Identity and hashing](core/canonical/identity-and-hashing.md) | Can required identity domains share construction work without merging trust domains or adding byte passes? |
| Canonical redesign | [Canonical v2 single identity](core/canonical/v2-single-identity.md) | Can one canonical chunk identity and one ordered commitment replace two byte-complete commitments safely? |
| H05 handoff | [Terminal findings](core/canonical/h05-terminal-findings.md) | What survived H05/H05b/H05c, what was rejected, and why CP-0009 remains control? |
| Canonical-v2 execution | [Agent prompt](core/canonical/canonical-v2-agent-prompt.md) | How should the next agent explore v2 quickly without weakening hard semantics or running promotion-scale tests? |
| CDC | [CDC locality and algorithms](core/cdc/locality-and-algorithms.md) | Is there a better exact/local chunking pipeline, and which benefits are compatible with frozen boundaries? |
| CAS | [CAS and authenticated reuse](core/cas/authenticated-reuse.md) | Can immutable reuse and lookup avoid redundant reads while preserving byte-level authority? |
| Core pipeline | [CAS + CDC + COW full-create pipeline](core/pipeline/full-create-pipeline.md) | Which work is already fused, and where is enough end-to-end core budget left? |
| SQLite storage | [Durability and layout](storage/sqlite/durability-and-layout.md) | Which local physical layout minimizes pager, index, and sync cost under atomic publication? |
| Mapping | [COW mapping and deltas](core/cow/mapping-and-deltas.md) | Can early count-changing edits escape suffix-linear rewrite without losing canonical determinism? |
| Assurance | [Verification, security, and resources](assurance/verification-security-resources.md) | How do we strengthen proofs, fuzzing, crash behavior, and exact bounds without slowing the product path? |
| Later handoff | [Hot/cold materialization](handoffs/hot-cold-materialization.md) | How can authenticated deltas and verified native seeds avoid payload work safely after Phase 4? |
| Compression | [Compression and packing](storage/compression-and-packing.md) | Does the retained post-CDC corpus provide enough byte saving to repay foreground codec/pack cost? |
| Constraints | [Invariant matrix](foundations/invariant-matrix.md) | What is immutable, what is experiment-variable, and what needs a format migration? |
| Measurement | [Benchmark and evidence method](foundations/benchmark-and-evidence.md) | What campaign is sufficient for causal, acceptance, scale, and portability claims? |
| Portfolio | [Hypothesis ledger](foundations/hypothesis-ledger.md) | Which ideas are proven, refuted, open, blocked, or deliberately deferred? |
| Synthesis | [Decision map](decision-map.md) | Which smallest experiment should run next, and what can stop immediately? |
| Historical prompt pack | [Pre-CP-0009 concurrent research](while-waiting-phase-4-to-finish/index.md) | Preserved research inputs; old prompts must be amended to current CP-0009/WP4-P authority before reuse |

## Current empirical anchor

CP-0009 is the exact current-product control. WP4-P is complete and remains
closed at K64/F64 + DIR256K. A future candidate must run adjacent balanced A/B
against the exact CP-0009 binary; its standalone median is context, never a
historical subtraction operand:

| Measure | Accepted value |
|---|---:|
| Durable create | `640.109209 ms` (`156.223 MiB/s`) |
| Mapping/proof construction | `504.215417 ms` |
| Proof consumption | `0.038542 ms` |
| Standalone COMMIT | `135.855250 ms` |
| Same-open same-count edit | `9.737250 ms` after `245.330416 ms` first authority |
| Warm/fresh logical materialization | `425.800708 / 433.512791 ms` |
| Authenticated returned 1-MiB range | `3.171209 ms` / `315.337 MiB/s` |
| Reopen/head ready | `3.007750 ms` |
| CDC references | `5,284` |
| Created objects | `5,372` |
| New canonical bytes | `105,291,554` |

The accepted F2-v3 campaign remains historical optimization evidence, while
the observer-heavy F4-A diagnostic measured `636.836792 ms` durable and
partitioned its mapping median into `128.723024 ms` CDC-exclusive,
`280.146626 ms` across three required hash intervals, `48.853618 ms`
VDBE+pager, and `24.281657 ms` direct VFS. It was not an acceptance A/B and
does not replace CP-0009.

F4-A2 then measured the net removable complete-chunk materialization budget as
only `3.701583 ms` median (`0/5` rows at the `33 ms` gate). The accepted
scanner therefore remains the rational default; a different CDC algorithm is
a format/identity research question, not an F4-A2 relabel.

The research waves and terminal H05 work add five directional conclusions:

1. H05/H05b/H05c are closed: H05 exposed a strong `16.655343%` median signal
   but is a frozen storage-gate no-go, so CP-0009 remains control;
2. a compact `(length, canonical ObjectId)` v2 is now the next core research
   direction, beginning with a nonpersistent authority/migration shadow and
   short graded exploratory screens;
3. native materialization must prioritize authenticated parent-to-child deltas
   and verified APFS file seeds, not call logical reconstruction “cold”;
4. exact-fixture compression and Git delta packing are negative directions;
   and
5. SQLite 4/8/16-KiB page profiling is residual work after canonical/core
   authority is settled.

## Decision sequence

For every candidate:

1. State the invariant and exact mechanism to change.
2. Bound its removable wall with current evidence.
3. Stop if the optimistic ceiling is too small.
4. Write a prospective one-variable experiment and counter prediction.
5. Prove identity, authority, durability, error, resource, and cleanup parity.
6. Run a hard-`<=120`-second exploratory screen; grade the speed signal while
   keeping semantic, authority, durability and resource failures terminal.
7. Run a balanced release A/B on the complete workload only after the screen
   passes.
8. Run 512-MiB scaling and protected edit/read workloads only after the
   retained 100-MiB gate passes.
9. Promote nothing from a screen, microbenchmark, diagnostic, or another system's
   paper.

## Scope boundary

The immediate objective is local performance. Remote object stores,
replication, gossip, consensus, distributed leases, and network fan-out are
out of scope unless an isolated local mechanism survives without them. Local
NVMe locality, immutable preparation, authenticated indexes, atomic manifest
publication, and bounded compaction remain valid subjects.
