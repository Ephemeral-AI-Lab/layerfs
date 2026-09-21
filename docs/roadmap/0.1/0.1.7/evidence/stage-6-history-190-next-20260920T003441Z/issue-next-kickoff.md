> Status: Research; informative and not a product contract.

## Publication requested; next optimization squads launched

The owner requested committing/merging the completed parent-lookup optimization and continuing with subagents. The completed change retains its measured improvements (stride10 operation47.161768126→27.386866665s; stride3 125.277281254→79.607320336s), identical70state roots/paired Store bytes and production LOC85,533→85,582(+49). It does **not** meet the historical time tripwire or stride3 verification target; O3 pins/cache admission and existing harness lint/format failures remain open.

The publication branch is isolated from unrelated local architecture work. Exact first-parent/staged-tree LOC and source identities are being checked before PR creation/merge. Large immutable Stores/executables remain local per the repository archive policy; committed receipt manifests retain original paths, bytes and hashes.

Three subagents have begun the next-target investigation:

1. **Provider reads:** confirmed fresh `PoolReader` per inode leaf and two `record()` extractions per pooled chain element (dependency discovery and reverse reconstruction). Compressed physical groups can be decompressed twice. Existing ordinary `group_decodes`/pack/edge counters omit this pooled branch, so they cannot price it. First target: measure pooled work explicitly and test bounded decoded-group reuse without changing chain-work refusal semantics. No speedup claimed.
2. **Filesystem residual:** three coarse scopes can isolate metadata overlay, zero-count lookup and release within the existing1024-node bound. Lazy reference processing remains inside `inodes`. Filtering ignored base lookups for newly allocated rows is a candidate only after error/corruption parity proof.
3. **Store admission:** fallback-index selection can compute the same content signature again during FULL admission. A lazy per-selection signature reuse is the smallest secondary candidate; full-pack write coalescing is riskier because it affects visibility, memory and transaction cadence.

Next research is separate from the change being merged. Reports are under `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-next-20260920T003441Z/`; no next-phase product edit or measurement has occurred yet.
