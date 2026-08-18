# WP4-M M4.5 v3 terminal repair benchmark

## 1. Final disposition and scope

- Date: 2026-08-19.
- Final M4.5 disposition: **PASS** for the private K64/F64 exact-XOR,
  same-count changed-spine milestone.
- Decision: retain the changed-spine mechanism.
- Qualification: `false`.
- Promotion/profile selection/rejection: `false`.
- Production integration, full-create gain, F0, and later Phase 4 work: not
  started/not claimed.

The retained v2 campaign and the earlier nonterminal v3 directories remain
direction-only evidence. Only
`target/wp4m-m45-repair-k64-20260819-v3-terminal/` carries final acceptance.
The first v3 audit discovered an omitted decoded-root Q charge; the next audit
required explicit allocated main/journal/sidecar fields. Neither earlier
directory was overwritten or used for the terminal result.

## 2. Hypothesis and one changed variable

Both arms use one executable, the same prospectively amended operation, the
same independently prepared pair base, exact byte-copied database, authority
sidecar, and expectation file, the same exact FastCDC/COW mutation, one
transaction/COMMIT, and the same post-publication reopen, scrub,
reconstruction, and range checks.

The only changed variable is pre-COMMIT qualification:

- C0: complete requested transition/file closure authentication.
- C1: receipt-backed same-open changed-spine authentication; only
  witness-covered byte-identical immutable edges may be skipped.

Primary metric: durable same-middle edit latency. It is not 100-MiB/edit
throughput.

## 3. Prospective operation authority and frozen identities

`PHASE_4_WP4M_M4_5_OPTIMIZATION_SPEC.md` §13.3A prospectively amends the
experiment before accepted timing. The old §13.3 row remains verbatim and
withdrawn because exact Phase-2 FastCDC gives the uniform-`0x5a` stream 5,283
references.

The terminal binary contains a hard fixture gate. All 42 terminal official and
memory-extension rows agree on:

| Identity | Frozen value |
|---|---|
| Base source bytes / BLAKE3 | `104857600` / `bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7` |
| Edit | offset `52480416`; remove/insert `18854`; `inserted[i] = removed[i] XOR 0x5a` |
| Edited source BLAKE3 | `527b215f91735e023b23a2e970f86c9e25ea303d38a1e4006f3f3a2a98f9db49` |
| References / sequence | `5284` / `e6d6d858ab6ff9804839630df90a2e621ae06291e55ab12aea9957c566ec83f7` |
| Base root | `2d41c27f96b0332475fb8ec3c46a336c9c8a8084408bc545e5cbb24d51cb25d0` |
| Base transition | `ba15fd20469414de99c135fc90a5c5ad028f99f115b8c0d138ace9ec98536412` |
| Base closure | `d6aac6e40cc851dd6295dbeec6488f1c5ebefa7520f86b0cd12bdcdce1f0d54a` |
| Before / after file | `a94d42f6357b621ea51e306fe0a242854ed95d02d3e3dc7a88e3c2a20c194786` / `ab1f98a2c44c60f1b88f8aaec368ab2bbd68de9580e6e79b4dbf859800f2e7c8` |
| Result root | `d1a69475b0f8e25e44d7bd625a679b596ea2a8b3347ef8c15fafa13f654b299b` |
| Transition | `f11cc9d84deae7f1871adca62cc562ab63dbb01e9c39771ed3522eab4007cee1` |
| Ordered closure | `c0f6a39bf9939c89301bedb564516c5ec851321a1d89c69b2e95d4b1844a9587` |
| Expectation SHA-256 | `70520375af87d5227e28775a59879067d3b942cd82eb3f2fd2e15bb942b169ff` |

Exact local CDC reads 143,709 bytes and produces five replacement references.
The mutation writes eleven canonical objects, 110,745 canonical bytes, and
7,382 canonical mapping bytes. C1 covers 123 equal immutable edges, follows
eight new/different edges, and fully authenticates five new chunks / 103,363
canonical bytes.

## 4. Custody tuple

| Item | SHA-256 / value |
|---|---|
| Branch / HEAD | `codex/empty-worktree` / `f3df30a80172131b74b5949a6a55234c962dac67` |
| Measured implementation diff | `e08558c030040216489365a76c0643fa83e3f49aec9425ac06b78bba4d86057d` |
| Final terminal tracked diff | `d8284d2cc4594ff88a4c36b1e2cf827cee169d7354811e613b586a922f723f70` |
| Benchmark source | `994185370e7a510d6eee9ba0f115dd82e9302bfc697f58cc19b9d9b46a49da60` |
| Release executable | `f84e6b0f656e03ba3c537dbce08b085c3b52094a229b6df29593082e1d745ef1` |
| Fixture / manifest | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` / `8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca` |
| Raw official JSONL | `14c840f3de27225d5825c00998a1d26cee74641498310357d90dfd81bf795145` |
| Official preflight | `18d8da7d6844bce10a4113d7ff0c93ef00fb4cf83a77ef54a74402ef978bfef6` |
| Official summary / independent | `a2c4e37bea38bcdc823ab2eb542e179377f421cff11b5e708a206b019bc827e0` / `3a4f9b22a35120ace602961fc473efb6a1a91f2141e6a0da3738771d3195df90` |
| Official retained manifest | `e9cd136903ad1ac02a2d3dfc4daae214be613f19a7e9304fbcb896fcfc340f8d` |
| Memory-extension raw / preflight | `a008cb06f2d087e34bb944e9efc53b60ffba990a7016c9976690a5838891b85f` / `b0899e7e770a162047e91b7210d78f8accd0869ffe0129c0d35a8c91078eec20` |
| Memory summary / independent | `e7e2530e530e4edf47663f997c64bcec94fe64b8dd81d6a264a3df9b0a98690f` / `53bdc69808d04842761145af23ce62bf7ad3c1480ab01ba2f3c0f535506bf13d` |
| Memory retained manifest | `af745e754e1cc630ea124544945c1e4cb42aaae681f65516f81ae858a098212c` |
| Complete terminal tree manifest | `60887e2a4245fd3358f2242eac06b88e11051beacd3fc0bd0a2d7a7115f28cfd` |

All 12 official arm copies and all 30 extension arm copies match their pair's
database, authority, and expectation base hashes. The six official database
and authority identities are independently prepared and intentionally differ
between pairs; the expectation hash is fixed across all pairs. The complete
terminal manifest verifies 171 retained files. The retained v2 manifest also
verifies 15/15 entries and was not modified.

Official prepared-base custody:

| Pair | Base database SHA-256 | Base authority SHA-256 |
|---:|---|---|
| 0 (warmup) | `fa422abcda114f8356752a1dd2311ab5913a99ab7224b64015b593b87ac34c58` | `80fb523a92ad935f140e682d5c2a55f5a9efa42788f06f8ede954ddf74ccc5a9` |
| 1 | `a72adfebe211015aa431586926ef1ecf0853734ce55c5301b435a52d511d0935` | `0982ad4dbdf4f09752b90a28facb99086f0784e6e781e2e315853b86b092f763` |
| 2 | `7a5e1afef1baf2069ed36ac5e03d21b7dd146ca2c2af0bc8ff39c0eb151c7a68` | `58871a23203a29de16fc5d0758d5a93c183c30eac6a6e4aa8a87ea781abb26e3` |
| 3 | `04c43c7e6cfabed4212092926cd44e29f89260e3420a15f7e27cb83ee0a2ed96` | `62615e112d603f54b8b180ee77eb7d1d274cdad51cddac8d2b0e362d84b56732` |
| 4 | `3291e5b94ab12af6cf4aabb48ec6799897769b7200c9d02a492e8b22867cb940` | `7dc13fefbf669806bc866360cf65329a0221a3e9437dd4b3a443b07048ac22ae` |
| 5 | `37bc949b8581f5b3f4ec3bc36c243a49a8199a4df97fa5762157b2adafb90fd6` | `246accaf139f05d5c561363985c447b086669147609248f6edd157b109aea985` |

Each row's base expectations SHA-256 is
`70520375af87d5227e28775a59879067d3b942cd82eb3f2fd2e15bb942b169ff`.
The retained preflight records the matching C0 and C1 copy hashes before each
arm runs; the complete manifest records every base and arm image.

## 5. Commands and artifacts

Correctness commands:

```text
cargo test --workspace --offline --all-targets
cargo clippy --workspace --offline --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check HEAD
cargo run --offline -p layerfs-engine --bin phase4_create_edit_benchmark -- --self-test target/wp4m-m45-repair-debug-self-test-v3e
cargo build --release --offline -p layerfs-engine --bin phase4_create_edit_benchmark
```

Exact outputs: 96 tests passed and zero failed (44 core, 4 engine library,
31 benchmark, 12 parity, 5 eval; remaining all-target crates zero tests);
clippy/fmt/diff checks emitted no error; debug self-test reported
`self-test PASS root=f1cfdd7f... objects=20 auth_bytes=1054925`.

Terminal evidence root:
`target/wp4m-m45-repair-k64-20260819-v3-terminal/`.
Reproduction commands are in `wp4m-m45-repair.commands.txt` and
`memory-extension/wp4m-m45-memory-extension.commands.txt`. Raw rows,
preflights, pair bases, source/diff/toolchain records, binary, fixture,
manifest, summaries, independent recomputations, macOS `/usr/bin/time -l`
records, and complete hashes are retained under that root.

## 6. Official five-pair result

| Metric | C0 | C1 | Result |
|---|---:|---:|---:|
| Durable edit median | 440.023209 ms | 9.134334 ms | **-97.924124%**, 5/5 wins |
| Min / max / spread | 437.633000 / 443.282000 / 5.649000 ms | 9.081166 / 9.884333 / 0.803167 ms | protected speed PASS |
| Mapping + exact CDC/COW median | 6.515125 ms | 6.636708 ms | common mutation path |
| Pre-COMMIT qualification median | 430.447333 ms | 0.280583 ms | -99.9347% diagnostic |
| SQLite COMMIT median | 2.362916 ms | 2.351334 ms | common publication path |

Paired durable rows:

| Pair | Order | C0 ms | C1 ms | Delta ms | Delta % |
|---:|---|---:|---:|---:|---:|
| 1 | AB | 437.633000 | 9.123250 | -428.509750 | -97.915319% |
| 2 | BA | 441.672208 | 9.081166 | -432.591042 | -97.943913% |
| 3 | AB | 443.282000 | 9.626292 | -433.655708 | -97.828404% |
| 4 | BA | 440.023209 | 9.134334 | -430.888875 | -97.924124% |
| 5 | AB | 439.131208 | 9.884333 | -429.246875 | -97.749116% |

The result is within the conservative 8–10-ms planning envelope, though above
the 3–5-ms most-likely estimate. Classification: healthy local-work constant,
not a correctness tax or hidden full scan. Mapping/exact CDC is ~6.64 ms,
COMMIT ~2.35 ms, and C1 qualification ~0.28 ms; only 143,709 CDC bytes are
scanned. A 200–430-ms C1 full-scan regression is absent.

## 7. Lifecycle equations and separation

Every row checks:

```text
durable = mapping/COW + pre-COMMIT qualification + COMMIT
post-publication = reopen + fresh scrub + reconstruction + ranges
same-open lifecycle = durable + post-publication
first-open lifecycle = authority establishment + same-open lifecycle
```

| Median phase | C0 | C1 |
|---|---:|---:|
| Same-open authority establishment | 238.847125 ms | 237.833417 ms |
| Durable edit | 440.023209 ms | 9.134334 ms |
| Post-publication verification | 697.242792 ms | 694.629416 ms |
| Same-open complete lifecycle | 1,134.875792 ms | 703.763750 ms |
| Derived first-open lifecycle | 1,373.409959 ms | 940.826542 ms |

The local edit timer is never presented as the complete authenticated
lifecycle. Same-open witness establishment and fresh scrub remain independent
linear complete-closure phases; reconstruction remains linear in output.

## 8. Exact logical Q

All official rows independently satisfy:

```text
38,959  base live = 38,311 prepared expectations + 3 * 216 receipts
1,085,490 old authenticated CDC/range window
12,864  old RejoinChunk slots = 134 * 96
1,085,490 exact edited scan input
---------
2,222,803 bytes exact Q high-water
```

`q_cdc_overlap_current == q_high_water == 2222803` in both arms and every row;
`q_current=0` after the charged report output is delivered. Official report
outputs are 21,142–21,143 bytes for C0 and exactly 21,112 bytes for C1; they
are charged before allocation but do not exceed the CDC high-water.

The real SQLite canonical+decoded overlap test independently computes its
expected high-water. The exact 1-GiB test admits 1,073,741,824 bytes, rejects
the next byte before allocation, preserves the prior charge, and cleans up to
zero. Canonical builders, SQLite copies, decoded objects, file references,
tree nodes, DFS frames, delta paths, SQL, prepared expectations, eager ranges,
range/phase JSON, receipts, and final report output are all pre-admitted.

## 9. SQL/BLOB and storage accounting

| Counter (each official row) | C0 | C1 |
|---|---:|---:|
| Statement-cache acquisitions | 16,334 | 10,976 |
| SQL queries / executes | 16,418 / 18 | 11,060 / 18 |
| Rows returned / changed | 21,619 / 12 | 16,261 / 12 |
| Row BLOB reads / writes | 21,647 / 26 | 16,289 / 26 |
| Row BLOB copied bytes | 1,533,202 | 1,182,309 |
| Borrowed row BLOB reads / bytes | 21,201 / 420,927,421 | 15,922 / 316,104,492 |
| Transactions / COMMIT dispatches | 1 / 1 | 1 / 1 |

The counts include measured Store open and reconciliation-connection work;
each new object insert counts ID and canonical-byte BLOB writes.

| Post-row storage | C0 | C1 |
|---|---:|---:|
| Main DB apparent / allocated | 109,383,680 / 126,046,208 | identical |
| Journal apparent / allocated | 0 / 0 | identical |
| Authority sidecar apparent / allocated | 32 / 4,096 | identical |
| Total store apparent / allocated | 109,383,712 / 126,050,304 | identical |
| Allocated-store delta | 16,777,216 | identical |

Peak journal/temp, native prepare count, page-cache bytes, sync/fsync, and
byte-level host physical I/O remain explicitly `Unavailable`. W and D remain
`Unavailable`; narrower canonical/CDC/SQL/BLOB counters are not relabeled W/D.

## 10. CPU, RSS, and peak memory

Official CPU medians are 1.820 s (C0) and 1.380 s (C1); paired median change is
-24.176%, with C1 lower in 5/5 pairs.

The official five-pair RSS/peak directions were mixed and triggered §13.6.
The immutable official rows were preserved and 15 balanced pairs were added
with the identical executable/source/environment, producing 20 total pairs:

| Protected metric | C0 arm median | C1 arm median | Paired median | >5% regression pairs | Verdict |
|---|---:|---:|---:|---:|---|
| RSS | 18,366,464 | 18,178,048 | +2.566% | 8/20 | no repeatable regression |
| Peak footprint | 12,321,152 | 12,091,788 | +3.167% | 9/20 | no repeatable regression |

The governing repeatable-failure rule requires a paired median above 5% and
at least 16/20 regressions. Neither condition pair is met. The independent
memory recomputation agrees exactly.

## 11. Durability and authority evidence

- BEGIN counter/identity values are precomputed before `BEGIN IMMEDIATE`; the
  overflow regression proves no live writer, exact `LengthOverflow`, unchanged
  head, and immediate connection reuse.
- All post-BEGIN/pre-COMMIT failures route through `transaction_attempt`, roll
  back once, and retain exact first/cleanup/dominant causes, including
  `MissingObject(ObjectId)`.
- COMMIT dispatch is counted at dispatch. Prior-visible uses a real rejected
  SQLite COMMIT. Requested-visible uses a real successful COMMIT followed by
  a lost acknowledgement. Different-head uses a real successful COMMIT then a
  valid complete successor head. Ambiguous makes the fresh independent read
  genuinely unavailable. Every path uses production reconciliation.
- `Store::publish` returns `PublicationOutcome`; requested-visible success
  retains its first diagnostic instead of silently converting it to `()`.
- Once committed/requested-visible, later verification/report failure is
  wrapped as committed-publication failure and cannot relabel visibility.
- Same-open authority remains transaction/open/store/profile/epoch/head/
  receipt bound, single-use, and invalidated by reopen, mismatch, mutation,
  rollback, publication, reuse, and unresolved durability. Persisted receipts
  alone do not create cross-reopen authority.

## 12. Final decision and non-claims

All correctness, authority, durability, exact-Q, SQL/BLOB, split-storage,
copied-base custody, affected-speed, CPU, and protected-memory gates pass.
The read-only five-lane audit found no P0/P1 blocker. Final decision: **retain
and accept M4.5** as the private exact-XOR same-count changed-spine milestone.

This does not qualify or promote K64/F64, select a profile, integrate the
production engine, improve count-changing edits, claim full-create gain,
complete Phase 4, or start F0. F0 may begin only as a separate next work item.
