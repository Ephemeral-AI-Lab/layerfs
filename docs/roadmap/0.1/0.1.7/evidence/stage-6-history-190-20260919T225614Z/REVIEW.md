# Independent adversarial review of #190

> Status: Research; informative and not a product contract.

Read-only review of source and archived artifacts; no benchmark, build, verification run, product edit or harness fix was performed by this reviewer. Only this document and [review-derived.json](review-derived.json) are reviewer outputs. This review independently parsed original JSON/JSONL/gzip and codec text logs using Python's standard library, without calling the squad analyzers or trusting their derived JSON.

## Verdict

The synthesis's **archival arithmetic reproduces**. Its restrained disposition is supported: retain the tripwire and leave causal attribution open. The 23,520,347,667 ns build/update envelope is the largest recorded region, but these artifacts do not establish a causal tree-walk cost, exclusive codec cost, or a matched regression against v0.1.6. Fresh detailed stride10/stride3 work remains **NOT_RUN**, so the requested full root-cause campaign is incomplete. This review provides no gate or release approval.

## Reproduction from raw artifacts

All three S1 archived SHA-256 values match the document. All five S4 gzip files decompress to the declared original byte lengths and SHA-256 hashes. All seven copied S3 files match their custody lengths/hashes. These checks prove preserved bytes at archival capture; they do not establish the historical generation identity of an unsealed core or scratch-codec executable.

For core, parsed every timing child and required exactly one trace resource record per state for input_ns, build_ns and save_ns. Subtracted content, store creation, input, build and save from each state; all 17 residuals were nonnegative. The independently derived rows in review-derived.json match every S1 table cell. The raw phases operation_ns equals the state-child sum.

| Core exclusive accounting field | Independently reproduced ns |
| --- | ---: |
| content | 1,115,152,085 |
| store.create | 2,469,708 |
| input | 65,188,166 |
| build/update envelope | 23,520,347,667 |
| save envelope | 7,769,904,041 |
| unassigned residual | 93,006,002 |
| **Operation** | **32,566,067,669** |

The sum of these six rows minus operation is **0 ns**. That is an accounting closure retaining an unassigned row, not zero causal uncertainty. The save envelope already includes storage.begin and storage.finish; the build envelope also includes harness setup surrounding the public API. It is inappropriate to label the entire build envelope as validation, a base-tree walk, or pure C1 CPU.

Raw root = **44,831,509,917 ns**; root minus operation = **12,265,442,248 ns**. The handoff root is 1,000,000 ns too high. Core median state operation is **1,564,124,750 ns**. No new runtime was collected to derive these values.

For v0.1.6, independently selected storage-smoke-phase receipts by phase under each of the 17 records, then summed elapsed_ns. The full157_index vector matches [1,11,21,31,41,51,61,71,81,91,101,111,121,131,141,151,157].

| Legacy field | Independently reproduced ns |
| --- | ---: |
| Commit sum | 11,370,679,212 |
| Commit median | 564,295,292 |
| Exec sum | 59,717,921,250 |
| Per-state transfer sum | 18,588,473,754 |
| Work-wall residual beyond those three | 6,766,980,825 |
| Outer lifecycle residual beyond setup/work/cleanup | 44,874,000 |

Thus the descriptive difference is exactly **21,195,388,457 ns** and the ratio is **2.8640389075994275**. Neither quantity is a matched causal effect. Source at historical revision 7fab1027a brackets only the commit API in storage_smoke.rs:737, and starts/stops its clock around that call at :580–582. Historical changes.rs:680–722 includes canonical construction and streamed admission; :851–877 includes namespace work. Therefore the wholesale-content/tree-exclusion explanation is contradicted by actual boundaries. However, current content versus historical content is not a matched phase comparison.

The historical source identity distinction is real: compiled source ac729dfeb4ee923b0a42106a53d1c7de4cd4cfcb versus orchestrator 7fab1027a0061e8b932345d4fcd6ac22a089b155. Independently running git diff between them for crates and benchmark/fs-bench-pro produces no changes. Current core archive contains no matching binary/source seal; present product source equality cannot fill that custody gap.

S3's two codec logs independently reproduce the following **separate archival** comparisons:

| Log | Level 19 minus level 1 CPU ns | Stored-byte difference |
| --- | ---: | ---: |
| armA_sweep.txt | +1,011,835,000 | −262,222 B |
| rest2.txt | +1,425,492,000 | −262,222 B |

The retained scratch source main.rs:375–415 brackets process CPU around in-memory recompression, including per-level context/buffer work. It is not current live-save CPU, contains no RSS observation, and has no sealed historical source/input/executable linkage. Both logs must remain visible; neither may be chosen as the preferred current-lane cost. Their arithmetic supports S3's limited historical claim and cannot close any part of the current causal excess.

## Findings and unsupported claims

1. **Causal attribution remains open.** Source corroborates conditional traversal and sibling-page acquisition, not an unconditional all-base-tree walk per state. A dominant build envelope does not price the individual algorithms inside it. Source-based complexity and base-size correlation are evidence for a follow-up, not proof of cause.
2. **Exact denominator disagreement remains in squad documents.** S2's concluding 21.195067669 s and S3's H3 paragraph 21,195,067,669 ns subtract the handoff's rounded 11.371 s. They differ from the retained raw comparison by **320,788 ns**. The synthesis correctly chooses S4's exact **21,195,388,457 ns**; this review leaves the squad disagreement visible and does not edit their documents.
3. **No matched worker or cache explanation exists.** Historical source changes.rs:683–688 forces one producer for predecessor plans. It does not record which states took that route. Its raw cache_profile explicitly says fresh-store-existing-os-cache-uncontrolled. Core archive has no cold/residency proof. Environment worker changes alone do not parallelize the current serial driver. Neither effect can be assigned a number from this evidence.
4. **Legacy subphase counters are not an exclusive partition.** Source calls CandidateFinish within changes.rs:887 and around the full build in remote_commit.rs:198–201. Adding historical phase/counter rows would double count even where an archived unattributed field says zero. Synthesis correctly avoids this.
5. **Fresh detailed evidence is absent.** No actual recorded filesystem.validate/directories/references/inodes split, new SaveOutcome commit count, or charged-byte/codec-CPU measurement exists for this campaign. Instrumentation plus build success is not an observed diagnostic. A 15/25-second unresolved command ceiling remains a NOT_RUN reason, not permission to infer results or silently apply the proposed 120/240 seconds.
6. **Validation failures stay failures.** Retained check receipts show release build/test exit 0, Clippy exit 101 and fmt exit 1. I inspected receipts, not reran checks. A source-line match can show existing statements but does not turn failing current checks green. No CI/preflight approval follows.

## Instrumentation review

The new harness uses existing public timed filesystem APIs. In core/crates/layerfs-content/src/filesystem/update.rs:67–113, timed and untimed variants validate the same base condition and call the same run implementation, differing in supplied phase handles. Six phases exist: validate, directories, references, inodes, cleanup and root.encode. Timing::disabled at core/crates/layerfs-telemetry/src/timer/scope.rs:194 invokes the closure without clocks/nodes. No per-file active nodes are introduced, and SaveOperation.accept creates no node (core/crates/layerfs-storage/src/cas/store.rs:372–383).

The successful detailed-tree maximum is **16 nodes per state + one root + one first-state Store create = 850 for 53 states**, below MAX_NODES=1,024. The 16 are state, content, predecessors, input, filesystem, six filesystem subphases, storage.begin, accept loop, index and storage.finish. The 53-state guard therefore has a valid source-derived bound. It is not a runtime completeness proof until a detailed artifact is produced.

Coverage is intentionally incomplete: lazy reference consumption appears inside inodes; release/setup and some cleanup remain residual; Store read/decode CPU remains nested. The added provider counters cover the filesystem provider, not every separate content-cursor provider. This cannot be described as isolated total read-back, ordering, codec or index attribution.

Review of the initial instrumentation found a harness behavior drift: moving prior_roots and prior_missing into the predecessors closure released their allocations earlier than before, even with detailed recording disabled. The reviewer reported this rather than fixing it. Root assigned S1 a repair retaining the maps to the original state scope. That revision and its validation are addressed in the appended review below. Additional outcome fields and counter emission also mean that “default unchanged” must mean product route/default values, not byte-identical trace output or zero instrumentation overhead.

## Reproduction method and limits

The independent derivation read timing.json with json.loads, parsed each trace.jsonl line and indexed the resource records by exact state/key, then computed residual = state.elapsed_ns − content − store.create − input_ns − build_ns − save_ns. For legacy it used gzip.decompress/json.loads and summed record.receipts entries whose kind is storage-smoke-phase and phase is commit or exec, with record.transfer_ns summed separately. statistics.median computed the 17-element medians. S3 text sections named groups ordinary and groups pooled were parsed as integer CSV and summed by level before taking 19 minus 1. hashlib.sha256 checked copies against custody. No squad script was executed. The first reviewer parse incorrectly filtered core fields as kind=counter, failed with KeyError, and was corrected to their actual kind=resource; no product work ran in either attempt.

No raw sample.sqlite was copied into S1 archive. Store totals reported in the handoff were not independently remeasured here. No source-based inference repairs unknown historic cache/worker/machine state. Acceptance should remain partial: archival reconstruction is reproduced; fresh diagnosis and causal allocation remain outstanding.

## Review append — instrumentation revision 2

S1 has now retained both [initial diff](squad-s1/harness-instrumentation.diff) and [revision 2 diff](squad-s1/harness-instrumentation-v2.diff). Inspection of current history.rs:1696 and :1859 confirms the closure returns `(bases, prior_roots, prior_missing)` and the caller binds both maps in the state scope. This removes the early-release change reported above; the reviewer did not edit the harness. The extra counters/trace output remain an intentional harness change, so no claim of identical measurement overhead is supported.

The source-derived 850-node bound remains valid after this correction. No detailed runtime artifact exists to test completeness, phase sums or source-output equality. The build/test receipts described above predate this small revision; any subsequent check must be identified separately by the coordinator, not silently attributed to the earlier logs. This does not alter the archival arithmetic or causal limitations.

## Review append — separately retained revision 2 validation

Read-only inspection now confirms release-build-v2.json exit 0 (**8,151,938,750 ns command wall**), harness-tests-v2.json exit 0 with **116 passed tests** in its log, and harness-clippy-v2.json exit 101. All **22** listed Clippy source statements independently match their recorded HEAD lines; the current Clippy result remains **FAIL**. Earlier v1 checks and the initial review finding remain preserved.

Independently recomputed current harness-source, release-binary and both Cargo.lock SHA-256 hashes match built-artifact-v2.json; `git diff HEAD -- core/crates` is empty. These records qualify the separate v2 build/test validation, not historical binary identity or a new measurement. No product performance invocation or additional reviewer build/test was performed. Fresh detailed attribution and causal diagnosis remain outstanding.
