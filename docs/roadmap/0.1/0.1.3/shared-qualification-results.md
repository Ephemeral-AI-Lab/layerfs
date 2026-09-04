# Shared infrastructure qualification for issue #22

This is the proposed shared-infrastructure closure evidence for
[#22](https://github.com/Ephemeral-AI-Lab/layerfs/issues/22), using immutable selected qualification anchors and their narrowly scoped
corrected-source follow-ups. It is not completion of any full family,
independent final verification campaign, #35, or `PHASE1_TERMINAL_PASS`.
The coordinator publishes the selected evidence and resolves the report/recovery
audit gates before closing #22. Those corrections are now qualified as recorded
below. Publication and issue closure remain coordinator actions. Issue #21
remains open; subsequent family outcomes retain their actual statuses.

## Frozen source and scope

The original product baseline is
[`1e81e9b8`](https://github.com/Ephemeral-AI-Lab/layerfs/tree/1e81e9b8cf871324341c221a51b0a0239c580da9).
The specifications were published before implementation at
[`de27a847`](https://github.com/Ephemeral-AI-Lab/layerfs/tree/de27a847489ffec56429673d712503ae8ea7e8ec/docs/roadmap/0.1/0.1.3),
with the separately frozen capped replacements at
[`8913cf8d`](https://github.com/Ephemeral-AI-Lab/layerfs/blob/8913cf8d/docs/roadmap/0.1/0.1.3/capped-inherited-replacements.md)
and final shared execution supplement at
[`837da2f6`](https://github.com/Ephemeral-AI-Lab/layerfs/blob/837da2f6b6167b225958bb421572e23a38b94e50/docs/roadmap/0.1/0.1.3/execution-contract.md).
The [published issue update](https://github.com/Ephemeral-AI-Lab/layerfs/issues/22#issuecomment-5533915339)
records the initial freeze and unchanged Phase 1 boundary.

Both selected anchors use the same
[`4c207c70` implementation](https://github.com/Ephemeral-AI-Lab/layerfs/tree/4c207c70f3282c316d5ab18d832504085835eda3/benchmark/fs-bench-pro)
and [sealed build](../../../../benchmark-results/fs-bench-pro/phase1-v013/assets-4c207c70/evidence/build.json):

| Identity | Value |
| --- | --- |
| Host binary SHA-256 | `3bc327d5ac14be15b2c585b055f0f148fbad9ecc111592e219a5fb6c560e9adc` |
| Instrumented product seal | `810655a13d8621b2e04efeda5747e54929e4d4717e8d5d82dcddcf75f905b727` |
| Measured harness seal | `1fe49d740cc23421b8c8a1ae5e56cc7c2ace2ab0a5b03dc950ae47ae5f9489a9` |
| Runtime image | `sha256:781f4513dcba84f51bb5b7fda4704e7e5dfe52c8aabf777b310778afba41935f` |
| Runtime environment identity | `c4b2ab833a6ee08b4263044053eeb97124b44442ac009aca0d9c1e5635afb460` |

This is an explicitly instrumented baseline, not the released binary relabeled.
The changes add passive observations and disabled-by-default verifier fault
activation; they do not optimize measured product algorithms or storage.
The original SDK definition modules and singular timed SDK helper retain their
released meanings. The new families use the existing binary, workload helper,
Store, authenticated daemon/FUSE route, custody/cache implementation and thin
runners. The [reuse map](infrastructure-reuse.md) identifies those components.

## First acquisition and cache-hit selected command

These are the first two prescribed seeds of `payload-create-1m`, each run once.
Both begin from the same qualified **empty** input Store; the 1 MiB payload is
created inside the measured real-FUSE workload. Input reuse does not substitute
for measured creation. Their raw product outcomes and current evidence
validation pass. Matching independent family verification is still required
before they support an eligible performance claim.

| Metric, exact nanoseconds | Seed 1: first acquisition | Seed 2: cache hit |
| --- | ---: | ---: |
| Full CLI invocation wall | 5021133250 | 1148118917 |
| CLI source validation | 3822177042 | 315576750 |
| CLI registry query | 236450000 | 6472833 |
| Sample command wall, preparation through cleanup | 945600083 | 811800000 |
| Sample preparation, including runtime readiness | 502346791 | 430890000 |
| Cache acquisition | 39801916 | 6033208 |
| Cache generation/build | 20068416 | 0 |
| Cache validation | 15564500 | 2733667 |
| Independent Store clone | 7238125 | 6738541 |
| Runtime preparation | 371242833 | 341735458 |
| Supervised worker wall | 158863958 | 112033917 |
| Host orchestration envelope | 86102625 | 57185958 |
| Sum of pure named public-call intervals | 29780042 | 30083334 |
| Inner ordinary workload | 8167250 | 6507833 |
| Sample cleanup | 283533500 | 268051333 |

Cache and runtime rows are components of preparation, not additional elapsed
intervals to add again. Likewise pure calls are inside host orchestration, which
is inside worker/sample/CLI wall. The first CLI command took **5.021133250 s**,
slightly above the aspirational 1–5-second warm development goal. The cache-hit
CLI took **1.148118917 s**; its sample alone took **0.811800000 s**. Neither
number is reported as the duration of a whole family or an exhaustive verifier.

The [build command receipt](../../../../benchmark-results/fs-bench-pro/phase1-v013/assets-4c207c70/evidence/commands.json)
separately records 0.103128667 s for the reusable host-build command,
43.638556125 s for image build, and 0.449223333 s for image binary identity
capture. Those commands used existing compiler/Docker caches and are excluded
from the selected CLI walls; they are not a clean-from-source build benchmark.

Both samples cloned 917504 bytes using APFS copy-on-write, rejected hard links,
and verified the clone against pristine Store SHA-256
`fac603c4bf03c288b8c60e63624ade3f311480359fc78d75e960491e5005714f`.
They share cache key
`8c33ebab4ccbbd973f3145fbd001acc1f90aabf244775c244d40272e0ed0f737`.
No live Workspace, reader cache or post-operation Store was reused.

Evidence:

- [Seed 1 outcome](../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-1m-s1-performance-5a93ab533372/outcome.json), [raw receipts](../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-1m-s1-performance-5a93ab533372/raw.jsonl), [CLI invocation](../../../../benchmark-results/fs-bench-pro/phase1-v013/invocations/bff411fad5c4455cb782db533100be8a.json).
- [Seed 2 outcome](../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-1m-s2-performance-7352299acb92/outcome.json), [raw receipts](../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-1m-s2-performance-7352299acb92/raw.jsonl), [CLI invocation](../../../../benchmark-results/fs-bench-pro/phase1-v013/invocations/093fee02cfaf4f5593fdb0138f4011e8.json).
- Each attempt retains its source/environment, input/master manifest, independent
  clone receipt, container observations, compressed cgroup samples, stderr and
  complete evidence manifest. The two attempt-manifest hashes are respectively
  `f44bf07db49b39c728cdb71c276b78fad09e6418fa8c4d9fe4b50e050afc9304`
  and `8d8663cdf8f9368bacdb6dc9ae3b3484cda4d0c2cf5fdca190c9bffa6782a143`.

## Qualification and acceptance checklist

| #22 acceptance criterion | Evidence and precise disposition |
| --- | --- |
| Committed specifications and baseline available to family owners | **Met.** Exact source links above; all twelve family specifications and common rules precede implementation. |
| Existing SDK semantics retained; one selected case without all-tier preparation | **Met.** Released definitions/helper preserved; selected anchors perform one Create, public Exec, Commit, query and End, with one ordinary 1 MiB creation and no performance verifier, reopen or injection. Only the compatible empty input Store was acquired. |
| Cache hit/miss, invalidation, interruption/corruption, concurrent publication and sample isolation | **Met.** [Cache check output](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/cache-check.stdout.txt) records the existing shared-publisher regression set plus cross-family reuse, directory clone isolation and directory metadata corruption checks. Actual miss/hit anchors bind the same master to independent clones. |
| First-use and warm-prepared walls separately measured; budgets explicit | **Met.** Exact table and immutable invocation/sample receipts above; build/setup costs remain separate. The execution contract fixes resource and phase ceilings; the aspirational first-CLI miss remains visible. |
| Registry, counts, byte envelopes, routes, timing purity and cleanup checks | **Met for shared infrastructure.** [Static registry](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/static-registry.jsonl), [static additions](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/static-additions.jsonl), selected runtime receipts and fail-closed report validation. The additions receipt binds [its binary/source](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/static-additions-source.json); its binary SHA equals the final selected binary. The family definition and common fixture/verifier files are unchanged between that source and `4c207c70`. |
| No optional optimization or cached measured state | **Met under the completion amendment.** Original anchors are the frozen instrumented baseline. Required functional repairs now have a separately labeled corrected source; optional performance/storage optimization remains Phase 2. Measured creation still writes its bytes and output states never enter prepared caches. |

Additional retained qualifications are scoped narrowly:

- [Native/canonical verifier qualification](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/verifier-route-1.jsonl)
  checks a 4099-byte file, hard-link alias, symlink and empty directory. It
  rejects extra paths, changed bytes, broken aliasing, changed symlink target,
  nanosecond timestamp mismatch and mode mismatch.
- [Digest-custody qualification](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/digest-route-1.jsonl)
  checks verification-only descriptors, strict manifest parsing and native/
  canonical digest mismatches. It does not turn candidate bytes into independent
  semantic expectations. Early verifier runs remain diagnostic receipts with
  their original context, not relabeled final-source family proofs.
- [Physical spool regression](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/physical-spool/result.json)
  records one passing source-stable test of actual native allocation across
  failed short append, reclaim and discard. The corresponding before/source
  receipt and manifest are retained. Actual selected Commit diagnostics also
  report `Some` physical allocation/peak values with zero observation errors.
- [Workspace and Store fault-controller qualification](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/fault-controllers/context.json)
  records one passing test per controller and unchanged controller sources.
  These qualify disabled feature-gated controllers, not execution of the 28
  reliability subcases or any live fault benchmark.
- [Capped input identities](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/capped-inputs/identity-receipt.json)
  bind the two bounded prefixes, the existing full-size checkpoint and the five
  versioned plans. Their 25 samples/five verifiers remain a separate campaign.

The selected observations are causal samples, not exact continuous per-phase
memory/category peaks. Seed 1 retained 12 cgroup observations over 120840875 ns
with an 11144125 ns maximum observed gap, bracketing its 86102625 ns host
orchestration scope. Host observer drain and supervisor polling are separate.
Both samples have zero swap/OOM, matching resource caps, real FUSE counters,
normal daemon exit 0 at worker process termination after Client cleanup, and
zero owned files at final cleanup. The process-scoped daemon owner survives
individual Client drops and supports subsequent reopened Clients.
Physical spool peaks are mutation-boundary native allocations, not inferred
from logical spool length or sampled boundary maxima.

## Preserved failures and publication boundary

The failed [`71247071` Linux image build](../../../../benchmark-results/fs-bench-pro/phase1-v013/assets-71247071/evidence/image-build.stderr.txt)
remains a failed harness-build attempt. Its retained command returned exit 1;
the Linux xattr ABI declaration was corrected before selected collection.
The early [native Git oracle failure](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/ordinary/native-git-check-1.tool-transcript.txt)
also remains failed. Its [qualification history](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/ordinary/qualification-transcripts.json)
records the XOR descriptor defect, focused corrections/rechecks and missing
historical pre-run source seals. Those diagnostic passes are not promoted to
exact-source admission evidence.

The [initial selected review](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/selected-review-initial.json)
retains erroneous derived failures for Docker's `CAP_SYS_ADMIN` spelling,
normal daemon shutdown and an incorrectly extended cgroup window. Correcting
that report consumed the same raw evidence and did not rerun or alter the
product samples. Subsequent report versions record their own generator hash.

The task-scoped publication list selects the two complete small attempt
packages, their invocation receipts, sealed build metadata, qualification
text/source receipts and original failure logs. It excludes live campaign
streams, mutable progress/results, workload payloads, SQLite Stores, executables
and unrelated user files. Full local manifests may reference retained binaries
or prepared Stores not copied into Git; the publication is an evidence index
with authenticated identities, not a self-contained Store/cache archive.

## Audit closure and corrected-source qualification

The [functional-repair amendment](failure-repair-amendment.md) changes terminal
completion policy. Shared harness audit corrections and the first required
product repairs are committed at
[`fbf32e84`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/fbf32e84662d00993c033515e113437965395494).
The [sealed corrected build](../../../../benchmark-results/fs-bench-pro/phase1-v013/assets-fbf32e84/evidence/build.json)
uses host binary `00767269960707775f7ff9b3549568c234ab2b89359b682707a9874b1f8259e5`,
product seal `e24867af45d83c455dbfac530d43140fec7cdc40d3eae9ff70a30883d239125a`,
and image `sha256:2a9a6dc9d5f09a9785d611916f96100fe82f515f45a453bb35c83204fafb8d3e`.
Its build retained unchanged dependencies, selectively invalidated three changed
Cargo packages, built the host in 14.323245625 seconds and the image in
28.041642208 seconds. The earlier failed registry TLS attempt is preserved.

The corrected
[payload-create-1m seed 1 package](../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/payload-create-1m-s1-performance-f285d157909c/outcome.json)
records a cache-hit sample wall of 864327375 ns, including actual preparation,
product work and cleanup. It belongs to the complete 24-slot payload performance
invocation of 49978744542 ns; the per-sample wall is not mislabeled as a standalone
CLI invocation. All 24 corrected payload outcomes passed execution and evidence
validation. Matching late verification remains required for those performance
claims.

The corrected selected tiny-create-1, bulk-create-100 and bulk-delete-500 seed-1
performance outcomes also passed. One justified focused
[bulk-delete-500 independent proof](../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-500-s1-verify-4ed93a7acfd4/outcome.json)
passed canonical state, full expected final-tree bytes/metadata, reopened FUSE
state and cleanup. Its full runtime observation window and lossless compressed
canonical evidence passed validation. The proof is reused in the later complete
campaign. Report-only verifier-mode correction `8b9557db` revalidated the same
raw proof and retained the original erroneous review; no product rerun occurred.

Runner interruption/orphan recovery, failed-outcome reached-phase validation,
mode-specific purity, physical allocation observations, source-cache
compatibility and compressed artifact readback now have focused qualification.
The report explicitly refuses terminal pass for unresolved required product
failures. This satisfies issue #22's shared implementation/qualification
criteria. It does **not** complete the remaining family campaigns or #35.
All original baseline failures retain their source and FAIL status, and #21
remains open for later phases and release qualification.

## Atomic sampler integration and scoped evidence retention

The observer/verifier update at
[`b8c2ad4b`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/b8c2ad4bf4fa0415fd49d57abea15729b33a4284)
qualifies complete-row cgroup sampling and retained history accounting. Its
[sealed build](../../../../benchmark-results/fs-bench-pro/phase1-v013/assets-b8c2ad4b/evidence/build.json)
has host SHA-256 `2cc488da2e6b0d677038a150117956b7893050fccc79aab7c25d07a8c6145fb3`
and image `sha256:d7cfd5b1b29a61e724d05f2e80f368b8aa5ba08133b0c516bd5c40b6cfdd8d3b`.
The corrected product seal remains
`e24867af45d83c455dbfac530d43140fec7cdc40d3eae9ff70a30883d239125a`;
the daemon and FUSE executable hashes are identical to the `fbf32e84` build.
Host and workload-helper identities remain distinct because their harness code changed.

The old `tiny-stat-1`, seed 1, performance
[attempt `2babc4ee0210`](../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-1-s1-performance-2babc4ee0210/outcome.json)
completed the product operation but ended its cgroup stream with a partial row.
That mandatory observation is invalid. Its raw product PASS, original partial
artifact and explicit [invalidation record](../../../../benchmark-results/fs-bench-pro/phase1-v013/invalidations.jsonl)
remain preserved. The sampler now assembles every field and the newline before
one unbuffered blocking-pipe write, checks the 4096-byte atomic bound, and keeps
the same field order and 10 ms cadence. Read, formatting, size and write errors
propagate; a truncated row is never silently removed to manufacture a pass.

Only that invalid slot was recollected. The b8
[replacement attempt `f95ef696b6f6`](../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-stat-1-s1-performance-f95ef696b6f6/outcome.json)
and [live validation](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/atomic-sampler-live-validation.json)
passed with `issues=[]`, `violations=[]`, 10 cgroup observations, a 12553250 ns
maximum observed gap, zero swap/OOM, and successful owned cleanup. The required
dispatch observation window was 101583541 ns. Sample command wall was
2220685750 ns; inner workload was 2393542 ns. The report's causal sampling rule
passed; this is not a continuous per-category peak measurement or a full-family proof.

The [scoped build-selection ledger](../../../../benchmark-results/fs-bench-pro/phase1-v013/evidence-builds.json)
retains already-completed fbf performance for payload, tiny-file churn and
directory traversal, with the one b8 slot override above. It also retains the
passing `payload-create-1m:1:verify` and `tiny-bulk-delete-500:1:verify` slots
under their original source/image identities. The
[qualified compatibility bridge](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/report-slot-sampler-bridge/result.json)
requires unchanged product, normative contracts, timed family operations,
generators and independent expected-state definitions. Only the separately
hashed `sample_resources` body is excluded from the registry comparison;
its signature and all surrounding bytes must match. This is an explicit,
source-bound retention decision, not blanket equivalence between builds.

The [sampler integration note](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/atomic-resource-sampler/integration.md)
records exact selected identities, binary comparisons, observation values,
platform contract, model results and the ledger snapshot hash. Publication is
limited to selected immutable qualification packages, identities, validation,
bridge and original failure receipts. Live campaign streams, mutable results,
Stores/caches, binaries, workload payloads and unrelated user content stay out
of this shared closeout package. Issue #22 can close after that evidence is
published; remaining child-family verification, #35 and `PHASE1_TERMINAL_PASS`
are separate obligations, and #21 stays open.
