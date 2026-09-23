# #237 C1 fixed-identity 10k proof

> **Status:** Research; informative and not a product contract.

## H1 preregistration — frozen before either sample

The prior [C1 direct pair](c1-direct-prototype.md) kept the public route, fixture and cold-source method matched but let `runner.py` choose a fresh stack and scope seed for each arm. This H1 pair uses the **exact** root research `cold_diagnostic.py --fixed-operation-identity` driver from `3c2c8d7932286cc44a9cff2cc7de00d9525bf31c`, copied byte for byte into this isolated worktree (SHA-256 `124e9323d2580cb2a7cbc14ba54923f7e240ea7307e928a02b55f6279ccebbbe`). It fixes only the public `stack` and `scope_seed`; transport keys, session credentials, namespaces, source cache, Stores and output paths stay independent. The driver writes `operation-identity.json` before the public call. Its `sha256-fixture-case-v1` derivation for `namespace-10000` and manifest SHA-256 `c1d7937c9f90d3585558e6d20b35183a737530d386667d08badc3121a6b3d55e` yields stack `1b88dfdc2ffd3c6c0b0ac0ec4c064284` and scope seed `3afa1d39bd5ede7b5a0a40d71e50160dde8e897fa9865929300cb01b95bbdd18` in both arms. This is a new harness identity and a new proof question; it does not replace either earlier timing sample.

Control is the base `a6d1d563f98b40755c43684a0449c74d39d894ff` product source (`update.rs` restored exactly from that commit), with product seal `1f24a0fd8a1ab9207fec22ae837da6e790ae3e931938db41108b5f513ac5a5d9`. Candidate is the direct fresh-build source in `dc654bb061380c49b5634d444f7043fbccba969b` with product seal `086df9cf44aec9d990635ed333a3bbb0971235d77560caaeb9a55832eba92a59`. The Core harness seal in the previous pair was `6d9a3e2eb2a1eea1f3f0d63f0df15949d6f3bab103657a824b0a04d34482eff8`; require it to remain identical in H1 receipts. Reuse only binary archives whose SHA-256 exactly matches those product seals, or record the private locked release rebuild before a sample. The same source script and `--fixed-operation-identity --verify` flags apply to both arms. The verifier is a separate child after the caller timer and complete performance command. Keep its existing 5 s watchdog; a timeout stays a failure.

One sample per arm, in this order, at fresh output paths:

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py --case namespace-10000 --fixed-operation-identity --verify --out benchmark-results/fs-bench-pro/issue237-c1-h1-control
python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py --case namespace-10000 --fixed-operation-identity --verify --out benchmark-results/fs-bench-pro/issue237-c1-h1-candidate
```

The source is the same sealed `core-native-import-fixture-v2` seed-1 10k workspace: 10,000 regular files, 100 data directories, 300,000,000 bytes including its 100 MB anchor, unchanged names, modes and mtime. The prepared workspace is reused **only** outside the measured call. Each arm hashes and invalidates every source payload and requires a separate whole-input zero-residency `mincore` preflight and immediate nonfaulting recheck before its own timer. Record resident pages, page count, method, exact recheck-to-timer gap and any failure. This proves source-payload residency only; metadata/dentry cache remains unqualified and the Core runner's `source-cache-uncontrolled-v1` and `admission_eligible=false` labels stay intact. Do not warm a timed source read. Four construction workers, the eight-object channel, 4 KiB SQLite database pages, request/command limits, Store policy and C1 resources stay as declared.

The decision is primarily an **identity/readback proof**. Require both public calls to return a confirmed C5 root, equal exact root IDs, and equal complete `objects` identity sets. Require the independent reopened `verify_namespace` child to check every expected path's kind/mode/mtime and stream all 300,000,000 bytes into SHA-256 within its existing watchdog, including History's published root. If either verifier times out, retain its failure; any later independent Store-only full readback is labelled diagnosis and does not promote that gate. For each closed Store, record `PRAGMA page_size` (must be 4096), apparent and allocated bytes, page/freelist count, object count/IDs, `sum(length(data))`, pack used/capacity/spare bytes and per-save pack/object counts. Equality of the tree Save's reachable object set and pack geometry is necessary to attribute unchanged C1 representation. Any whole-Store space increase remains a reported compactness failure even if it arose in the preceding file Save; no retrospective tolerance is granted. The #229 sparse-history compactness/readback lane remains a separate required gate. The fixed-identity H1 caller times and CPU/RSS are retained as raw observations, never used to replace the previous pair or claim a fully cold 700 MB/s result.

This worktree alone owns its Cargo target, fixture, scratch, Stores and output directories. Coordinate the timed window with the parent before each arm. Preserve all failed/partial receipts. Do not rerun an unchanged arm to select a better number.

## Results

One H1 control and one H1 candidate were run, with no retry. Both sidecars
recorded the exact frozen `stack_hex`, `scope_seed_hex`, case and fixture digest.
Each preflight and immediate recheck found **0 resident source payload pages
among 27,503**; the recheck-to-timer gaps were 1.606 ms and 0.957 ms. This
still leaves metadata/dentry cache unqualified. Both locked release builds and
the Core harness seal matched the preregistered product/harness identities.
The [control receipt](evidence/c1-direct/h1/raw/control/receipt.json) and
[candidate receipt](evidence/c1-direct/h1/raw/candidate/receipt.json) retain the
original caller timer, command wall, CPU/RSS scope, telemetry and cleanup. Raw
preflight, recheck, identity sidecar, verifier result and telemetry are beside
each receipt; the closed Stores remain in their private output directories.

| H1 measure | Control | Candidate | Difference |
| --- | ---: | ---: | ---: |
| Public Init caller | 1.615643334 s | 1.381173500 s | −0.234469834 s (−14.512%) |
| 300 MB decimal throughput | 185.685 MB/s | 217.207 MB/s | +31.522 MB/s |
| Complete performance command, excluding later verifier | 2.084575625 s | 1.837776500 s | −0.246799125 s |
| Service + daemon lifecycle CPU | 2.257530 s | 2.003170 s | −0.254360 s |
| Service sampled maximum RSS | 58,703,872 B | 59,588,608 B | +884,736 B, not a phase peak |
| Separate full verifier wall | 3.030436 s | 3.182914 s | outside the speed denominator |
| Store apparent bytes | 333,910,016 | 333,910,016 | 0 |
| Store allocated bytes (`st_blocks × 512`) | 346,107,904 | 335,491,072 | −10,616,832 |
| Pack BLOB capacity (`sum(length(data))`) | 331,087,872 | 331,087,872 | 0 |
| Pack assembled/used bytes | 305,973,797 | 305,973,738 | −59 |
| Pack count / object count | 1,263 / 24,683 | 1,263 / 24,683 | 0 / 0 |
| SQLite page size / page count | 4,096 / 81,521 | 4,096 / 81,521 | 0 / 0 |

The exact canonical root is
`e7850f75a65309568e3454f7ab962aa16952d0c02218a55f42f3e2fd44ddf673`
in **both** calls. All 24,683 distinct stored object IDs match exactly, and
the root ID is present in both Stores ([read-only comparison](evidence/c1-direct/h1/comparison.json)).
Because object IDs authenticate canonical bytes and the two root graphs name
the same IDs, this establishes the same reachable canonical object graph for
the two completed Stores; the full reopened readback separately checks the
meaning of the graph. Both existing `verify_namespace` children returned
`PASS` within the unchanged 5 s watchdog: 10,101 paths, 101 directories,
10,000 files, modes and mtimes for every path, and SHA-256 of all 300,000,000
read-back file bytes. The History publication root was checked after reopen.
Save 3, the C1 tree save, has **308 objects, three packs, 786,432 B capacity,
and 489,198 B used in both arms**. Whole-Store apparent bytes and pack capacity
also agree; the 59 B used difference is entirely in the preceding file Save
([control geometry](evidence/c1-direct/h1/control-geometry.json),
[candidate geometry](evidence/c1-direct/h1/candidate-geometry.json)). No dense
Init Store-space regression was observed. This does not settle the separate
#229 sparse-history lane.

**Status remains qualified.** The control's public operation and full
verifier succeeded, but the daemon lost one telemetry event: its receipt is
`INCOMPLETE`, with stderr and raw telemetry retained. The candidate's
telemetry/cleanup and verifier passed, but the runner marks it `INELIGIBLE`
because its cache contract is still `source-cache-uncontrolled-v1`.
Neither row is a fully cold #231 admission PASS. The H1 times are one raw
observation each, not a repeated treatment selection. At 217.207 MB/s, the
candidate still misses the 700 MB/s goal by 0.952602 s on the 300 MB public
timer; the file ingest span remains the main future work.

## C1 verification and adoption remainder

The four formerly failing `filesystem_ordering` assertions now run their
spill, threshold, append/read/flush and checked-release cases through a real
base-root **update**. The fresh-build test separately checks the direct
counter path and the inclusive `ordering_bytes / 16` cardinality bound.
`cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content`
passed in full at the candidate source/test identity; its ordering target
passed 17/17. No production source change was needed after the initial
prototype commit. The C1 algorithm remains on this isolated research branch,
unmerged and unpushed. A later adoption decision still needs the #229
sparse-history compactness/readback lane and the repository's final release
checks. The prior unfixed-identity pair and its failed root/Store equality
result remain intact in [the first experiment](c1-direct-prototype.md).
