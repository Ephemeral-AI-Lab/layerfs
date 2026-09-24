# #231: lightweight four-tier SDK Init verification, 2026-09-24

> **Status: all four functional lite-verifier rows PASS.** Owner direction
> replaced per-file full-content benchmark readback with complete namespace
> inventory plus a deterministic file sample. This result does **not** claim
> all file payloads were read. Performance admission and #231 closure remain
> open under the uncontrolled source-cache/numeric-gate contract.

## Identity and scope

The [prospective lite contract](SDK-VERIFIER-LITE-20260924.md) was measured
on clean source `92608113f23cd89a7b8dc94b54f65693456d60b9` (tree
`1608e3e8e7fea28395d621c88f2a93b4ea00032f`). Product seal:
`b7f6c49d721d910fff835fe65e5bf42348aa1212f4c3c584de2f8e44167a47b7`;
harness seal: `ef03a2891b339e3e6dc03bc083624f0eb20f320fe570e5af7dbc312d428db7b8`;
Cargo lock SHA-256: `72919f072f26a55ed8e2d3284536fe11dd0e5149fd2f9058ed96a36df2ad1a06`.
The locked release driver SHA-256 was
`e6b51b46610efc507bf77cab3f33ff216a62729a81c59cf8594ed5b3509fa63f`;
the new verifier SHA-256 was
`b23e016215a74ed2f431cbec84833b5ec044d5b55084e39114987faeb4190b19`.
The changed build took 1.110119333 s under its 30 s bound; later cases
reused exact binary hashes. There was one fresh Store/History and one public
SDK Init call per case, with no repeated arm.

The verifier authenticated the C5 genesis root and sealed manifest,
enumerated **every** actual namespace path through public C1 reads,
rejected duplicate/missing/unexpected paths and wrong inode kinds, and
checked every directory's portable metadata. Its deterministic sample
selected at most 195 file paths across the tree and size bands; for each
selected file it used public C1 `read_all` and checked portable metadata,
full logical byte count and SHA-256. A separate script recomputed the
sample from the sealed manifest and matched all four observed sampled-file
and sampled-byte totals, including both 100-MB anchors in the 100k case.
The `manifest_bytes` field is the **expected** total; `sampled_bytes` is
the amount actually read back.

## One-shot results

| Case | Public SDK Init | Complete command | Lite verifier | Complete paths / sampled files / sampled bytes | Cleanup | Receipt |
| --- | ---: | ---: | ---: | --- | --- | --- |
| 100 files / 5 MB | 0.030653500 s | 0.044402125 s, PASS | **0.029412625 s, PASS** | 102 / 53 / 3,354,003 B | PASS | Functional PASS; performance INELIGIBLE |
| 1,000 files / 20 MB | 0.105125042 s | 0.120415709 s, PASS | **0.051693583 s, PASS** | 1,011 / 70 / 6,430,827 B | PASS | Functional PASS; performance INELIGIBLE |
| 10,000 files / 300 MB | 1.226295125 s | 1.239515667 s, PASS | **0.626640167 s, PASS** | 10,101 / 72 / 101,928,859 B | PASS | Functional PASS; performance INELIGIBLE |
| 100,000 files / 500 MB | 5.588053292 s | 5.607812208 s, PASS | **2.849658958 s, PASS** | 101,001 / 73 / 200,286,236 B | PASS | Functional PASS; performance INELIGIBLE |

Every verifier finished below the frozen 9.5 s bound and emitted the
expected root, manifest SHA-256, path/file/directory counts, sample policy
and positive sampled coverage. The 100k verifier had timed out at 9.5 s
under the preceding **full-content** treatment. The new 2.850 s result
does less content work by design; it is not a speedup claim for an
identical full-byte oracle. The four optional structured-text variants
were not selected.

Each raw result directory is under
`benchmark-results/fs-bench-pro/issue231-lite-verifier-92608113f-{100,1000,10000,100000}/`
in this worktree. Read-only `runner.py verify --run` returned PASS for
all four retained-evidence manifests. The small
[curated receipts](evidence/sdk-lite-four-tier-20260924/) and
[SHA-256 inventory](evidence/sdk-lite-four-tier-20260924/SHA256SUMS.json)
are committed here. Historical five-second, full-content and failed
overlap receipts remain unchanged.

## Checks and remaining gate

The verifier example's release unit test for sample coverage, nine focused
benchmark Python tests, warning-denying Clippy for the verifier example,
Core formatting, and `git diff --check` passed. The runner's locked
release build passed. No product implementation under `core/crates/*/src`
changed in this verifier treatment; a full Core workspace test was not
repeated solely for the benchmark example.

All rows still declare `source-cache-uncontrolled-v1` and
`admission_eligible=false`, with no frozen numeric SDK latency target.
The owner-approved lighter verifier resolves the **verification-time**
blocker, but cannot turn these raw public-call observations into #231's
four eligible performance PASS rows. Do not merge the treatment PR to
remote `main` or close #231 solely on this cohort.
