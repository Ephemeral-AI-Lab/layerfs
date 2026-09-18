# Gates and oracles

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Consumed by Stage 6 [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)
> and its two family issues ([#182](https://github.com/Ephemeral-AI-Lab/layerfs/issues/182),
> [#183](https://github.com/Ephemeral-AI-Lab/layerfs/issues/183)).
> Siblings: [`c1-families.md`](c1-families.md),
> [`c2-families.md`](c2-families.md),
> [`memory_cpu_space_support.md`](memory_cpu_space_support.md),
> [`test_setup_and_cache_discipline.md`](test_setup_and_cache_discipline.md).
>
> **No figure in this document is a measurement.** Every number is a declared
> constant, a bound already pinned by an existing test, or a case configuration.

## 1. The claim kind (frozen)

`benchmark_rules.md` §1 requires the question and the exact claim a family is
allowed to support to be frozen *before* implementation. It was frozen in
[`CONTRACT.md`](CONTRACT.md) §1 by owner decision **D1**:

```text
claim_kind = structural-complexity
```

That is: **"is the C1/C2 architecture algorithmically sound"** — scaling ratios,
mechanism counters, declared resource bounds — and **not** "is v0.1.7 faster than
v0.1.6". Every gate below is therefore an **absolute, single-arm** gate.

**Reopening is a scenario change, not a re-label.** If the owner later commissions
a reference-tree entry point and rules `empirical-performance`, then:

- only `component.primitives` (3 cases) can carry a number, because it is the only
  library-matched pair that exists — `pipeline.filesystem` and `pipeline.c2` are
  `NOT_RUN` (the reference's workspace update is private and `WorkspaceAdmission`
  has no public method);
- `benchmark_rules.md` §8 applies: different operation surfaces require **separate
  non-comparative rows**, so no C1/C2 family can be paired with a v0.1.6 family;
- every comparative section of this document becomes `NOT_APPLICABLE`.

Nothing else in this document changes under either ruling. Re-opening needs a new
scenario identity and new receipts: historical rows are never re-labelled
(`benchmark_rules.md`, and `AGENTS.md` §3.2).

## 2. Status vocabulary

Every row ends in exactly one of these, and the distinctions are load-bearing:

| Status | Meaning |
| --- | --- |
| `PASS` | oracle matched **and** every gate in §5 held |
| `FAIL` | a frozen gate was missed, with valid evidence. Retained; admission fails |
| `TARGET_MISS` | the numerical target was missed but the row is otherwise valid |
| `INCOMPLETE` | a required measurement or counter is unavailable — **never** a PASS |
| `INELIGIBLE` | a precondition failed (residency, coverage, cache arm mismatch) — the raw timing is retained but excluded |
| `NOT_RUN` | not attempted, with the reason and the measured wall time where one exists |

A gate that cannot be evaluated because a counter does not exist is `INCOMPLETE`
or `NOT_RUN`. It is **never** converted to zero and never silently dropped.

## 3. Gate taxonomy

| # | Class | What it asserts |
| --- | --- | --- |
| G1 | **Correctness** | the independent oracle matched (§4) |
| G2 | **Mechanism** | the declared route was taken, proven by counters rather than by speed |
| G3 | **Scaling** | the per-doubling ratio lies inside the declared band |
| G4 | **Resource** | declared ceilings held; residency and swap gates held |
| G5 | **Cleanup** | cleanup ran once and passed; the master is unchanged; no sidecars |
| G6 | **Custody** | identities pinned, seals matched, receipt append-only |
| G7 | **Timing purity** | the tree is complete, and no verifier/oracle/digest work sits inside the timer |

## 4. Oracle classes

An oracle must be **independent of the product**: derived from the fixture recipe
and the declared operation, never from the mutated Store.

| Class | Mechanism | Precedent in tree |
| --- | --- | --- |
| **O1 identity** | expected root `ObjectId`, from a frozen constant or recomputed off the product path | `object_identity` (11 tests), `fixture_seal` (2) |
| **O2 logical equality** | read back the logical bytes and compare to the fixture-derived expectation | `edit_reference` (2), `filesystem_reference` (2) |
| **O3 structural count** | chunk counts / page counts / binding counts pinned as constants | `edit_canonical_chunk_count::Expected`, `PLAN_SHA256` |
| **O4 tree equality** | directory root + inode table + filesystem root as a three-tuple | `filesystem_profile` (2), `filesystem_updates` (6) |
| **O5 mechanism counters** | route proof: `untouched_subtrees`, `pages_reused`, `prefix_selected`, `reused` | `10-counters.md` §15.3 |
| **O6 footprint accounting** | allocated vs apparent, `pack_bodies <= database`, freelist, sidecar absence | `tests/memory_bounds.rs` |
| **O7 SQL invariants** | schema identity, 4 tables, 2 indexes, watermark `I1`, `quick_check` | `sqlite/schema.rs` `validate`, `tests/policy_capacity.rs` |

**The single most reusable asset is the sealed-oracle parity set** — **35** external
tests that already exist and stay green through every change:
`fixture_seal` 2, `filesystem_reference` 2, `edit_reference` **3**,
`object_identity` 11, `filesystem_codec` 9, `filesystem_updates` 6,
`filesystem_profile` 2.

*Correction (review S4): an earlier revision said 34 and under-counted
`edit_reference` at 2. Because this set is the **oracle** for the expensive families,
the registry must pin the true count and the file list — otherwise the oracle can
silently shrink when someone deletes a test.* For families where a per-case oracle would be expensive,
**"the parity set stays green plus the pinned identity constants match" is the
oracle**, and it is stronger than a spot check because it covers the codec layer too.

## 5. Per-family oracles and gates

### 5.1 C1

| Family | Oracle | G1 gate | G2 mechanism | G3 scaling |
| --- | --- | --- | --- | --- |
| `c1.construct.whole-file` | O1 + O2 | expected root matches; readback equals input | representation is whole-file | memory flat in n |
| `c1.construct.chunked` | O1 + O2 + O3 | root matches; readback equals input; `chunks_emitted` equals the independently computed count | — | **flat heap** in n (allocator-gated); RSS is a **G4 bound only**; time is **diagnostic** |
| `c1.cdc.chunk-count` | **O3 primary** | `initial_count`, `final_count`, `final_sha256`, `file_root`, `map_sha256` all equal the pinned values (`PLAN_SHA256` is already a 12-entry array) | — | — |
| `c1.edit.length-preserving` | O1 + O2 | root matches; `final_len == base_len` byte-exactly | `payloads_created == 1`, `nodes_created` bounded | local edit cost independent of file size |
| `c1.edit.length-changing` | O1 + O2 | root matches; byte equation `final = base - removed + replacement` | `nodes_read` bounded; deferred bytes bounded | ×2.0 |
| `c1.transition.boundary` | O1 + O2 + representation assert | root matches; below/exact route to whole-file, above to chunked, shrink to whole-file | read-amplification within the bounds already pinned by `tests/edit_transitions.rs` (large→small ≤ final + 2·MAX_CHUNK; →empty = 0; in-place ≤ 4·MAX_CHUNK and < half the base) | — |
| `c1.many-tiny` | O4 + sampled O2 | entry count and per-file lengths match the manifest; `TreeSample` (≤11 files, ≤11 dirs, 3 ranges/file, 64 KiB/range) matches | — | — |
| `c1.tree.construct-traverse` | O4 | three-tuple identity matches; listing equals the fixture manifest | `ObjectWork` nonzero where work occurred, zero where it did not | pages/time ×2.1–2.4 |
| `c1.tree.namespace-mutation` | O4 + O5 | three-tuple matches; alias survives | `released` equals the declared count | — |
| `c1.change-locality` | O4 + **O5 as the claim** | three-tuple matches | **`untouched_subtrees > 0`** — the COW claim. `pages_reused` alone is *not* the claim: it means re-encoded to identical bytes | demanded/pages within the bounds pinned by `tests/filesystem_bounds.rs` |
| `c1.fs.build-scale` | O4 | three-tuple matches at each tier | walk ceiling two-sided: 4,096 accepted, **4,097 refused** | — |

### 5.2 C2

| Family | Oracle | G1 gate | G2 mechanism | G3 scaling |
| --- | --- | --- | --- | --- |
| `c2.lifecycle` | O7 | schema identity, 4 tables, 2 indexes, watermark `I1`, `quick_check == ok` | refusal paths return the declared error class | fixed cost flat in n |
| `c2.reuse.cross-file` | O1 | root matches; readback byte-exact | `reused` equals the declared equation (identical profile → reuse for every member after the first; unique → 0) | reuse cost flat in n |
| `c2.delta.cdc-locality` | O1 + O3 | root matches; chunk counts match | `DeltaCounters` route proof: `prefix_selected` / `full_losses` / `no_candidate` / `work_exceeded` all reported as **policy outcomes** | chain depth ≤ declared |
| `c2.delta.boundaries` | O1 + O3 | root matches at each grammar edge (0, 1, 8,191, 8,192, 16,384, 32,768, 32,769) | `chunks_emitted` matches the edge expectation | — |
| `c2.reuse.workspace` | O1 + O5 | root matches | reuse counts match the profile | — |
| `c2.footprint` | O6 + O1 | `pack_bodies <= database`; exactly one file; **no** `-wal`/`-shm`/`-journal` | — | retained bytes ×2.0 in canonical bytes |
| `c2.read.waves` | O2 + O5 | readback byte-exact; emitted bytes equal requested | `opens` is 1 on the opening wave then 0 (**the O(1) claim**) — **but only through `StoreProvider::read_wave`** (`cas/provider.rs:143` computes `u64::from(opened)`); the direct `Store::read_batch` route reports `opens: 1` unconditionally (`cas/store.rs:294`), so this cell is ungateable on that route and the case must be driven through the provider. `pages == ceil(ids / 128)` | **flat memory** |
| `c2.pool.cold-warm` | O1 + O3 | leaf identity matches; pooled row count matches | `PoolCounters` reuse counts; index entries ≤ 131,072 | cold vs warm reported separately, never pooled |
| `pipeline.*` | O1 + O2 | end-to-end root and readback both match | C1 construction, handoff and save acknowledgement all counted | ×2.0 |

Every G3 cell that is blank is a family with no scaling claim — and therefore no
ratio gate. A family may not acquire a scaling claim after seeing results.

## 6. Cross-cutting gates

### G4 — Resource

| Gate | Value |
| --- | --- |
| `heap_peak_bytes` | ≤ the family's declared ceiling |
| `rss_incremental_peak_bytes` | ≤ the declared ceiling |
| `swaps` | **== 0** (hard failure otherwise) |
| `resident_pages` | **== 0** wherever the row claims de-warmed or cold |
| `disk_read_bytes` | **≥ 0.9 × requested** wherever a row claims a de-warmed or cold read — **device attestation** |
| `allocation_attribution` | `exclusive` for any row gating `store_allocated_bytes`; `shared-with-master` rows are refused |
| allocation count | **== 0** on the paths already pinned allocation-free (`tests/filesystem_ordering_scan.rs` asserts zero allocations across a full ascending sweep) |
| codec workspace | 2 MiB encode / 1 MiB decode — declared, not summed with other claims |

### G5 — Cleanup

`cleanup.status == PASS`; the fresh output path was consumed exactly once; the
prepared master is byte-identical afterwards (`master_unchanged`); no sidecars;
the sample Store removed. **Arms never share a Store** — `cleanup.rs:65,120` issue
paged `DELETE`s, so sharing would let one arm mutate another's artifact.

### G6 — Custody

Source commit and dirty flag, product seal, harness seal, fixture digest, oracle
identity, seed, worker count, cache state, and the append-only receipt path. A
rebuilt artifact invalidates its matched arm.

### G7 — Timing purity

- `report.is_incomplete()` must be **false**. Clipping is silent by design, so the harness converts it to a hard failure (§10.5 of the resource doc).
- zero verifier, digest, oracle and enumeration work inside the timed region, proven by counters rather than asserted.
- `elapsed_ns` is recorded beside every counter and **never gate-decides alone**: Phase 0 established bit-identical work counters against a **+17.6 %** same-binary wall spread.
- the external command wall is labelled external and never substituted for an inner metric.

### G3 — Scaling band

**Time is not a gate (correction, review S3).** With the established **±17.6 %**
same-binary wall spread, the O(n) *time* band is `[1.60, 2.40]` — which overlaps
O(n log n) `[1.68, 2.88]` and cannot separate O(n) from O(1) either. A zero-width
band on a ±17.6 % measurement is a coin flip, not a gate. **Therefore G3 gates on
counters, heap and disk; `elapsed_ns` is reported `DIAGNOSTIC` and can never produce
`FAIL`.** Any per-family time band elsewhere in this document is superseded by this
paragraph.

**The exponent.** The byte ladder `{1, 10, 100, 500}` MiB contains **no doublings**,
so "per-doubling ratio" is undefined as previously written. Freeze these:

```text
doublings_i    = log2(t_{i+1} / t_i)          # 1→10 = 3.3219 ; 10→100 = 3.3219 ; 100→500 = 2.3219
per_doubling_i = ratio_i ** (1 / doublings_i)
b              = least-squares slope of log2(v) on log2(t)   # the actual complexity claim
```

Report `per_doubling_i` because the spec names it, and report `b` because a claim of
"O(n)" is a statement about the slope; a four-point ladder supports a slope with a
residual, a single ratio does not.

| Claimed growth | Counter band | Heap / disk band | Time (diagnostic only) |
| --- | --- | --- | --- |
| O(1) | `[0.90, 1.11]` | `[0.90, 1.11]` | `[0.60, 1.67]` |
| O(n) | `[1.90, 2.10]` | `[1.80, 2.22]` | `[1.60, 2.40]` |
| O(n log n) | `[1.95, 2.55]` | `[1.90, 2.60]` | `[1.60, 2.90]` |
| O(n²) | `≥ 3.6` | `≥ 3.4` | `≥ 2.6` (one-sided) |

A missing tier makes the band `INCOMPLETE`, not PASS — with three distinct cases:
not selected by the lane → `NOT_RUN` (reason `lane=smoke`); selected but absent from
the results dir → `INCOMPLETE`; cut for budget → `NOT_RUN` (reason `budget-cut`,
with the measured wall time). A ratio outside the band is a **refutation**, reported
as one.

## 7. Claim mapping

Every published sentence maps to:

```text
claim_id, claim_kind, exact wording and scope,
measured_fixture_sizes_bytes, family and scenario IDs, source arm,
metric + unit + timing_boundary_id, throughput byte basis where applicable,
aggregate formula, gate and status, evidence directory and manifest,
candidate source and product seal
```

A report generator must **fail** if a claim lacks eligible evidence. Under
`claim_kind = structural-complexity`, `measured_fixture_sizes_bytes` may be
non-empty only for rows whose tiers were actually collected — complexity analysis
is not measured evidence, and a synthetic logical size must not be presented as one.

## 8. What is still missing — stated plainly

This document does **not** close three of #171's acceptance bullets, because they
have no instrument yet:

| Bullet | Why it is unsolved |
| --- | --- |
| **Failure / unknown-outcome / cleanup verified** | `UnknownOutcome`, `UninspectedState` and `CleanupFailed` must be exercised **without fault injection**, which `core/AGENTS.md` and #171 forbid. External perturbation is required — killing the process at a declared point, or hand-editing a Store so the watermark is ahead and `Store::open` refuses with `Integrity`. Neither is designed. |
| **Concurrency / visibility** | Two `begin_save` on one Store: the second must fail `OwnershipUnavailable` (busy timeout is zero). Same process or two, one directory or two — unspecified. |
| **Zero retry / fallback / fsync / WAL proven** | There is no counter for an absent route. The rules require a sealed call-graph/manifest status plus observable runtime tripwires, and explicitly forbid fabricating a zero. Not designed. |

Also still open, and upstream of admissibility:

- **the counter-attribution caveats — both re-verified, neither is an open blocker.**
  `SortedWork.pages_read` no longer undercounts batched merges
  (`filesystem/sorted/page.rs:277`; landed in Phase 1 with its receipt under
  `../evidence/phase1-execution-20260918T090000Z/rounds/c1-rebaseline/`). The claim
  that an inner engine inside `Engine::apply_root` returns its work to nobody does
  **not reproduce**: the crate's only `Engine::<F>::new` is
  `filesystem/sorted/finish.rs:33`, its work is returned at `finish.rs:109`, and
  both callers aggregate it (`filesystem/update.rs:249-252`, `:361`). A counter
  error would still invalidate every row citing it, so a *new* Stage 6 counter must be
  attributable where it is charged — but nothing here blocks a row today.
- `c2.delta.small-file`'s case list is still `TBD`.
- the declared cardinality array depends on the tier-cut and boundary-family
  decisions still open in [`c1-families.md`](c1-families.md) §9 and
  [`c2-families.md`](c2-families.md) §9.

## 9. Non-goals

- No comparative gate is created here. Under `structural-complexity` none is needed; under `empirical-performance` none is *possible* beyond three cases.
- No gate is loosened, and no timeout inflated, to turn a miss into a pass.
- No counter is estimated, fabricated or converted from an unavailable value.
- No row is marked PASS by argument; a verification mode must reproduce it.
