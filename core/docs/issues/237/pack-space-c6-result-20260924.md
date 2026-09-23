# #237 C6: sparse space PASS, dense allocation guard FAIL

> **Status: Research; informative and not a product contract.** One
> C6 sparse release probe and one Core SDK 100k release diagnostic under
> the [frozen half-full plan](pack-space-c6-half-full-plan-20260924.md).
> The sparse mechanism works on the focused input. The dense allocated-byte
> rule misses; C6 is **not** a combined acceptance PASS or a #229 proof.

## Frozen source, build and readback

The sampled clean C6 source was `f059b8b7f9a8009dc4ee8091dac52ce74eacdf6a`
with product seal
`1f91717aeaf6d500825e675ff636e1ae96a42e0324a5deebb8c20a56fc3b787e`.
The 17-Save sparse probe used the exact benchmark-example source SHA-256
`0ff00e0b259e2d71020528de0199f6182a496c0d36164a9d588dd5ffc589094f`
shared with the retained C3 and C5 sparse arms. Its C6 locked Cargo
**release** binary SHA-256 was
`0ff064d7303d325ea58333090d8697f3b681c89226e3bf6234ef82d29b99084a`.
The [sparse receipt](evidence/sparse-17save-c6-20260924/receipt.json)
pins those identities. All 17 public Saves returned; the separate reopen
matched every canonical object ID and exact bytes: **PASS**.

The 100k diagnostic used the same Core SDK release runner harness seal
`96a04c70338d34116c1c66ac04617bb77df368fbcae4e0c1ca29ac04f48e90a3`
and seed-1 SHAKE manifest
`23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`
as the retained C3 control. The [build](evidence/packspace-c6-20260924/build.json),
[run identity](evidence/packspace-c6-20260924/run.json) and
[public receipt](evidence/packspace-c6-20260924/receipt.json) retain
their hashes. Final [source check](evidence/packspace-c6-20260924/cold-launch.json)
found **0/126,206 resident payload pages**; directory/inode metadata
cache remains unqualified. The [independent reopened verifier](evidence/packspace-c6-20260924/verification.json)
passed all 101,001 paths, portable metadata and 500,000,000 source
bytes in **8.897570750 s**. The raw run hash manifest check returned PASS.

## Measured sparse improvement

| Shared 17-Save release probe | C3 control | C5 | **C6** |
| --- | ---: | ---: | ---: |
| Closed Store apparent and allocated | 4,608,000 B | 483,328 B | **483,328 B** |
| Pooled v12 rows | 17 | 17 | **17** |
| Pooled BLOB lengths | 4,456,448 B | 75,665 B | **75,665 B** |
| Pooled declared used | 75,665 B | 75,665 B | **75,665 B** |
| Pooled post-used tail | 4,380,783 B | 0 B | **0 B** |
| SQLite 4-KiB pages / freelist | 1,125 / 0 | 118 / 64 | **118 / 64** |
| Reopened 17-object byte/ID oracle | PASS | PASS | **PASS** |

C6's [read-only sparse geometry](evidence/sparse-17save-c6-20260924/geometry.json)
has the **same closed Store SHA-256**
`032b1c54efa4d479c66b4f5e1720528c2f65a5e112329b8b9c04069f2b979522`
as C5. The **4,124,672-B allocated reduction** from C3 exceeds the
planned 3,000,000-B focused threshold. It demonstrates page reuse over
17 sparse Saves, with 64 final free pages still in the file. It is not
the `history-stride10` or `history-stride1` workload and cannot close
their full-path oracle. The [raw sparse stdout](evidence/sparse-17save-c6-20260924/probe.stdout)
and [lifecycle time/RSS](evidence/sparse-17save-c6-20260924/probe.time)
retain the one-run resource readings.

## Dense guard and attribution

| One raw Core SDK 100k release row | C3 control | **C6** | C6 minus C3 |
| --- | ---: | ---: | ---: |
| Public SDK call | 5.013439250 s | **5.186846834 s** | +0.173407584 s |
| Complete driver command | 6.397864750 s | **6.544249666 s** | +0.146384916 s |
| Driver lifecycle peak RSS | 154,189,824 B | **155,205,632 B** | +1,015,808 B |
| Store + History apparent | 520,003,584 B | **519,942,144 B** | **−61,440 B** |
| Store + History allocated | 523,399,168 B | **525,819,904 B** | **+2,420,736 B** |
| Store SQLite pages / freelist | 126,933 / 0 | **126,918 / 0** | −15 / 0 |
| Pooled v12 BLOB length/used | 4,194,304 / 4,051,316 B | **identical** | 0 / 0 |

The frozen dense allocation maximum was **524,399,168 B** (C3
523,399,168 + 1,000,000). C6 exceeded it by **1,420,736 B** and
therefore **FAILS** that rule. The [C6 Store geometry](evidence/packspace-c6-20260924/candidate-pack-geometry.json)
pins original Store SHA-256
`66a53099f68c7f13dbb7feab799c0b2eb32fcd39b3b9e3664680688b2185dfe2`.
Its SQLite file is 15 pages **smaller** than C3's, with no freelist in
either. The entire allocated-byte increase is the
`st_blocks*512 − st_size` excess changing from **3,395,584** to
**5,877,760 B** (+2,482,176 B), offset by 61,440 B fewer pages.
Read-only `dbstat` assigns the 15 fewer pages to `object_packs`
(8) and `objects` (7); every other named table/index has the same
page count. Native v15 pack rows also differ (C3 944, C6 937), so the
single pair cannot attribute the filesystem block change causally to
the finish rule.

The dense no-rewrite path has independent support: the C6 source skips
the SQL update above **131,072 B used**, its final pooled pack declares
**133,675 B used**, all 16 pooled rows remain 262,144 B, the Store
freelist is zero, and a
[read-only comparison](evidence/packspace-c6-20260924/pool-multiset-compare.json)
of all 16 v12 BLOBs with C3 gives the same sorted multiset digest
`c785c716c19ebef2d7cd833b6cc391a74056df0c661ffd0a187ff5c9b0846310`.
This explains why the treatment did not rewrite the dense pooled pack;
it does **not** turn the allocated-byte miss into a PASS. The raw
time/RSS differences are one-run observations with unqualified metadata
cache and changing native pack placement, not a causal slowdown claim.

## Decision and limits

**Retain C6 as an experimental source change, not an admitted combined
space fix.** Its sparse Store result is strong for the focused mechanism.
Its 100k combined allocation is still **32,325,632 B below** the original
Core control but **5,709,824 B above** the exact-source v0.1.6 Store,
and it missed the prospective C3-relative dense cap. Do not resample
the unchanged arm, move the allocation term into setup, or relax the
gate after seeing it. C3 remains the retained dense reference.

The complete #229 matched history gate remains open: historical
`history-stride10` commands take 38–40 s against 15/25-s bounds,
the prior matched stride1 attempt stopped at
`Integrity("dependency encoded work")`, and its verifier cannot
exclude unexpected paths. The small sparse probe cannot replace that
proof. The raw C6 sparse Store is under
`benchmark-results/issue237-sparse-c6-17save-20260924-01/`; the
raw dense run, including its closed databases and hash manifest, is at
`benchmark-results/fs-bench-pro/sdk-100k-packspace-c6-20260924-01/`.
The benchmark example's hex formatting was cleaned up **after all
three sparse probes**, in commit `f43c10db2`, to pass all-targets
warning-denying Clippy; no historical probe was repeated or relabelled.
