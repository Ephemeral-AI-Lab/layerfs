# #237: v0.1.6 release Init on the exact Core 100k source

> **Status: Research; informative and not a product contract.** One retained
> public reference call. Payload pages were cold at GO, but namespace metadata
> cache state was not qualified. The 15-s complete-command and under-10-s
> verifier expectations both missed.

## Identity and boundary

The [prospective contract](v016-exact-100k-reference-contract-20260924.md)
froze the exact SHAKE seed-1 manifest
`23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`:
100,000 files, 1,001 directories and 500,000,000 logical bytes. The
benchmark-only harness commit was `61d1eb10d8096f1315640fc2623542a34685850d`;
its `crates` tree `dcc4fb6fd01115dcbf91ba02df414e91eb5733be` is
identical to peeled v0.1.6 tag `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`.
The locked Cargo **release** driver SHA-256 was
`641f710b76599d27437556490c9acd9a4636a9f7dfe6ae33dd1dc9ca211ddca0`.
The [identity receipt](evidence/v016-exact-100k-20260924/identity.json)
pins the wrapper, cold driver, manifest, seed and one public command.

The wrapper made a [fresh independent byte copy](evidence/v016-exact-100k-20260924/source-copy.json).
After the released client reached READY, [full source validation/invalidation](evidence/v016-exact-100k-20260924/cold-preflight.json)
and a [nonfaulting recheck](evidence/v016-exact-100k-20260924/cold-recheck.json)
both found **0/126,206 resident payload pages**. GO-to-public-timer
preflight gap was **0.353 ms**. The timer surrounded one authentic
`Client::initialize_layerstack(...Directory(copy))` call. Fresh Store/client
setup and Store readback stayed outside that timer. Directory/inode metadata
residency remains unknown.

## One call, independent oracle and resources

| Measurement | Observed |
| --- | ---: |
| Public Init call | **3.779070375 s** |
| Complete performance child including cold preflight | **19.119348541 s**; exceeds 15 s |
| Process user + system CPU inside Init | 3.125596042 + 4.220182875 = **7.345778917 s**; workers overlap |
| Process lifetime peak RSS at public-call end | **92,405,760 B**; incremental peak above t0 **87,932,928 B** |
| Canonical objects inserted | **112,424** |
| Separate full reopened verifier | **PASS**, all 101,001 paths, every kind/mode/mtime, 100,000 SHA-256/size checks, 500,000,000 bytes |
| Separate verifier command | **21.895595708 s**; exceeds under-10-s expectation |

The [raw public receipt](evidence/v016-exact-100k-20260924/receipt.json)
and [verifier receipt](evidence/v016-exact-100k-20260924/verification.json)
retain the values and statuses. The verifier reopened a copy of the closed
Store using typed genesis/root IDs; it did not run inside public or complete
performance time. The complete command's miss is largely untimed source
validation and zero-residency checking, not extra public Init work. This is
diagnostic evidence, **not** a budget or fully cold admission PASS.

## Closed Store geometry on the matched source

Read-only [geometry](evidence/v016-exact-100k-20260924/store-geometry.json)
pins the original Store SHA-256
`6bd5587f425364e3c962dea58a768bbbfe5f53902ea4e098bff9017de2a8eabb`.
The single v0.1.6 SQLite file is **514,879,488 B apparent /
520,110,080 B filesystem allocated**: 125,703 pages × 4,096 B, of which
20 pages (81,920 B) are freelist. It contains **2,690 packs /
507,025,429 B of pack BLOBs** and **112,424 object rows**. Versions are
v1: 9 packs, v2: 1,116, v4: 1,501, and v6 pooled metadata: 64. The old
variable-length BLOBs expose no distinct post-used reservation: bounded
header/directory reads found no gap or trailer for v1/v2/v6; v4 defines
the final group's end as BLOB length.

| SQLite named allocation | Bytes |
| --- | ---: |
| `object_packs` B-tree | 508,952,576 |
| `objects` B-tree | 5,726,208 |
| Metadata group table/index | 53,248 |
| Schema and other named tables/indexes | 65,536 |
| Freelist | 81,920 |
| **Total apparent** | **514,879,488** |

On this exact manifest, the retained [Core release control](sdk-100k-release-result-20260924.md)
is **39,219,200 B larger apparent** and **38,035,456 B larger allocated**
when its content Store **and** History are counted. Its 35,168,077 B
declared pack tail is **89.67%** of the apparent gap, but the remaining
bytes include format/row/index differences. Its 5.077702667-s SDK call
uses a different public surface; the two raw times do not prove a
version-to-version speed regression. The old historical #152 100k row used
different source bytes and remains archival context.

Raw data, including the closed Store and independent source copy, remains
under `benchmark-results/issue237-v016-exact-100k-20260924-01/` in the
isolated v0.1.6 worktree. The linked evidence here is a byte-for-byte
curation of its small receipts, not a rewritten historical row.
