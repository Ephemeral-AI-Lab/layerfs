# SDK Init after the #237 / #236 merge

One SDK call per case on the clean merged source `ac946c7d3be45d0c468bff5d508a1bc4fa3f26b1`
(`product_seal=1d9d64ee874c4dc3b305c3be5021471b80abf757e0f9309bdba12457f57c5f84`).
The main checkout had unrelated tracked deletions under `web/` and
`artifacts/ocr/`, so these calls ran in a temporary clean checkout of that
commit. Those deletions were not changed. The check used the merged
`layerfs-sdk::Client::init_project` path and the refactored Service with
`ImportBatch`.

| Case | SDK call | Complete command | Separate reopened readback | Functional result | Status |
| --- | ---: | ---: | ---: | --- | --- |
| 100 files / 5 MB | **159.816 ms** | 1,555.635 ms | 680.261 ms, 102 paths / 5 MB | **PASS** | Registered SDK v2; performance `INELIGIBLE` |
| 1,000 files / 20 MB | **432.069 ms** | 445.340 ms | 664.830 ms, 1,011 paths / 20 MB | **PASS** | Registered SDK v2; performance `INELIGIBLE` |
| 10,000 files / 300 MB | **6,139.828 ms** | 6,154.255 ms | 8,287.477 ms, 10,101 paths / 300 MB | **PASS** | Unregistered SDK functionality diagnostic |

The registered 100 and 1,000 rows each made one public SDK call, returned a
typed Project/genesis/root, exited cleanly, and passed the benchmark's full
independent verifier and retained-receipt custody check. Both complete commands
were below 15 s and both verifiers below 5 s. The 10,000 call made one SDK
request and its separate verifier checked all 10,000 file bytes, SHA-256 values,
portable metadata and the published root. Its 8.287-s verifier is **outside**
the registered two-case, 5-s verifier gate; the 10,000 case remains `NOT_RUN`
in the frozen #236 benchmark selection. No 100,000-file call was made.

The registered runner uses the **debug** profile. The retained resource and
closed-Store figures for these three debug calls are:

| Case | Driver lifecycle CPU, user + system | SDK driver peak RSS | Store + History apparent bytes |
| --- | ---: | --- | ---: |
| 100 | 0.286 + 0.039 = **0.325 CPU s** | Not captured | **7,270,400 B** |
| 1,000 | 1.030 + 0.094 = **1.124 CPU s** | Not captured | **23,719,936 B** |
| 10,000 diagnostic | 14.338 + 0.957 = **15.295 CPU s** | Not captured | **334,004,224 B** |

CPU is `getrusage` for the complete SDK driver process lifecycle, including
worker threads; it is not CPU isolated to the public operation. The SDK route
did not capture process or phase peak RSS, so no memory number can be recovered
from these receipts. Storage is closed `store.sqlite` plus `history.sqlite`
apparent size; each History file is 86,016 B. The 10k pair occupied
336,166,912 B allocated on disk, including History (`st_blocks * 512`).

Each case used a fresh independent byte copy of its prepared source. The
whole-input preflight and the last nonfaulting check before SDK-driver launch
found **zero resident payload pages**: 0/390, 0/2,045 and 0/27,503. The SDK
timer starts after `Host::create`, which does not read the source payload.
Inode/directory metadata residency was not measured. The registered receipts
therefore keep their original `source-cache-uncontrolled-v1` contract and
`admission_eligible=false`; these raw times are not cold-namespace performance
PASS values or a speed comparison to the old daemon-host release build.

The SDK driver and verifier were built once from the merged source with locked
Cargo inputs in the main checkout before measurements, then copied as exact
hash-checked binaries into the temporary checkout. The runner recorded
`exact-binary-reuse`. Driver SHA-256:
`6ec006d3e59fb8fb36f7e7cba1fdce6405e7ab8b7d6b3d945d98f53f266591ea`;
verifier SHA-256:
`b7a9555856537e614037cc18d7aa3adcb452066e1c4b58ca554889e6f49dfa9b`.
No build or verifier wall was included in an SDK operation timer.

The [100-file receipts](evidence/sdk-merge-check-20260924/100/receipt.json),
[1,000-file receipts](evidence/sdk-merge-check-20260924/1000/receipt.json) and
[10,000-file diagnostic receipt](evidence/sdk-merge-check-20260924/10000-diagnostic/receipt.json)
retain the exact identities, results and costs. Their neighboring
`cold-preflight.json`, `cold-recheck.json`, `cold-launch.json`, `perf.jsonl`
and `verification.json` preserve the cache and readback evidence. Complete raw
outputs, including Stores, source copies and fixture manifests, were moved to
`benchmark-results/fs-bench-pro/sdk-merge-check-20260924/` in the main checkout;
the temporary checkout was removed after collection.

## Follow-up: release profile explains the apparent 10k slowdown

The 6.140-s diagnostic above used Cargo's **debug** profile. The earlier
1.110–1.419-s daemon-host observations used **release** binaries. A separate
one-shot SDK diagnostic built the same merged product source with
`cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --release`
for the SDK driver and verifier. It used the same SDK fixture manifest
(`878f44e10303cc43f5f202a4cf59e817316b16cee153fb70f8bf389ade6253ab`)
and a new independent source byte copy. Its final prelaunch check also found
**0/27,503 resident source payload pages**.

| 10,000 files / 300 MB | Debug SDK diagnostic | Release SDK diagnostic |
| --- | ---: | ---: |
| Public `Client::init_project` | 6,139.828 ms | **1,289.037 ms** |
| Driver lifecycle user + system CPU | 14.338 + 0.957 s | 0.994 + 0.986 s |
| Independent reopened readback | 8,287.477 ms, PASS | 2,166.301 ms, PASS |
| Source payload residency before launch | 0/27,503 pages | 0/27,503 pages |

The release SDK call is **4.76× faster** than the earlier debug SDK call and
falls inside the old 1.110–1.419-s release daemon-host observation range.
This establishes that the 6.140-s number was dominated by its debug build
profile; it does not establish an exact route-to-route speed ratio. The SDK
generates fresh authority identity on each call, the release and debug Stores
therefore have different roots, and directory/inode metadata cache state was
not qualified. Both independent verifiers passed all 10,101 paths and
300,000,000 bytes. The 10k case is still **unregistered** in the #236 SDK
benchmark and neither diagnostic is a performance admission row.

The [release diagnostic receipt](evidence/sdk-release-10k-20260924/receipt.json),
[cold recheck](evidence/sdk-release-10k-20260924/cold-recheck.json),
[binary provenance](evidence/sdk-release-10k-20260924/sdk-build-source.json),
and [full readback result](evidence/sdk-release-10k-20260924/verification.json)
retain the measured identities and limits. Full raw output is under
`benchmark-results/fs-bench-pro/sdk-release-compare-20260924/`; the temporary
checkout was removed after collection.

Owner direction on 2026-09-24 is **debug-only** for subsequent active Core SDK
Init measurements. The release row above is retained as a historical diagnostic
of the build-profile mismatch; it is not an active benchmark selection or a
replacement for any debug receipt.
