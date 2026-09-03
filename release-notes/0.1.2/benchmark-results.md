# LayerFS 0.1.2 terminal benchmark evidence

> **Status:** Final Developer Preview evidence index for `v0.1.2`.

The raw directories below are immutable local custody, not Git-tracked release
assets. Each identity is the SHA-256 of that directory's `evidence.sha256`
manifest. The tracked release record and issue comments publish the summary,
paths, and hashes without pretending the multi-gigabyte raw runs are hosted.

## Universal edit-engine conformance

Final production-seal conformance covers the Workspace unit suite, all seven
file-edit groups, reconciliation, diagnostic-seed gating/decoding, scoped
warning-denying Clippy, and two real Linux FUSE proofs. It is retained at
`final-v012-issue14-19af57ef`, manifest
`9e18afc5ccafba5434b10044b9dec0a79842b51234513ad9ef3f178e08564f4e`.
The final custody bridge manifest is
`516b436eca0b73f30bc3d15cfd6f93eb0308938ea4124be5582d04efb3c8473d`.

The last complete record before the release-only version/docs and final seed
gate proof is:

`benchmark-results/fs-bench-pro/edit-engine-acceptance/final-v012-issue14-ec2ede20`

Manifest: `7433ee0888ee18e956853226ce2390cd137061603b5d4c5dd65dbade1d78a58a`.
The earlier authoritative seven-group record remains
`issue14-terminal-r005-20260903`, manifest
`569ce2675cc96a2be39900f073f21220e64dd22104769a4276ad10d5af9b9ccc`.

## Same-count family

The 14-ID final-product same-source repeatability run is:

`benchmark-results/fs-bench-pro/edit-same-count/final-v012-same-count-b2c92cd8`

- manifest: `a31f0bbb43ae0b081b7a1c92f9c3424f94440f05793d21681fc0b817a956eb05`;
- exact anchor custody supplement:
  `final-v012-same-count-b2c92cd8-custody-supplement`, manifest
  `598ac957403a588ded48100492cb6113afdc748fb2d3bec893080c09d87b4134`;
- 84 timed rows and seven separate fragmentation receipts;
- symmetric aggregate identical-source A/A ratio `1.027103819`;
- arm walls `1.484462623 s` and `1.445289751 s`, paired `2.929752374 s`;
- peak RSS `77,987,840` bytes, zero swap, no OOM, cleanup pass.

Identical-source A/A repeatability uses one sealed daemon and gates the
label-invariant symmetric aggregate family-wall ratio. Per-case A/A ratios are
diagnostics. This owner-approved rule does not apply to a directional product
comparison: baseline/candidate admissions continue to gate every member.

The prospective v3 rule and first accepted final-source run are retained at
`issue13-final-source-terminal-v3b-025c542f`, manifest
`c19d31f26065d712ae9e62707b6c1d9f59328ec87bb11f1ebd364dbeb544a4d2`,
with its separate fragmentation verifier manifest
`53bfbc1a040e510c314803e3a885ea6ac2de63b4b1844bc14b0b67d0c135e882`.

## Count-changing family

The authoritative directional 25-ID optimization campaign is:

`benchmark-results/fs-bench-pro/edit-count-changing/issue15-terminal-final-fresh-bb95b08c`

- base manifest: `d8953f1f179996c378b749de4bbcbda2a567c97f7cd6d68b700ece4b272a3238`;
- ratio-of-medians supplement: `issue15-terminal-authoritative-supplement-d1a7389b`,
  manifest `270fd1c4ccbd2aae758c1a9ca3d84291548044b294dfe6d0de9099a7fb0afd44`;
- complete tolerated-row dispositions: `issue15-terminal-ratio-dispositions-e76ef180`,
  manifest `e1beb19da1f361a76661a7a4b3cbd4603980044a4193db400f87bbfaa39177c0`;
- 150 directional performance rows, 45 controls, and seven separate verifier
  receipts pass;
- maximum ratio of medians is `1.0746733575`; all three values above `1.05`
  have phase/counter dispositions and make no improvement claim.

The final-source same-product A/A release-seal run completed all 150
performance rows but is intentionally nonauthoritative:

`benchmark-results/fs-bench-pro/edit-count-changing/final-v012-count-changing-b2c92cd8`

It is sealed diagnostic/no-go because five absolute per-arm throughput medians
missed; its manifest is
`f866ffb9e21816c90082fe3154347f83aa99c00a102f76223acdb6340939d3a6`.
No verifier was run after that no-go. Exact anchor replay custody is retained in
`final-v012-count-changing-b2c92cd8-custody-supplement`, manifest
`97043e011d05dd98311b3dc9a14e45d9b9a6c5652539a9f4a1339f88e472dfb9`.
This diagnostic never replaces the passing directional issue #15 evidence.

## Store footprint

The authoritative v4 baseline is:

`benchmark-results/fs-bench-pro/store-footprint/issue16-baseline-terminal-v4-fabf5eb8`

Manifest: `517cbf392098a23ece9d87df70c9d112a3b2613b777992e56db864c6d8643408`.
It contains nine performance Stores and three separate exact verifier Stores.

| Control | Canonical bytes | Durable median | Verifier |
|---|---:|---:|---:|
| unique-100000 primary | 542,909,962 | 661,061,632 | 43.182 s, target |
| metadata-cardinality-100000 | 579,605,932 | 734,003,200 | 64.372 s, tolerated |
| large-object-500m | 501,649,815 | 596,377,600 | 4.103 s, target |

The final workload's content, file mode/mtime, root and directory mode/mtime,
fresh reconnect, resource, census, and cleanup verification is retained at
`final-v012-store-verification-supplement-ec2ede20`, manifest
`801d003bc75f59df65abf64c8f550d1204163661698e7dd7583c5309a5ecf1af`.
These are correctness supplements, never performance samples.

No patch-compatible mechanism reaches the 600,000,000-byte primary goal. The
owner accepted the exact blocker and retained the current ObjectId/SQLite
format. The layout/pack experiment manifest is
`7c3c2cf0d45768352142448548f364a0126117dd35c13206a408d215d722d7b1`;
the terminal disposition manifest is
`f825494d3fafa0837b607209d59e9b1249014844cb19286cb18f3396e27c016a`.

The physical-pack experiment establishes a conservative object-storage lower
bound of `562,513,789` bytes: `542,909,962` pack bytes, `18,817,024` location
index bytes, `786,432` current non-object Store pages, and `371` manifest/
checksum bytes. All `422,085` segments match their source bytes, but a real
Store may add publication/journal state. It is not production-ready, is not in
v0.1.2, and remains open as issue #18.

## Publication identity

The benchmark seals bind the exact measured development sources. The annotated
tag, CI check, deterministic source archives, and `SHA256SUMS` bind the release
source. Release-only version/docs changes and the final diagnostic-seed helper
refactor do not rewrite retained raw evidence; final conformance records the
scoped product delta.
