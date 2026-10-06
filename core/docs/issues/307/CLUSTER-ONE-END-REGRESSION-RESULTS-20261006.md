# Restored Monolithic: cluster-one-end regression results, 2026-10-06

> **Status:** Complete eight-case component campaign; regression qualification FAIL.
> Five latency screens PASS, zero storage/joint PASS. S7/S8/S9 remain incomplete.

The restored public Project Init at `7878bbbb40b2d162e03dcb6e4e43da7b63d5b5e4`
is byte-equal in all core product/SQL files to retained `9b74ac035faaf271edd01726913f63f084adaaa2`.
The payload format is withdrawn. Acquisition batching, reservation correction, S7
overlay/daemon accounting and S9 authenticated/runtime/Bridge work remain. All eight
new samples complete with functional root/sample proof, cold content attestation,
cleanup and fixed build/command/proof limits. Performance/storage misses remain.

The main reference is **the same-profile cluster-one-end public Project Init** at
`197d2fb7d0a141d7a9350852022febeec3255bf2`, tree
`dbbe49212b26294024be28986e868c27f4de4825`. Original #302 **candidate** rows
become these controls. The eight [source/closure audits](checks/monolithic-restoration-20261006/cluster-one-end-baseline-audit.json)
pass; their old competitive FAIL/PASS verdicts, case names, dates and seals remain.
The old7edddb Service MEMORY/OFF split Store is reported separately below.

## Same-profile latency screen

`10*current_product_ns <= 11*cluster_one_end_product_ns`. Whole product includes
fresh Store create/open, real public Init, required checkpoint/allocation release
and final checked close. One new sample per case, one clean source; no best-of or
unchanged treatment/baseline replay. The 83,626,792ns historical Monolithic value
versus79,759,708ns is context arithmetic only, not a retroactive PASS.

| Profile / files | Cluster-one end ns | Restored ns | Signed delta ns | Difference % | Speed |
| --- | ---: | ---: | ---: | ---: | --- |
| durable / 100 | 79,759,708 | 108,304,791 | +28,545,083 | +35.788850932% | FAIL |
| durable / 1,000 | 201,566,000 | 216,069,833 | +14,503,833 | +7.195575147% | PASS |
| durable / 10,000 | 2,492,429,625 | 2,690,838,208 | +198,408,583 | +7.960448753% | PASS |
| durable / 100,000 | 7,724,523,333 | 9,777,662,541 | +2,053,139,208 | +26.579493899% | FAIL |
| disposable / 100 | 38,747,750 | 35,765,459 | -2,982,291 | -7.696681743% | PASS |
| disposable / 1,000 | 129,258,375 | 132,802,209 | +3,543,834 | +2.741666836% | PASS |
| disposable / 10,000 | 1,645,276,292 | 1,759,129,584 | +113,853,292 | +6.920010490% | PASS |
| disposable / 100,000 | 5,558,569,958 | 7,008,597,625 | +1,450,027,667 | +26.086343753% | FAIL |

## Strict final allocation screen

Final DB+WAL+SHM must be at most the corresponding cluster-one-end allocated total.
No allowance is introduced. Storage is physical allocated bytes after completion,
not a high-water or a resident-memory observation. Every allocation miss makes the
joint regression gate FAIL even when its latency screen passes.

| Profile / files | Cluster-one end B | Restored B | Signed delta B | Difference % | Storage / joint |
| --- | ---: | ---: | ---: | ---: | --- |
| durable / 100 | 5,255,168 | 5,304,320 | +49,152 | +0.935307872% | FAIL / FAIL |
| durable / 1,000 | 20,545,536 | 20,922,368 | +376,832 | +1.834130781% | FAIL / FAIL |
| durable / 10,000 | 305,070,080 | 308,314,112 | +3,244,032 | +1.063372718% | FAIL / FAIL |
| durable / 100,000 | 514,965,504 | 551,370,752 | +36,405,248 | +7.069453724% | FAIL / FAIL |
| disposable / 100 | 5,222,400 | 5,271,552 | +49,152 | +0.941176471% | FAIL / FAIL |
| disposable / 1,000 | 20,537,344 | 20,897,792 | +360,448 | +1.755085760% | FAIL / FAIL |
| disposable / 10,000 | 305,074,176 | 308,264,960 | +3,190,784 | +1.045904325% | FAIL / FAIL |
| disposable / 100,000 | 514,940,928 | 551,313,408 | +36,372,480 | +7.063427671% | FAIL / FAIL |

## Workload, commands and independent proof

Original seed1 manifest identities:100 files/5MB compact-v3,1000/20MB compact-v3,
10000/300MB and100000/500MB. The closed prepared sources were reused as independent
writable byte copies in this task’s managed init-entry-performance checkout;10000
and100000 were streamed without clonefile/COW and payload hashes checked while
copying. No measured Store was reused. Copy warmth never establishes cold: each arm
separately reports zero whole-input regular-content residency after the same native
helper. Filesystem metadata residency is explicitly unobserved.

Four Init constructors, environment `LAYERFS_CONSTRUCTION_WORKERS=1`, Monolithic
schema4 acquisition, selected Durable WAL/FULL/fullfsync or Disposable MEMORY/OFF,
unchanged ARM64 inputs and capacities. Build/performance caps30s, proof19s.

| Profile / files | Complete command ns /30s | Proof ns /19s | Build ns /30s | All paths | Sample files / B | Functional / cold / cleanup |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| durable / 100 | 143,926,292 | 25,456,833 | 4,351,214,583 | 102 | 53 / 3,354,003 | PASS / PASS / PASS |
| durable / 1,000 | 297,448,333 | 31,708,791 | 82,118,084 | 1,011 | 70 / 6,430,827 | PASS / PASS / PASS |
| durable / 10,000 | 3,295,830,167 | 344,837,167 | 88,297,791 | 10,101 | 72 / 101,928,859 | PASS / PASS / PASS |
| durable / 100,000 | 16,534,815,208 | 3,357,587,000 | 90,224,708 | 101,001 | 73 / 200,286,236 | PASS / PASS / PASS |
| disposable / 100 | 58,299,792 | 16,014,833 | 128,073,625 | 102 | 53 / 3,354,003 | PASS / PASS / PASS |
| disposable / 1,000 | 205,160,208 | 30,451,583 | 74,989,666 | 1,011 | 70 / 6,430,827 | PASS / PASS / PASS |
| disposable / 10,000 | 2,388,841,458 | 343,963,500 | 88,932,917 | 10,101 | 72 / 101,928,859 | PASS / PASS / PASS |
| disposable / 100,000 | 14,086,328,375 | 1,060,398,375 | 75,885,250 | 101,001 | 73 / 200,286,236 | PASS / PASS / PASS |

Proof inventories every path/kind/directory metadata and reads full metadata/bytes
for the deterministic sample, not all payload bytes. All roots match the same-profile
cluster-one controls. Current and baseline public Project acquisition vehicles differ:
current indexed working rows occupy the measured Store; the cluster-one source used
real ordered-run scratch. That regression dimension is disclosed, never substituted
with the uncommitted per-Init SQLite prototype or older Service.

## Attribution and practical limits

Durable100’s observed108,304,791ns versus79,759,708ns is a35.788851% miss.
Bootstrap is38,687,291ns versus17,574,333ns; Init66,429,625ns versus59,264,834ns.
The21,112,958ns bootstrap difference accounts for most of the28,545,083ns whole
difference. This is single-sample attribution, not proof of a repeatable or causal
slowdown. The earlier83.627ms same-product observation remains separate.

Current Durable write commits are18/26/146/549 versus the cluster-one10/17/135/440.
Disposable are18/25/146/546 versus10/17/132/436. The source-defined working-row
acquisition and retained SQLite pages are real paid work. Native SQL statements/VM,
commit/preallocation, whole lifecycle CPU and driver lifetime RSS are in the
[ledger](checks/cluster-one-regression-20261006/ledger.json). SQL/COMMIT/transaction
and phase clocks overlap; no nested sums become the headline. These counts alone
are not exclusive device bytes, a new EXPLAIN campaign or a proof that one mechanism
caused the entire delta. Existing same-source paired plans/profiles remain pinned.

All four other containers are declared unchanged interference. No own measurement
overlap, source dirt or binary/helper/fixture post-seal drift is observed. Resident
heap/pager/journal/kernel or phase/system peaks are UNAVAILABLE; lifetime RSS does
not establish them. No full Workspace/FUSE/daemon/transport or milestone acceptance
is inferred from this public Project component matrix.

## Separate older competitive screen

These contextual calculations use the retained #302 baseline rows at7edddb:
older Service MEMORY/OFF split Store versus the same current samples. They are
separate from the main same-profile controls and do not relabel original verdicts.

| Current profile / files | Old Service ns | Current ns | Competitive speed / storage / joint |
| --- | ---: | ---: | --- |
| durable / 100 | 43,420,250 | 108,304,791 | FAIL / PASS / FAIL |
| durable / 1,000 | 120,408,541 | 216,069,833 | FAIL / PASS / FAIL |
| durable / 10,000 | 1,475,158,041 | 2,690,838,208 | FAIL / PASS / FAIL |
| durable / 100,000 | 6,114,136,834 | 9,777,662,541 | FAIL / FAIL / FAIL |
| disposable / 100 | 41,980,292 | 35,765,459 | PASS / PASS / PASS |
| disposable / 1,000 | 132,300,250 | 132,802,209 | PASS / PASS / PASS |
| disposable / 10,000 | 1,577,107,625 | 1,759,129,584 | FAIL / FAIL / FAIL |
| disposable / 100,000 | 5,920,051,500 | 7,008,597,625 | FAIL / FAIL / FAIL |

## Source, evidence, checks and boundary

Restoration source/tree: `7878bbbb40b2d162e03dcb6e4e43da7b63d5b5e4` /
`5d5d885bd8935041c5740d3c2e158f19fd6a6576`. Exact whole core product seal
`44f07dd216a13d3103440b15b6d295a026ee80878f6e572e468855f27e2e74c5` matches
retained9b source. Source-only withdrawal leaves root reference and S7/S9 code
untouched. Schema7/10 and payload case are unavailable; historical bodies, tests,
proposals, VFS/SQL analyses and all failed/unrun outcomes remain retained.

Checks:260 host bodies and128 Linux portable bodies, no-run builds, host/Linux
warning-denying Clippy, formatting,652-file boundary guard,40 tooling and16 owning
harness tests PASS. Each test invocation had explicit at-most120s timeout. No CI
or aggregate pre-push wrapper applies. Every new candidate raw manifest and full
independent byte copy, driver/verifier/helper/fixture/source post-seal is verified:
[closure manifest](checks/cluster-one-regression-20261006/closed-copy-manifest.json).
All eight selected rows ran; no selected NOT_RUN remains. Older selections retain
their original FAIL/NOT_RUN, and qualified controls were not resampled.

Local commits and exact first-parent production LOC:

- `4b43fa667`: core95,164→95,164; reference65,417→65,417;
  combined160,581→160,581 (delta+0), finalize prior barrier failure.
- `7878bbbb4`: core95,164→94,499; reference65,417→65,417;
  combined160,581→159,916 (delta−665), withdraw newer physical implementation.
  This is format withdrawal, not algorithmic simplification or legacy retirement.
  [Prepared/committed comparison](checks/cluster-one-regression-20261006/restoration-committed-loc.json).
- Final evidence checkpoint: core94,499→94,499; reference65,417→65,417;
  combined159,916→159,916 (delta0), subject to the exact staged/committed receipt.

All comparisons use exact Git first-parent/final staged snapshots and unchanged
`tools/production_loc.py` SHA256c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb.
Shipped SQL and required product code count; tests/inline tests/examples/docs/tooling
do not. Unstaged/untracked unrelated notes are excluded. Earlier experiment commit
comparisons remain in the [payload history](DURABLE100-PAYLOAD-SEGMENTS-RESULTS-20261006.md).

The requested reviewable boundary is reached: scoped restoration and the complete
eight-case final-source campaign are done. Remaining findings are three latency
misses, eight allocation misses and incomplete S7/S8/S9 runtime/resource acceptance.
No further layout/mechanism proposal or implementation, push, release or deployment
is performed by this checkpoint.

Separate #307 updates: [S7 checkpoint](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6016874942) and
[S9 checkpoint](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6016875520).
