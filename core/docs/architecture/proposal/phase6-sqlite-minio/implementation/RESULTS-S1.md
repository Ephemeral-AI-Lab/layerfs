# S1: simpler Phase 6 authority — real-provider results

> Status: Research; informative and not a product contract.
> S1 dependency COMPLETE for the declared experimental profile. Full Phase 6 goal open.

Runtime [38b2d88c7b6efc97a4cede2dcbbba3232996ec25](https://github.com/Ephemeral-AI-Lab/layerfs/commit/38b2d88c7b6efc97a4cede2dcbbba3232996ec25).
[Selected architecture](../ARCHITECTURE.md), [frozen delivery](S1-SPEC.md),
[runtime and owning checks](S1-RUNTIME.md), [immutable live evidence](s1-live-cohort/).
Owning issue #293; #294 supplies supporting experimental evidence.

## Delivered simplification

The daemon owns writable SQLite metadata and canonical filesystem construction,
exact CAS checks, compression/packing and direct immutable MinIO uploads. The
global service owns two locator tables and C5 identities/history/conditional Branch
publication. Seven duplicate certification/index modules and eleven catalog tables
were removed. Normal publication performs one root-locator lookup and fixed context/
C5 operations; no second namespace proof or index installation. Experimental runtime
physical source 6485 -> 5319 (-1166), including comments/blanks. This is distinct
from shipped product LOC, which is unchanged.

All actual normal-service MinIO counters remain equal to the recorded genesis
baseline at every known publication: put_calls3/get_calls0/uploaded1254B/received0B.
Thus each Commit issues zero additional authority PUTs/GETs and receives zero pack
bytes. LocatorDb has no MinIO client or canonical reader. This is actual issued-client
request accounting plus source structure, not a physical network/cache containment
claim. Daemon uploads, necessary base/dedup reads and independent verifier reads
remain real. Root selection and immutable locator consistency remain globally owned.

## Exact workloads and independent proof

One frozen-source run per required case uses actual public WorkspaceApi.exec,
kernel FUSE, daemon SQLite, C1/C2, immutable MinIO ACKs/locator registration and C5.
Original closed full128 and270 fixtures plus their original three commands/manifests
are reused byte-exact. A fourth generic command verifies the publisher boundary:
UID/GID65534; no inherited private/service/MinIO environment; no access to supervisor
/proc/1/environ or private backing; no signal permission to supervisor; zero effective,
permitted,inheritable,ambient capabilities and NoNewPrivs1. It changes no filesystem
content and produces exact clean UpToDate. Commands are not recognized by the daemon.

Separate readonly proofs validate every declared path/kind/mode/size and every file
SHA256 at all four states, plus exact Commit IDs/roots/parent history. Each case has
four publications and three created Commits. Original128 final137files/1115288B;
270 final4files/39B plus all directory bindings. Ordinary shallow many/f0 edits and
existing deepest270file edits pass. Independent root4KiB create/overwrite smoke
passes both literal byte/mode states and history. All successful cases explicitly
unmount with zero handles, delete their sandbox/volume, drain metadata and MinIO
owners, and retain provider data/receipts. Canonical root vectors and physical
resource containment remain NOT_RUN; known-reply root seals are not expected pins.

## Raw observations

|Case / step|SDK Exec ns|SDK Commit ns|Publication RPC ns|Bytes/modes/history|Numeric cache|
|---|---:|---:|---:|---|---|
|many128 / seed|46601417|47094959|560625|PASS|INELIGIBLE|
|many128 / target|145411042|109523167|636083|PASS|INELIGIBLE|
|many128 / localized-successor|3329917|38156000|497166|PASS|INELIGIBLE|
|many128 / publisher-boundary|4305000|16806375|1385292|PASS|INELIGIBLE|
|components270 / seed|26388000|37425209|654708|PASS|INELIGIBLE|
|components270 / target|266234584|159626042|583417|PASS|INELIGIBLE|
|components270 / localized-successor|101474666|41733083|667000|PASS|INELIGIBLE|
|components270 / publisher-boundary|5041375|15975291|659208|PASS|INELIGIBLE|
|root-smoke / create|13431041|36377750|619084|PASS|INELIGIBLE|
|root-smoke / overwrite|13787583|31287792|560833|PASS|INELIGIBLE|

|Selection|Complete child ns / bound|Separate proof ns / bound|Semantic|Cleanup|
|---|---:|---:|---|---|
|many128|6254143375 / 15,000,000,000|3166991084 / 9,500,000,000|PASS|PASS|
|components270|6489480000 / 15,000,000,000|6757839000 / 9,500,000,000|PASS|PASS|
|root-smoke|5907327333 / 15,000,000,000|38272042 / 9,500,000,000|PASS|PASS|

Raw durations use Rust Instant and Python monotonic_ns; milliseconds are derived
from those same integer durations. Nested SDK/service/child spans are not added.
The collector also fails if authority I/O counters change or publication observation
cardinality differs. Initial image defect, corrected fixture image and retained idle
failed container are declared below; no build overlapped a timed child. Other host
activity/cache cannot be certified. All speed values INELIGIBLE for unknown cache,
not a matched speed, benchmark/release admission or repeatability claim.

## What was removed from the expensive path

Retained historical runtime ac0497cbb4aba1ad493d6f36659ea9f3824b233e:

|Case|Historical Commit ms|Current Commit ms|Historical publication ms|Current publication ms|Historical authority GETs / B|Current GETs / B|
|---|---:|---:|---:|---:|---|---|
|128file mutation|628.198625|109.523167|532.306917|0.636083|927 / 6369311|0 / 0|
|270component initial Commit|1284.607125|159.626042|1112.926709|0.583417|1937 / 14218069|0 / 0|

These are historical versus current uncontrolled-cache observations, not an eligible
paired speed comparison. Source/profile changed (including actor isolation, bounded
SQL corrections and reservation pages); do not attribute every timer delta solely
to one change. The explicit removed authority GETs and certification/index work
explain the architectural reduction. Initial constructor still processes actual new
files/directories. SDK mutation time and provider encoding are separate remaining
costs. Current initial publication context/stage/commit spans:12863875/120250/112583ns;
27070916/128500/117250ns, actual nonoverlapping subphases, not the entire RPC span.

128target retains10lookup/15registration calls,13daemonPUTs,plus2actual64serial
reservation-page calls. 270target15lookup/31registration/4reservation calls. One
persistent authenticated metadata connection throughout each successful case.
No SQLite engine creation per Commit. The deep successor Exec101.474666ms is
recorded for correctness only; its latency optimization remains deferred. Shallow
successor3.329917ms Exec/38.156000ms Commit; root overwrite13.787583/31.287792ms.
No new speed threshold or implicit cache TTL increase is declared.

## Retained failure and fixture correction

First128attempt at the same runtime failed seed before target construction:
cp could not find /fixtures/phase6/many128. Exit1/1118706958ns, no Commit or
separate proof PASS. Wrong image1c193485fbf2cfb1b3a5210c5f86761404b458481fb66636c066a0a73140584b
was assembled from provider-only base8ea025...without closed fixture layer. All
raw failed evidence remains in initial-fixture-failure; owned failed container
d2433851ea59/layerfs-dbd59ea75fdc62de0a1f6d9d099609ab and volume retained. Its
MinIO process drained and data retained. Idle failed container is declared interference.

Corrected image reuses the previously proved fixture-bearing f1a2fee8...image and
replaces only its executable with the frozen S1 Linux binary. Required fixture
directories admitted; no fixture regeneration, altered original command/oracle,
quota/deadline/worker increase or cache priming. Fresh corrected output; failed
receipt unchanged. The first attempt never passed the target; this is correction
of an image-input defect, not repeating an unchanged passing arm for a nicer time.
Prior historical failed67/270 and V4c3b oracle evidence remains unchanged.

## Source, profile and reproduction

Owned branch codex/phase6-metadata-experiments/worktree
/Users/yifanxu/.codex/worktrees/phase6-sqlite-minio-design/layerfs, publication confirmed.
Native macOS MinIO/global SQL/C5; Linux Docker VM daemon/FUSE. SQLite3.53.2,
MEMORYjournal/OFFsync/cache2MiB/mmap0/busy0, no Workspacefsync/cloud durability.
Root supervisor/nonroot65534 command profile; stronger confinement/multi-tenant
admin attacks remain unqualified. Current512inode/256handle/singleWorkspace/Branch/
producer/serializedcapture/FULL-only experimental encoding remain explicit limits.

Image52f618a114e3a3bb9db0e447e4a09dcfe993857b5e5652c789649a742a40c261;
host9e2f88015dff8da686421bfbf7525533613dec428e5ffa61d81b8fc06e074635;
Linux6245b3c8dee026cbbb5f469aaacbc5e7ee275a60695d5cac32b33cad53450f62;
MinIOb107901fd1afe7b36165c6aa66bb027f96e2ec2b690eebe8c9376746d9a9b0df.
Case128cac09ad197898e9c806e7c816f1073b3a3e1f3dd1b0dddc4aaf01c68d86a07ac;
270ae21f1f3b7c4ccf362e590ce7339824d52c2c34d62960c9b4e6b3b73d9466dca.
Per-run identity pins source,dependency lock,root ARMv8 flags,harness/scenario/case,
image and provider. Raw evidence includes exact proof-plan seals and all roots.
Private provider credentials/logs and binary/provider data are excluded from Git.

Reproduction uses the recorded image/binary and fresh outputs:

```
python3 core/benchmark/phase6-live/run.py --output <fresh> \
 --minio /Users/yifanxu/.codex/worktrees/issue290-storage-probes/layerfs/benchmark-results/storage-probes/deepseek-full-import-v1/minio-provider/minio \
 --image sha256:52f618a114e3a3bb9db0e447e4a09dcfe993857b5e5652c789649a742a40c261 \
 --driver benchmark-results/phase6-integration/binary-archive/9e2f88015dff8da686421bfbf7525533613dec428e5ffa61d81b8fc06e074635/phase6-live-probe \
 --case-file benchmark-results/phase6-integration/s1-many128-trust.case \
 --purpose '<recorded source-covering correctness diagnostic>'
```

270substitutes s1-components270-trust.case; root smoke omits --case-file. Case/image
preparation is reuse of sealed inputs, not a cold-cache claim. Do not rerun these
unchanged passing arms. S1 runtime33unique owning tests/hostClippy/fmt/native/Linux
release passed; exact failed/corrected scope in S1-RUNTIME. Full unchanged Core
suites/examples/boundary/LinuxClippy unrun tool-onlyscope. No CI/preflight.

Each preserved/design/runtime commit has its own actual first-parent/final-staged
product comparison:135696->135696(+0),reference65417/Core70279unchanged. Counter
SHA256c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb over Git
archives with committed-tree confirmation; tools/tests/docs excluded consistently.
This result commit has its own exact comparison in message/issue. No shipping
source retirement, benchmark/release/canonical/physical admission is claimed.

Next S2: replace current fixed daemon population/edit caps with admitted paged SQL
storage, then inherited import/loading. Keep the small global API; do not rebuild
its deleted certification engine. Remaining generic syscalls, capture overlap,
Unknown/provider recovery, delta/pooling/v1 cutover, physical progress, genuine
public SDK Init/allsevenfamilies and complete DeepSeek commands/locality remain
open/NOT_RUN at this source. Full objective active; #288, remote main and issues
remain untouched/open.
