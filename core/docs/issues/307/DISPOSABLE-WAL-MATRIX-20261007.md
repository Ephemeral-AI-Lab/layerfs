# Disposable WAL: namespace Init and history matrix

> **Status:** Dated measurement checkpoint; not release evidence or a product contract.

The owner-requested four namespace Init sizes and history strides10/3/1 have
all executed once and passed functional verification, content-cold checks,
command/proof limits and terminal cleanup. The matrix is **not all PASS**:
Init100/1000/10000 miss the newer-incumbent1.10 time arithmetic; all four
miss the original cluster-one-end arithmetic, and stride1 exceeds
its original allocated-storage ceiling. All comparisons are single, historical,
unpaired observations; no release speedup or full resource qualification is claimed.

Disposable/WAL/OFF is now the only permitted global Store execution profile.
Durable is disabled indefinitely until explicit owner reauthorization, under the
[root instructions](../../../../AGENTS.md#active-persistence-profile--owner-direction-2026-10-07).
The retained Durable implementation may compile; it was not executed. Overlay
MEMORY/OFF/EXCLUSIVE remains a distinct local database and is not part of these
component measurements.

The [original frozen selection](../../../../docs/roadmap/0.1/0.1.7/disposable-wal-init-history-20261007.md)
and [100k source-cold correction](../../../../docs/roadmap/0.1/0.1.7/disposable-wal-init-cold-correction-20261007.md)
precede their samples. Every original receipt is retained in
[the campaign checks](checks/disposable-wal-matrix-20261007/). The
[machine report29](checks/disposable-wal-matrix-20261007/29-matrix-report.json)
contains exact identities, arithmetic, original allocation, later file-stat
observations and all eight selections, including the zero-sample refusal.

## Identity and measurement scope

- Init100/1000/10000 and the original100k cold refusal: source
  `5ba8362cb82ce8f210f2cc0fcdb8affffcc1d72b`.
- Corrected Init100k and strides10/3/1: source
  `06dc05a7d6a8e77fb1c5ffa532225ad635920275`. Native/product inputs and binaries
  are unchanged; only the explicit prepared-source selection and its tests/docs
  change. Source/tree, compilation, dependency, harness and fixture identities
  are retained per row. Git dirtiness is exactly the three owner-preserved,
  hash-recorded non-input notes; the whole checkout is never labeled clean.
- Locked Rust1.85.1 release; repository ARM64 inputs; environment construction
  workers=1. Init uses its existing four constructors; history has one producer.
- Native macOS arm64, system SQLite3.51.0, global profile `sqlite-wal-off-v2`.
  Image/daemon/FUSE/Exec are N/A. These are the public Project Init and retained
  C1/C2/C5 component operations, not integrated Workspace latency. Init uses
  Monolithic acquisition schema4; history uses GroupRowsIndexed schema3.
- Source-content zero residency is enforced by the sealed native helper;
  history also checks Store/extant-WAL content at each state boundary. Filesystem
  metadata residency and bounded product buffers retain their declared exclusions.
  Prepared sources and the pinned immutable history corpus are reused. Every
  measured Store is freshly created inside the measured lifecycle; no state is
  removed from the operation through preparation.
- Background Docker services remain running and are recorded. Spotlight readers
  on the first100k fixture are separately proved and preserved. No unrelated
  process is interrupted. The external history observer remains in the timed
  driver. Lifetime wait4 RSS/CPU are recorded as such, never phase maxima.

Binary SHA256 values (receipt11; rechecked by28):

| Executable | SHA256 |
| --- | --- |
|benchmark_init|`65285aded5a4dc68665f646d0bee077369c8578ee60ac3456b8f11b836177393`|
|verify_namespace|`c8b8f303e68ed9ae6761c9d3971526933ee44e55f52803706711b017ec37c736`|
|benchmark_history|`2bd92bd2d79ce4ffa827b3be08c8ac005887572b4d72ef6a07d34d9f20af1a62`|
|verify_history|`af5e1508cca2a7ecaff356582ea9910e0ad6073230090b75c03b9735bd04cdaf`|

## Namespace Init

The product clock includes fresh Store creation, public Init and required
seal/checked close. Complete performance includes source-cold attestation and
terminal observations; fixture preparation/build and the independent verifier
are separate. All command/proof limits are the original30s/19s allowances.
Original allocated bytes below are observed after the producer and before proof.
The removed strict-allocation mechanism has no new PASS: NOT_RUN remains its status.

| Files | Product ns | Complete performance ns /30s | Proof ns /19s | Original allocated Store B | Functional / cold / cleanup | Receipt |
| ---: | ---: | ---: | ---: | ---: | --- | --- |
|100|63918250|1044272917|579196916|5259264|PASS / PASS / PASS|[13](checks/disposable-wal-matrix-20261007/13-init-100-v1/receipt.json)|
|1000|186204834|275462375|31228250|20590592|PASS / PASS / PASS|[14](checks/disposable-wal-matrix-20261007/14-init-1000-v1/receipt.json)|
|10000|2327953500|2971469750|352048167|321703936|PASS / PASS / PASS|[15](checks/disposable-wal-matrix-20261007/15-init-10000-v1/receipt.json)|
|100000|7495571834|14681307917|1108929291|532172800|PASS / PASS / PASS|[24](checks/disposable-wal-matrix-20261007/24-init-100000-owner-wal-v2/receipt.json)|

The prior Disposable MEMORY incumbent at `2fced797d` is a retained comparison,
not a newly run arm. The following is the existing arithmetic
`10 × current_ns <= 11 × historical_ns`, not matched-harness admission.

| Files | Historical MEMORY ns | Current difference ns / percent | 1.10 arithmetic | Original allocated-byte difference |
| ---: | ---: | --- | --- | ---: |
|100|45688500|+18229750 / +39.900084%|FAIL|+4096|
|1000|155291459|+30913375 / +19.906681%|FAIL|+0|
|10000|1943650833|+384302667 / +19.772207%|FAIL|+16232448|
|100000|7423399375|+72172459 / +0.972229%|PASS|+16592896|


The original cluster-one-end controls at `197d2fb7d` also remain visible; they
are distinct from the later restored MEMORY incumbent above. All four current
values exceed their original1.10 arithmetic. The newer100k incumbent comparison
being within10% does not close this older target. The accepted1000-file WAL point
and its owner disposition retain their separately documented scope. No new
measurement or threshold is introduced by this additional arithmetic.
[Receipt32](checks/disposable-wal-matrix-20261007/32-original-init-targets.json)
hash-checks the original compact controls and records both integer operands.

| Files | Original cluster-one-end ns | Current delta | Original1.10 arithmetic |
| ---: | ---: | ---: | --- |
|100|38747750|+64.959901%|FAIL|
|1000|129258375|+44.056301%|FAIL|
|10000|1645276292|+41.493165%|FAIL|
|100000|5558569958|+34.847126%|FAIL|

The accepted earlier1000-file WAL point remains198720291ns. This new owner-selected
observation is186204834ns (−6.298027%); neither replaces the other or establishes
repeatability. The older eight Init speed failures and eight strict-allocation
failures remain unchanged. WAL, allocation lifecycle and other implementation
changes are not isolated here, so the whole delta is not attributed to WAL alone.

The separate namespace oracle covers every path/kind and sampled file metadata
and content. It is not a full-byte oracle of all input files.

| Files | All paths | Selected files | Authenticated selected B | Total fixture B |
| ---: | ---: | ---: | ---: | ---: |
|100|102|53|3354003|5000000|
|1000|1011|70|6430827|20000000|
|10000|10101|72|101928859|300000000|
|100000|101001|73|200286236|500000000|

## Retained history

Each row performs all registered states, construction, Save, History custody,
required cold boundaries, final seal/close and canonical census. The complete
performance clock also pays the initial corpus cold attestation. The independent
proof uses an actual byte copy, all-state structure, retained independent expected
roots and the existing bounded five-anchor content oracle. All proof copies and
original closed Stores remain retained. One combined Store holds C2/C5; no
fabricated split-file allocation is reported.

| Stride / states | Product ns | Complete performance ns / bound | Proof ns / bound | Original allocated Store B / ceiling | Storage | Receipt |
| --- | ---: | --- | --- | --- | --- | --- |
|10 / 17|33380784333|54282765417 /60s|5573308125 /12s|51384320 /54278964|PASS|[25](checks/disposable-wal-matrix-20261007/25-history-stride10-group-rows-indexed-wal-v1/receipt.json)|
|3 / 53|68569406416|87685327334 /170s|7066451250 /12s|64872448 /70427034|PASS|[26](checks/disposable-wal-matrix-20261007/26-history-stride3-group-rows-indexed-wal-v1/receipt.json)|
|1 / 157|182854812667|204333286708 /300s|17267117916 /30s|101498880 /92342273|FAIL|[27](checks/disposable-wal-matrix-20261007/27-history-stride1-group-rows-indexed-wal-v1/receipt.json)|

All three rows pass semantic proof, cold eligibility, command/proof budgets and
cleanup. All pass the historical1.10 speed arithmetic against the independent
Phase4.5 reference retained from the previous matched campaign. These are not
new matched speed results. The original approved allocation ceilings retain
all their meaning, including the stride1 FAIL.

| Stride | Historical independent reference ns | Current time delta | Prior Disposable incumbent ns | Current delta vs incumbent | Allocated B delta vs incumbent |
| --- | ---: | ---: | ---: | ---: | ---: |
|10|34544042250|-3.367463%|33708739000|-0.972907%|+692224 (+1.365546%)|
|3|72230709750|-5.068901%|66838917708|+2.589044%|+626688 (+0.975454%)|
|1|192712692209|-5.115324%|185189368250|-1.260632%|+16326656 (+19.168991%)|

| Stride | Canonical objects / B | All structural path-states | Selected content paths / authenticated B | Acquired proof B | Root / custody proof |
| --- | --- | ---: | --- | ---: | --- |
|10|51689 /380559460|101477|67 /970326|3301773|CHECKED / CHECKED|
|3|73447 /589480854|306861|66 /921174|3689369|CHECKED / CHECKED|
|1|104618 /871337620|904143|66 /921174|5347088|CHECKED / CHECKED|

Historical separate worktree census/root-pin files are unavailable. The immutable
retained baseline receipts still contain the independent producer roots and
recorded native proof. Their hashes are pinned; current roots are compared with
those expectations. The current closed SQL/canonical census and current native
namespace/content proof actually run, rather than treating missing old files as
revalidated evidence.

## Allocation failure and physical file observations

Stride1 original allocation101498880B exceeds92342273B by9156607B
(**9.915943%**). Its logical file is85348352B: the original allocation exceeds
file length by16150528B. The independent proof copy has the same85348352B
length and SHA256 `94613822a21812af141537decea420256f5a6118a01bcce59d6796d4828fac19`,
but occupies85348352B. Thus the observed original-file allocation excess is
separate from encoded-file length. The exact allocator/VFS cause is not isolated
by these measurements. A smaller proof copy cannot replace the original Store
in this gate, and the failure remains FAIL.

Init10000 similarly has logical length305471488B and original allocation321703936B.
Init100000 originally reports532172800B allocated at the measurement observation;
the later post-proof file-stat snapshot is522747904B at unchanged logical length
515612672B. The later observation is retained separately and never substituted
for the measured original allocation. These filesystem observations do not
establish daemon/Linux-volume storage or a new storage tolerance.

## Failed, ineligible and unrun selections

- Three Init historical speed-arithmetic misses and stride1 allocation FAIL,
  exactly as tabulated. No limit or old verdict was changed.
- Original100k v1: INELIGIBLE, zero product attempts,1626 resident pages after
  source invalidation. Receipt17 shows Spotlight holding30 descriptors on15
  fixture files. The original fixture and refusal are preserved.
- The prospective100k v2 copies every original byte with a128KiB window into
  an owned `.noindex` preparation directory and preserves all metadata.20/21
  verify100000 files/500000000B and the exact same manifest. No global Spotlight
  setting or unrelated process changes. Its in-run zero-resident-page check
  passes; directory naming itself is not evidence of coldness.
- Receipt03 retains one harness-test failure caused by the new Durable refusal
  preceding an older missing-pin expectation. The corrected test uses Disposable;
  separate tests cover the Durable ban.04 passes39 owning tests;19 passes12
  affected tests after the new source placement. No failed product was replayed.
- Durable: **NOT_RUN — disabled by owner until explicit reauthorization**.
  No MEMORY-profile baseline was executed. No Linux daemon/FUSE/Exec or S10
  normalization operation was measured. Phase-local RSS, exact native allocation
  attribution and exclusive physical I/O remain unqualified; wait4 peaks are
  lifetime values in29. No new numerical resource limit is inferred.

## Checks, custody and reproducibility

Receipt02 builds all four locked release examples.05 all-target Project Clippy,
06 formatting,07 the706-file product-boundary scan and08's20 directly invoked
boundary guard tests pass. The source-only placement correction reuses these
unchanged native checks and binaries (22). Receipt28 independently validates121
original manifest-bound files, all used binaries, native proof/count/cache/limit
outcomes and exact comparisons in1.473605542s complete command. This validates
receipt integrity, not a claim that the failed numerical gates passed.

The existing runner executes each sample through the retained campaign launcher:

```bash
LAYERFS_CAMPAIGN=disposable-wal-matrix-20261007 LAYERFS_CONSTRUCTION_WORKERS=1 \
python3 -B core/docs/issues/307/checks/incumbent-restoration-20261007/measure.py \
/Users/yifanxu/Ephemeral-AI-Lab/layerfs <registered-case-id> candidate <outer-stop-s>
```

All exact commands and outer stops are in each copied `invocation.json`:100s
for Init,110/220/370s for the three explicitly selected long history performance
families. Their inner original60/170/300s and separate12/12/30s budgets remain
enforced. Ordinary tests all use100s stops. No test hangs or unknown product
outcomes occurred. Setup, complete performance and independent proof are distinct.
Original launcher-artifact creation timestamps are retained in29 as filesystem
observations, not invented wall-clock fields in older raw receipts.

Seven original closed Stores and three closed independent proof copies remain
in the owned ignored result directories for bounded evidence retention, including
the failing stride1 allocation. Source preparations and all textual receipts are
retained; no failed artifact is deleted. The three protected notes, four protected
containers and root reference remain intact. No new container, worktree, push,
deployment, dependency or production-source change.

## Production LOC

| Commit | Change | Combined | Core | Active | Reference | Excluded predecessor / integration | Receipt |
| --- | --- | --- | --- | --- | --- | --- | --- |
|`54d9c7457`|Profile policy and frozen plan|165673→165673 (+0)|100256→100256|57382→57382|65417→65417|36325 /6549 unchanged|01|
|`5ba8362cb`|Registered WAL harness and verifier|165673→165673 (+0)|100256→100256|57382→57382|65417→65417|36325 /6549 unchanged|12|
|`5a2ce43ec`|Retained Init rows and cold correction plan|165673→165673 (+0)|100256→100256|57382→57382|65417→65417|36325 /6549 unchanged|18|
|`06dc05a7d`|Explicit corrected source selection|165673→165673 (+0)|100256→100256|57382→57382|65417→65417|36325 /6549 unchanged|23|

Results commit `e8e381db9` has its exact unchanged comparison in receipt31.
The following documentation-only original-target clarification has its unchanged
comparison in receipt33; no benchmark is rerun.
The counter is `tools/production_loc.py`, SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`.
No reference or transport code is retired in this campaign.
