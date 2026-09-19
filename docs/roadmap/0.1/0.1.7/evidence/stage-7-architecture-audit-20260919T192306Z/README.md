# Stage 7 — C1/C2 architecture audit, first pass

> **Status:** Research; informative and not a product contract.

Owner-requested parallel review of production LOC, simplifications, public API
boundaries and configuration. Product source was read from a frozen snapshot;
other agents continued Stage 6 in the shared checkout. No product files were
changed and no compilation, tests or benchmarks were run by this review.

The source is HEAD `66bce8378b5e9ecb1135b1636f8ee2ffac46ccf5` plus the captured
working-tree product changes in [reviewed-product.patch.gz](reviewed-product.patch.gz).
The [snapshot manifest](snapshot.json) records per-file SHA-256 values; no file
changed during capture. The v0.1.6 comparison tag resolves to
`dbdf0fed6fceba9f72997287eaa7d7ee9ae0fd79`.

This is the first Stage 7 audit, not Stage 7 acceptance. Cross-revision substitution
and runtime integration are not claimed by source inspection.

## Reports

- [Production LOC and simplifications](loc-and-simplifications.md): exact
  same-counter totals, scope limitations, 18 implemented work reductions and two
  structural migration improvements. Historical operation counters are distinct
  from current source inspection and from a v0.1.6 performance comparison.
- [Public APIs and architecture](public-apis.md): the recommended #179 caller
  surface, actual save/read composition, exposure of implementation APIs and seven
  flexibility findings.
- [Configuration](configuration.md): four persisted policy knobs, per-operation
  resources, fixed versus derived values, examples and source-confirmed gaps.

## Synthesis

The C1 provider/consumer boundary and C2 Store facade support integration design.
Source inspection does not establish independent revision substitution. A
compatible algorithm replacement must preserve the supported caller interface,
canonical profile, physical-format compatibility, resource behavior and errors.
The snapshot itself contains a schema-changing optimization, illustrating why
unchanged Store method signatures do not establish persisted-data compatibility.

| Production scope | v0.1.6 | Committed HEAD | Frozen working snapshot |
| --- | ---: | ---: | ---: |
| Reference product | 65,417 | 65,417 | 65,417 |
| Replacement C1 | absent | 12,303 | 12,512 |
| Replacement C2 | absent | 6,453 | 6,819 |
| Replacement telemetry | absent | 763 | 763 |
| Combined product | 65,417 | 84,936 | 85,511 |

Old content + layerstack-store total 34,077; replacement C1+C2 total 19,331.
The package footprint is 14,746 LOC / 43.27% smaller, but old packages also own
history, runtime and deferred features. Equivalent-functionality savings remain
unestablished. Keeping both generations means no reference-retirement saving has
yet occurred. These totals include runtime SQL and exclude comments, blanks,
tests, examples, benchmarks and tooling.

Prioritized review findings:

1. **Persisted compatibility needs explicit qualification.** Captured schema 6
   requires the new content-signature table; older schema identities are refused.
   Record algorithm-only versus schema-changing revisions and test compatibility
   separately from caller compilation.
2. **Define the supported integration surface.** Both crates expose more than
   #179 needs. SQL, pack, reducer and cache implementation APIs invite accidental
   coupling. A historical reducer optimization already changed a public return
   type and the budget-derived touched-serial ceiling. C2 errors also expose an
   engine-specific type. Restrict or document surfaces based on real callers;
   a new plugin framework is not the remedy.
3. **Validate supplied capacities as a coherent contract.** Construction policy
   validation does not validate the separate mutable capacity structure.
   Filesystem batch-derived allocation arithmetic lacks complete upper checks;
   the named 4 MiB scratch maximum is enforced as a caller-selected budget rather
   than a hard ceiling. Runtime consequences are inferred from source;
   reproduction remains NOT_RUN.
4. **Make replacement assembly reproducible.** C2's path dependency and its
   caller must resolve the same C1 package instance. Selecting another worktree
   only for the caller can create distinct Rust types despite matching names.
5. **Specify unfinished-save needs in pair #179.** Published-base reads plus new
   writes are supported, and the current C1 update keeps intermediate results
   locally. Chaining a second C1 operation against an unpublished first result
   would need an explicit same-save bridge; it is a conditional integration
   requirement, not a blocker for the existing single-call route.

The reports also identify error-detail and timing propagation gaps, implicit
provider batch limits, and physical-depth fields living in C1's policy type.
Those findings require disposition against the chosen integration contract.

## Substitution evidence and next checks

| Composition | Source assessment | Executed proof in this audit |
| --- | --- | --- |
| Baseline C1 + baseline C2 + unchanged consumer | Existing public pipeline tests are suitable starting points; prior Stage 6 evidence has its own pin | NOT_RUN |
| Optimized C1 + baseline C2 + unchanged consumer | Feasible within the canonical/public contract and one package identity | NOT_RUN |
| Baseline C1 + optimized C2 + unchanged consumer | Feasible within public and persisted-format contracts; schema-changing combinations require explicit treatment | NOT_RUN |
| Optimized C1 + optimized C2 + unchanged consumer | Composition is plausible; it does not prove individual replacement | NOT_RUN |

Next, disposition the interface and configuration findings, select real compatible
algorithm revisions, then run the unchanged-consumer matrix and cross-revision
Store readback. A schema mismatch must be reported as incompatible rather than
silently migrated. Keep product fixes and associated tests separate from this
read-only review and coordinate execution with Stage 6's measurement lock.

## Evidence integrity and validation

The captured product patch was applied to HEAD in isolated scratch space;
all 476 captured files under `core/crates/` and `crates/` matched the manifest
SHA-256 values. See [reconstruction-check.json](reconstruction-check.json).
The manifest includes supporting documents and the counter as well as product
files. The counter SHA-256 is
`c6e853c2280e2ee96caa6cafff4b221fa9201e1f5bf40d12e8251f70387210fc`.

Exact totals: [v0.1.6](loc-v016.json), [HEAD](loc-head.json),
[working snapshot](loc-working.json). Per-file classification and production LOC:
[v0.1.6](loc-v016-files.json), [HEAD](loc-head-files.json),
[working snapshot](loc-working-files.json).

[Post-capture drift](post-capture-drift.json) records later changes separately;
it does not update this audit's source pin. Existing test source and historical
receipts were inspected, but no fresh product test result is claimed. Local
documentation links and whitespace were checked. Stage 7 remains open.
