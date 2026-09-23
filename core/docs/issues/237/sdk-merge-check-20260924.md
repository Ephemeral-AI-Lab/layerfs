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
