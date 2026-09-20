> Status: Research; informative and not a product contract.

## Step 1 — fresh stride10 phase/read-work diagnostic retained

One baseline sample completed with frozen instrumentation and unchanged product source. Raw diagnostic only: cache residency is uncontrolled, admission/cold-performance comparison remains INELIGIBLE.

| Interval/counter | Exact value |
|---|---:|
| Sum of17 state operations |47,161,768,126 ns|
| Filesystem interval |34,686,235,752 ns|
| Validation |3,429,981,622 ns|
| Directories |11,506,307,751 ns|
| Inodes |4,470,610,374 ns|
| Filesystem residual outside six named children |15,278,643,093 ns|
| Provider-read elapsed, nested inside filesystem |33,559,231,582 ns|
| Filesystem provider waves |110,715|
| Requested/returned canonical objects |117,533|
| Returned canonical bytes |286,247,942 B|
| Provider connection opens / group decodes |16 /513|
| Complete command wall |66,657,423,500 ns|

Provider time overlaps filesystem phases and must not be added again. It includes local Store/SQLite acquisition, reconstruction and authentication; it is not pure disk or codec CPU. This is a fresh baseline, not a rerun that replaces the original32.566-second historical receipt.

The new public-API structural regression test fails on the unchanged product as intended: for five parents in one leaf, omitted parent metadata requires15 acquisitions vs10 for explicit identical metadata (batch64), with equal canonical roots. Two accompanying correctness tests pass. Read-work observer test passes after correcting its initial missing-docs compile failure; all attempts retained.

The repeated-parent batching/reuse proposal is now supported by observed read work and the deterministic regression. Baseline stride3 collection is in progress; a constrained ordering-quota matrix will be run on baseline before product edits, because earlier value insertion could change spill order.

Production LOC unchanged: reference65,417 + core20,116 =85,533 (delta0). Baseline binary SHA256 c5826d20f217a73ed4d814b82c3f8d8cdcaaa7af94ead18c0291918c88d4a855.

Evidence: `.../stage-6-history-190-opt-20260919T232858Z/runs/baseline-history-stride10/`; exact commands/identity/pre/post machine observations/timeout are retained. Raw O3 pinned-counters gate remains INCOMPLETE; no release claim.
