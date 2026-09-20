# #190 — bounded parent lookup batching and record reuse

> Status: Research; informative and not a product contract.

**Implemented and validated in the working tree.** One product file now batches
directory-parent acquisitions and later metadata acquisitions with existing
`lookup_many`, retaining the final bounded authenticated parent window for reuse.
It preserves the original reducer insertion order, canonical format, validation,
worker policy and resource ceilings. The [architecture addendum](../../../../../../core/docs/architecture/04-filesystem.md)
describes the implementation and its bounded, partial reuse.

The fresh diagnostic pair reproduces a reduction in actual read work and elapsed
operation time on both selections. **Both final Store files are byte-identical to
their respective baselines; every state root and retained save counter matches.**
These are one-sample instrumented diagnostics with uncontrolled cache residency,
not cold or release-admission evidence. The historical v0.1.6 tripwire is not
declared resolved. [Independent review](review/REVIEW.md) reproduces the results.

## Changes and LOC

| Scope | Before | After | Signed production LOC delta |
| --- | ---: | ---: | ---: |
| Legacy reference | 65,417 | 65,417 | 0 |
| Replacement core | 20,116 | 20,165 | +49 |
| **Combined product** | **85,533** | **85,582** | **+49** |

Changed product file: [filesystem/update.rs](../../../../../../core/crates/layerfs-content/src/filesystem/update.rs),
405 → 454 production LOC. No new product file, dependency, public API or format.
Parent order/uniqueness is already validated, so no redundant sort/dedup layer is
introduced. Two bounded record windows may coexist; older omitted records are
reread in batches instead of keeping an unbounded all-parent cache.

Harness-only read observation, external tests and documentation do not contribute
to production LOC. The same `tools/production_loc.py` scanner/version counts both
snapshots, including runtime SQL and excluding blank/comment lines, tests, inline
legacy tests, examples, tools and benchmark code. [Before](production-loc-before.json),
[final](production-loc-final.json) and independent recount agree. This is a
working-tree comparison, not a commit comparison; no commit or push was made.
Unrelated architecture proposal edits present at the start were preserved.

## Step record

1. [Measure](STEP-1.md): fresh named phases and actual provider acquisitions;
   no guessed disk traffic or codec CPU.
2. [Batch](STEP-2-3.md): bounded `lookup_many` calls in both directory-parent and
   final-metadata passes.
3. [Reuse](STEP-2-3.md): final-window reuse under the same immutable base;
   supplied metadata retains precedence. These are one cohesive change, so the
   measured effect is joint rather than separately attributed to each step.
4. [Prove and confirm](STEP-4.md): core/harness checks, deterministic read-count
   regression, quota parity, stride10/stride3 diagnostics and separate sampled verification.
5. [Consider deeper work](STEP-5.md): subtree summaries and streaming deferred;
   the remaining intervals do not yet isolate the mechanism those changes would remove.

## Exact diagnostic comparison

One sample per arm/case; no medians or best-of selection. Values below are integer
nanoseconds or counts. A negative delta is candidate minus baseline.

| Metric | Stride10 baseline | Stride10 candidate | Stride3 baseline | Stride3 candidate |
| --- | ---: | ---: | ---: | ---: |
| States | 17 | 17 | 53 | 53 |
| **Operation ns** | **47,161,768,126** | **27,386,866,665** | **125,277,281,254** | **79,607,320,336** |
| Operation delta ns | — | **−19,774,901,461** | — | **−45,669,960,918** |
| Filesystem interval ns | 34,686,235,752 | 14,832,606,168 | 99,950,581,082 | 53,702,386,711 |
| Validation ns | 3,429,981,622 | 3,505,397,540 | 17,333,541,001 | 17,489,653,664 |
| Directories ns | 11,506,307,751 | 1,633,180,084 | 28,913,504,290 | 4,958,796,085 |
| Inodes ns | 4,470,610,374 | 4,482,754,167 | 15,008,284,830 | 15,739,979,668 |
| Filesystem residual ns | 15,278,643,093 | 5,210,486,416 | 38,692,689,426 | 15,511,671,619 |
| Provider read elapsed ns, nested | 33,559,231,582 | 13,792,721,269 | 98,406,450,952 | 52,197,491,759 |
| Provider waves | 110,715 | 66,616 | 210,380 | 111,853 |
| Requested/returned objects | 117,533 | 74,279 | 230,323 | 135,296 |
| Returned canonical bytes | 286,247,942 | 148,826,516 | 706,999,818 | 377,985,893 |
| Group decodes | 513 | 513 | 3,966 | 3,892 |
| Provider connection opens | 16 | 16 | 52 | 52 |
| Save commits | 1,149 | 1,149 | 1,470 | 1,470 |
| Complete command ns | 66,657,423,500 | 40,871,159,292 | 146,622,418,084 | 99,357,294,167 |

Observed operation reductions are 41.92994081173605% and 36.45510220277214%, computed
as `100 * (baseline_ns - candidate_ns) / baseline_ns`. These percentages describe
the retained diagnostic pair; they are not a universal speed guarantee.

Provider time overlaps the filesystem interval. It includes Store/SQLite demand,
reconstruction and authentication, not separately observed physical disk or codec
CPU. Filesystem residual contains remaining metadata overlay, zero-count/release
and bookkeeping; it is **not** assigned wholesale to a tree scan. Inodes includes
lazy reference-stream work. The smaller references/cleanup/root.encode spans and
every state residual are preserved in [results.json](results.json) and the
[stride10](history-stride10-states.csv) / [stride3](history-stride3-states.csv)
per-state tables. Exclusive leaf spans plus explicitly named parent residuals sum
exactly to each state and operation; arithmetic residual is zero without pretending
all mechanism attribution is complete.

The unchanged non-provider filesystem work counters, roots and Store bytes guard
against claiming a saving by dropping canonical or validation work. The source-only
difference between compiled product arms is `filesystem/update.rs`; both use the
same harness source and dependency locks. This supports the combined batching/reuse
diagnosis. It does not allocate the original historical 21.195388457-second excess:
that older core binary/cache identity was not sealed, and timer surfaces differ.
The current candidate's 27.386866665-second stride10 operation still exceeds the
cited legacy 11.370679212-second Commit sum; the historical tripwire remains open.

## Correctness and resource acceptance

All 17 + 53 state root IDs match their baseline. Final database SHA-256 matches
within each pair, including all persisted bytes, not just logical root values:

| Selection | Baseline and candidate database SHA-256 |
| --- | --- |
| Stride10 | `ab64dab7aaed512ab93b7ccc46fd14f22e57791f642b39fdf2b94650a41a965f` |
| Stride3 | `cfb74bc9b1db6c9470129613283a8f4afa3b139c2819ae7bd7a4f14e548ecdb5` |

The focused five-parent/shared-leaf fixture demands 23/17/15 objects on baseline
at batch widths 1/3/64, versus 22/10/6 after. Explicit identical metadata demands
18/12/10 before, 18/9/6 after. Full-window equality at width64 proves reuse; smaller
windows deliberately permit batched rereads. All 56 quota cells have identical
success roots and typed refusals, including their limit/actual fields.

The first candidate inserted values earlier and made **eight** previously accepted
quota cells fail. It was rejected before performance collection. Its
[patch](candidate-v1-rejected.patch), [differential](candidate-v1-quota-differential.json)
and [+16 intermediate LOC receipt](production-loc-candidate.json) remain retained.
The generic matrix originally allowed resource refusal and thus passed alone;
comparison with baseline exposed the regression. Explicit assertions now pin those
eight baseline-supported cells. Final ordering restores exact parity without
raising any budget. See [final differential](candidate-v2-quota-differential.json).

An existing materialization test's total changed 107→104 waves and 303→300 objects
because one second parent lookup through a three-page inode path is eliminated.
Its expected wide waves `[50,64,64]`, inode-engine waves5, inode pages134 and golden
root remain fixed. Fixture depth is explicitly asserted; the initial failing log
and corrected focused proof are both retained.

Independent sampled read-back used the original verifier route, separately from
performance, against each exact binary and its generated Store:

| Selection/arm | Compared paths | Disagreements | Verification work ns | Target | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| Stride10 baseline | 1,083 | 0 | 8,124,985,000 | 10,000,000,000 | PASS target |
| Stride10 candidate | 1,083 | 0 | 8,000,840,000 | 10,000,000,000 | PASS target |
| Stride3 baseline | 3,377 | 0 | 29,241,424,750 | 20,000,000,000 | **TARGET_MISS** |
| Stride3 candidate | 3,377 | 0 | 29,978,286,084 | 20,000,000,000 | **TARGET_MISS** |

Zero disagreements means zero mismatches, missing and unexpected entries in the
declared sample; it is not exhaustive corpus read-back. All invocations fit the
60-second verification hard limit. Neither stride3 target is relaxed.

| Selection/arm | Apparent bytes | Recorded allocated bytes | Allocated comparison ceiling | Result |
| --- | ---: | ---: | ---: | --- |
| Stride10 baseline | 49,053,696 | 49,688,576 | 49,344,512 | **FAIL** |
| Stride10 candidate | 49,053,696 | 49,192,960 | 49,344,512 | PASS |
| Stride3 baseline | 61,767,680 | 62,402,560 | 64,024,576 | PASS |
| Stride3 candidate | 61,767,680 | 62,103,552 | 64,024,576 | PASS |

Allocation differences are not claimed as algorithmic savings: the paired file
contents are identical. Use each original `st_blocks` reading without replacing
it by a later stat. All four performance traces retain O3 **INCOMPLETE** because
history golden counters are absent. Cache/admission status remains **INELIGIBLE**.

## Checks, identities and reproduction

Final explicit commands passed: `cargo +1.85.1 test --manifest-path core/Cargo.toml
--locked --no-fail-fast` (**490 tests**), `test --examples` (example targets compile;
zero example test cases), `clippy --all-targets -- -D warnings`, `fmt --all --check`,
product boundary guard and its self-tests. The independent harness workspace's
locked tests passed **117 tests**, and both release builds passed. Exact commands,
exit codes and walls are in [checks](checks/).

Final **harness** Clippy still fails at 22 statements present in HEAD, and harness
fmt still fails. These are distinct from the passing core checks and are not
suppressed. Earlier observer compilation failed for missing public documentation;
that was fixed before the baseline build. Initial core formatting differences in
the new test were fixed before final checks. No CI/preflight or aggregate gate ran.

Source HEAD: `9f35c49ad62956f131dc2676787f99d69659686e`, with declared dirty harness
and final product worktree changes. [Baseline identity](baseline-identity.json)
and [candidate identity](candidate-identity.json) retain per-source hashes and
lock/toolchain identities; [final patch](candidate-v2.patch) retains the product change.

- Baseline binary: `c5826d20f217a73ed4d814b82c3f8d8cdcaaa7af94ead18c0291918c88d4a855`.
- Candidate binary: `f3afbe222248aa1040004dd09095b63cd94dc6a8f919f48c18a99f3ce42b916e`.
- Corpus manifest: `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`;
  tip `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`.

Each run folder contains its command, limits, environment, pre/post machine
observations, output, raw phases/timing/trace, immutable performance-trace copy and
hash manifest. Reused existing corpus preparation and incremental locked builds;
no reused mutated Store. Two preflight deferrals (baseline17 and candidate53)
preceded successful launch; no performance sample was discarded or repeated.
All eight behavior switches were unset; workers=1 and diagnostic phases=1.
Desktop activity remained, despite no named compiler/benchmark competitors and
at least70% CPU idle at launch. This is not a completely quiet/cold host claim.

Run `python3 <campaign>/analyze_results.py` to regenerate derived JSON on stdout;
the nested-span self-check runs first. The independent reviewer parsed raw files
separately, recomputed LOC and verified all artifact hashes after resource-sensitive
collection finished. Existing collection directories deliberately refuse reruns;
future experiments need a newly declared campaign/output and retained failures.

## Disposition

Accept the bounded batching/reuse implementation and its reproduced work-reduction
evidence. Defer subtree-format and save-streaming changes: remaining inode spans
include reference work, and candidate clone handoff is only49,171,738/87,294,823ns,
which cannot be substituted for the multi-second accept cost. Do not remove
validation or add workers to obtain a lower number.

Steps1–4 of the requested campaign are delivered; step5 is the documented decision
to defer unsupported deeper changes. #190 remains open for its historical tripwire,
verification target, golden-pin and qualification gaps. Every step was posted to
the issue, including the rejected implementation; the issue body links those updates.
