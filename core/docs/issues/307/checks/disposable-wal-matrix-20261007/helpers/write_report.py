from pathlib import Path
import json
base=Path('core/docs/issues/307');checks=base/'checks/disposable-wal-matrix-20261007'
data=json.loads((checks/'29-matrix-report.json').read_text())
rows={x['case']:x for x in data['rows']};prefix='phase7-sqlite-disposable-'
init=[rows[prefix+f'init-{n}-owner-wal-v'+('2' if n==100000 else '1')] for n in (100,1000,10000,100000)]
history=[rows[prefix+f'history-stride{s}-group-rows-indexed-wal-v1'] for s in (10,3,1)]
text='''# Disposable WAL: namespace Init and history matrix

> **Status:** Dated measurement checkpoint; not release evidence or a product contract.

The owner-requested four namespace Init sizes and history strides10/3/1 have
all executed once and passed functional verification, content-cold checks,
command/proof limits and terminal cleanup. The matrix is **not all PASS**:
Init100/1000/10000 miss the historical1.10 time arithmetic, and stride1 exceeds
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
'''
inputs=json.loads((checks/'11-build-inputs.json').read_text())
for name,b in inputs['binaries'].items():text+=f"|{name}|`{b['sha256']}`|\n"
text+='''
## Namespace Init

The product clock includes fresh Store creation, public Init and required
seal/checked close. Complete performance includes source-cold attestation and
terminal observations; fixture preparation/build and the independent verifier
are separate. All command/proof limits are the original30s/19s allowances.
Original allocated bytes below are observed after the producer and before proof.
The removed strict-allocation mechanism has no new PASS: NOT_RUN remains its status.

| Files | Product ns | Complete performance ns /30s | Proof ns /19s | Original allocated Store B | Functional / cold / cleanup | Receipt |
| ---: | ---: | ---: | ---: | ---: | --- | --- |
'''
for n,r,receipt in zip((100,1000,10000,100000),init,('13-init-100-v1','14-init-1000-v1','15-init-10000-v1','24-init-100000-owner-wal-v2')):
 text+=f"|{n}|{r['product_ns']}|{r['complete_performance_ns']}|{r['verification_ns']}|{r['allocated_store_bytes_at_measurement']}|PASS / PASS / PASS|[{receipt.split('-')[0]}](checks/disposable-wal-matrix-20261007/{receipt}/receipt.json)|\n"
text+='''
The prior Disposable MEMORY incumbent at `2fced797d` is a retained comparison,
not a newly run arm. The following is the existing arithmetic
`10 × current_ns <= 11 × historical_ns`, not matched-harness admission.

| Files | Historical MEMORY ns | Current difference ns / percent | 1.10 arithmetic | Original allocated-byte difference |
| ---: | ---: | --- | --- | ---: |
'''
for n,r in zip((100,1000,10000,100000),init):
 c=r['comparison'];text+=f"|{n}|{c['reference_ns']}|{c['time_delta_ns']:+d} / {c['time_delta_percent']:+.6f}%|{c['historical_speed_arithmetic']}|{c['storage_delta_bytes']:+d}|\n"
text+='''
The accepted earlier1000-file WAL point remains198720291ns. This new owner-selected
observation is186204834ns (−6.298027%); neither replaces the other or establishes
repeatability. The older eight Init speed failures and eight strict-allocation
failures remain unchanged. WAL, allocation lifecycle and other implementation
changes are not isolated here, so the whole delta is not attributed to WAL alone.

The separate namespace oracle covers every path/kind and sampled file metadata
and content. It is not a full-byte oracle of all input files.

| Files | All paths | Selected files | Authenticated selected B | Total fixture B |
| ---: | ---: | ---: | ---: | ---: |
'''
for n,r in zip((100,1000,10000,100000),init):
 p=r['proof'];text+=f"|{n}|{p['paths']}|{p['sampled_files']}|{p['sampled_bytes']}|{p['manifest_bytes']}|\n"
text+='''
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
'''
for s,r,receipt in zip((10,3,1),history,('25-history-stride10-group-rows-indexed-wal-v1','26-history-stride3-group-rows-indexed-wal-v1','27-history-stride1-group-rows-indexed-wal-v1')):
 c=r['comparison'];text+=f"|{s} / {r['proof']['states']}|{r['product_ns']}|{r['complete_performance_ns']} /{r['command_budget_ns']//10**9}s|{r['verification_ns']} /{r['verification_budget_ns']//10**9}s|{r['allocated_store_bytes_at_measurement']} /{c['allocation_ceiling']}|{c['allocation_gate']}|[{receipt[:2]}](checks/disposable-wal-matrix-20261007/{receipt}/receipt.json)|\n"
text+='''
All three rows pass semantic proof, cold eligibility, command/proof budgets and
cleanup. All pass the historical1.10 speed arithmetic against the independent
Phase4.5 reference retained from the previous matched campaign. These are not
new matched speed results. The original approved allocation ceilings retain
all their meaning, including the stride1 FAIL.

| Stride | Historical independent reference ns | Current time delta | Prior Disposable incumbent ns | Current delta vs incumbent | Allocated B delta vs incumbent |
| --- | ---: | ---: | ---: | ---: | ---: |
'''
for s,r in zip((10,3,1),history):
 c=r['comparison'];v=r['versus_retained_incumbent'];text+=f"|{s}|{c['reference_ns']}|{c['time_delta_percent']:+.6f}%|{c['incumbent']['comparison_ns']}|{v['time_delta_percent']:+.6f}%|{v['allocated_delta_bytes']:+d} ({v['allocated_delta_percent']:+.6f}%)|\n"
text+='''
| Stride | Canonical objects / B | All structural path-states | Selected content paths / authenticated B | Acquired proof B | Root / custody proof |
| --- | --- | ---: | --- | ---: | --- |
'''
for s,r in zip((10,3,1),history):
 p=r['proof'];c=r['canonical_census'];text+=f"|{s}|{c['canonical_objects']} /{c['canonical_bytes']}|{p['paths']}|{p['sampled_content_paths']} /{p['authenticated_bytes']}|{p['acquired_content_bytes']}|CHECKED / CHECKED|\n"
text+='''
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
LAYERFS_CAMPAIGN=disposable-wal-matrix-20261007 LAYERFS_CONSTRUCTION_WORKERS=1 \\
python3 -B core/docs/issues/307/checks/incumbent-restoration-20261007/measure.py \\
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

The following results-only commit has its own exact comparison in receipt31.
The counter is `tools/production_loc.py`, SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`.
No reference or transport code is retired in this campaign.
'''
(base/'DISPOSABLE-WAL-MATRIX-20261007.md').write_text(text)
