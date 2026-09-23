# #237 C5: final pooled-row shrink, sparse PASS and dense space FAIL

> **Status: Research; informative and not a product contract.** This
> [prospective treatment](pack-space-c5-sparse-tail-plan-20260924.md)
> made one bounded whole-BLOB rewrite at Save finish. Its one sparse
> release diagnostic improved closed-file space, but its one dense release
> diagnostic missed the frozen allocation guard. Both receipts remain
> diagnostic; C5 is **not accepted**.

## Shared 17-Save sparse probe

The C3 control ran from clean source
`5a25f4cf2ef89f0ee2153c53be7e99725ed91423` in an isolated
worktree. The C5 candidate ran from clean source
`45ca25d849dd739c552830d976cfb6f618abe63b`. Both locked Cargo
**release** examples had identical harness source SHA-256
`0ff00e0b259e2d71020528de0199f6182a496c0d36164a9d588dd5ffc589094f`
and used the same frozen default StoragePolicy, 17 Saves, four unique
inode values per Save and new Store per arm. The binary SHA-256 values
were `52d8b7b39ebc917589502efdd95ac62394a1624e93e630ccefcc70ca356e2384`
for C3 and
`18b426fedc67b6c0931bda63ed0e45498aa2ee5e678be4edbec8d26c40020258`
for C5. Each example separately reopened its Store and authenticated
all 17 canonical objects, including IDs and exact bytes: **PASS**.
The [C3 receipt](evidence/sparse-17save-c3-20260924/receipt.json)
and [C5 receipt](evidence/sparse-17save-c5-20260924/receipt.json)
pin binary, harness and output identities.

| One raw sparse release row | C3 control | C5 candidate | Difference |
| --- | ---: | ---: | ---: |
| Closed Store apparent | 4,608,000 B | **483,328 B** | −4,124,672 B |
| Closed Store allocated | 4,608,000 B | **483,328 B** | **−4,124,672 B** |
| SQLite 4-KiB pages / freelist | 1,125 / 0 | **118 / 64** | −1,007 / +64 |
| Pooled v12 rows / groups | 17 / 17 | **17 / 17** | no grouping change |
| Pooled BLOB lengths | 4,456,448 B | **75,665 B** | −4,380,783 B |
| Pooled declared used | 75,665 B | **75,665 B** | no payload change |
| Post-used pooled tail | 4,380,783 B | **0 B** | −4,380,783 B |
| Sum of 17 public Save timers | 25.328459 ms | **22.404292 ms** | one-run observation |
| `/usr/bin/time -l` complete command | 1.33 s | **0.46 s** | coarse lifecycle readings |
| Process lifecycle peak RSS | 27,639,808 B | **26,165,248 B** | raw, separate process |

The read-only [C3 geometry](evidence/sparse-17save-c3-20260924/geometry.json)
and [C5 geometry](evidence/sparse-17save-c5-20260924/geometry.json)
include Store hashes, page counts, freelist and pack versions. This
**4,124,672-B allocated reduction** exceeds the plan's 3,000,000-B
diagnostic threshold. With `auto_vacuum=NONE`, the final 64 free pages
remain in C5's file; earlier freed pages were reused by later Saves. The
time difference is not a causal speed claim from one sample per arm.
An initial C3 shell redirection failed because its `benchmark-results`
parent did not exist. The [failure note](evidence/sparse-17save-c3-20260924/prelaunch-failure.txt)
records that the binary never launched; after untimed parent setup, the
one public C3 probe ran once.

## Dense exact-source 100k guard

C5's one clean Core SDK release row used the same SHAKE manifest
`23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`,
runner harness seal
`96a04c70338d34116c1c66ac04617bb77df368fbcae4e0c1ca29ac04f48e90a3`
and zero-resident payload-page preflight as the retained
[C3 control](pack-space-c3-result-20260924.md). Its
[receipt](evidence/packspace-c5-20260924/receipt.json) has one
`Client::init_project` call, cleanup PASS and no dirty paths. The
[full reopened verifier](evidence/packspace-c5-20260924/verification.json)
passed all 101,001 paths and 500,000,000 bytes in **9.174528625 s**.
The raw run hash manifest check returned PASS. Namespace metadata cache
remains unqualified, so performance is `INELIGIBLE`.

| One raw dense release row | C3 control | C5 candidate | Difference |
| --- | ---: | ---: | ---: |
| Public SDK call | 5.013439250 s | **5.093861709 s** | +0.080422459 s, one-run observation |
| Complete driver command | 6.397864750 s | **6.486659333 s** | +0.088794583 s |
| Driver lifecycle peak RSS | 154,189,824 B | **155,860,992 B** | +1,671,168 B raw |
| Store + History apparent | 520,003,584 B | **520,130,560 B** | +126,976 B |
| Store + History allocated | 523,399,168 B | **530,206,720 B** | **+6,807,552 B** |
| Pooled pack BLOB lengths | 4,194,304 B | **4,065,835 B** | −128,469 B |
| Store SQLite pages / freelist | 126,933 / 0 | **126,964 / 64** | +31 / +64 |

The [C5 Store geometry](evidence/packspace-c5-20260924/candidate-pack-geometry.json)
pins its SHA-256
`b73e630aee4dfb97ba96c79dc15bf4199f86cbe06d9e93f3a3b2de4a3176dec5`.
The final pooled trim removed 128,469 B of BLOB tail but the closed
Store's apparent size rose 31 pages and its filesystem
allocated-minus-apparent term rose sharply. These are measured file
properties; the single row cannot isolate which part of the 6.81-MB
allocated increase is SQLite page movement versus filesystem allocation
variation. The plan allowed at most **+1,000,000 B**, so C5 **FAILS** the
dense space guard regardless of its sparse result. It must not be
promoted or resampled to obtain a passing number.

The [C6 prospective plan](pack-space-c6-half-full-plan-20260924.md)
uses a generic occupancy threshold to avoid the costly final rewrite for
the already more-than-half-full dense pack while retaining the sparse
benefit. The registered #229 history-stride guard remains separately
open: this 17-object probe is not that workload or its full-path oracle.
The complete raw C5 dense output is worktree-local at
`benchmark-results/fs-bench-pro/sdk-100k-packspace-c5-20260924-01/`;
the sparse raw outputs remain in their respective C3/C5 worktrees.
