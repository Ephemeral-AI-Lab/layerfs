# #273 side-lane integration into #264 Phase 4.5

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Prepared 2026-09-29 from documentation/test source
> `64e0e81a7f702e505eae8f3f935bac42de19d312` and product source
> `11a864fc133844cae7a4247b1f243d84d5763b10`.

This folder starts the requested merge-planning record with the benchmark
organization and retained baseline inventory. The main lane is #264 mounted
namespace; the side lane is #273 active backing, file edits and Commit.

- [Architecture refinement](architecture_refinement.md): detailed ASCII
  workflows and source-derived before/after time, backing-space and resident
  memory comparisons. It separates standalone namespace improvements from the
  actual combined active implementation and records remaining costs/limits.
- [Benchmark layout and verification plan](BENCHMARKS.md): proposed
  `core/benchmark/fs-bench-pro/families/` ownership, naming, migration map,
  registered selections and proposed extensions.
- [Baseline inventory](BASELINE-20260929.md): the complete retained nine-cell
  Workspace reference, original #248 case, historical SDK Init and history
  references, their identities, unavailable controls and qualifications.
- [Machine-readable baseline metadata](baseline-20260929.json): exact retained
  timing integers, statuses, source/image/seal identities and receipt hashes.

No physical benchmark is run for this documentation task. The proposed family
files are not created by this checkpoint. Existing runners, workloads, receipts
and historical IDs keep their identities. The frozen pre-#273 comparison
control remains NOT_RUN; Workspace reference timings remain INELIGIBLE.
The owner directs functional/algorithm correctness first and defers additional
memory qualification to #283.

## Source and integration boundary

The owned branch already contains #269 head
`6eb7553671d3000160ad023a57b335e52dd81a26`, the C1 ordering repair and the
rename one-seal repair. That ancestry does not mean the combined active path
retains every standalone #264 complexity benefit. It still stores inline and
extended paths, prepares resident replacement paths on rename and walks
inherited descendants when the destination increases depth. Its active path
bound remains 65,536 bytes / 256 components. Benchmark namespace scaling on
the actual integrated code; do not import standalone complexity or timing
claims merely because its commits are ancestors.

The [side-lane functional report](../PREMERGE-FUNCTIONAL-COMPLETION-20260929.md)
and [namespace implementation record](../../245/PHASE4_5_IMPLEMENTATION.md)
have different proof identities. The [integration check](../PHASE45-INTEGRATION-CHECK-20260929.md)
records later namespace regression work. Paused readable-fixture/test-note
edits in the working tree are outside this documentation commit and do not
establish a final all-routes PASS.

## Rules for the later campaign

Use the [repository benchmark rules](../../../../../docs/general/benchmark_rules.md)
and [Core benchmark instructions](../../../../benchmark/fs-bench-pro/AGENTS.md).
Select the baseline source and complete method before either arm runs. Retain
one attempt per selected case/arm with independent verification and cleanup;
keep failures and ineligible rows append-only. No fastest-run selection,
cross-profile pooling, new quota/deadline/worker allowance, or retrospective
receipt promotion follows from this plan. This is a review record, not merge
or release permission.
