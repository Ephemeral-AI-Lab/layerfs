# #237 pack-space C3: pooled reuse with exact payload packs

> **Status: Research; informative and not a product contract.** One retained
> Core SDK 100k release candidate under the
> [prospective C3 plan](pack-space-c3-plan-20260924.md). The dense Store
> allocation improvement is measured; metadata-cold admission and the separate
> #229 sparse-history guard remain open.

## One-shot identity and correctness

The clean committed candidate was `a3628330863398f0cd02e3c789558fd6b2559722`,
with product change `4b94e9131` and the same runner harness seal
`96a04c70338d34116c1c66ac04617bb77df368fbcae4e0c1ca29ac04f48e90a3`
as the retained Core control, C1 and C2. Locked Cargo **release** examples
used the same seed-1 SHAKE 100k manifest
`23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`,
a fresh independently checked byte copy, and a fresh Store. The
[run identity](evidence/packspace-c3-20260924/run.json),
[build receipt](evidence/packspace-c3-20260924/build.json) and
[copy receipt](evidence/packspace-c3-20260924/source-copy.json) retain exact
seals. Final [pre-call residency](evidence/packspace-c3-20260924/cold-launch.json)
was **0/126,206 source payload pages**. Inode/directory metadata cache state
remains unqualified, so the single raw latency is performance
`INELIGIBLE`.

The [public receipt](evidence/packspace-c3-20260924/receipt.json) has one
`Client::init_project` call, cleanup PASS and no dirty source paths.
The [separate full reopened verifier](evidence/packspace-c3-20260924/verification.json)
passed all 101,001 paths, portable mode/mtime, every file size/SHA-256 and
500,000,000 logical bytes in **8.644194375 s**. The raw run's hash-manifest
check returned PASS. Core workspace tests, warning-denying Clippy, locked
examples, formatting, boundary guard and its self-tests passed on this
source; an earlier test compilation failed on a test-only SQL `usize`
conversion and was corrected before this one public sample.

## Measured space and resource result

| One raw 100k release row | Core control | C2 | **C3** |
| --- | ---: | ---: | ---: |
| Public SDK call | 5.077702667 s | 5.017461833 s | **5.013439250 s** |
| Complete driver command | 6.408342167 s | 6.387194125 s | **6.397864750 s** |
| Driver lifecycle peak RSS | 155,631,616 B | 154,173,440 B | **154,189,824 B** |
| Pack rows | 2,082 | 4,282 | **2,297** |
| Pooled v12 pack rows | 16 | 2,001 | **16** |
| Pack BLOB capacity | 545,783,808 B | 519,366,346 B | **511,331,101 B** |
| Pack declared used | 510,615,731 B | 519,366,346 B | **511,188,113 B** |
| Post-used pack tail | 35,168,077 B | 0 B | **142,988 B** |
| Store + History apparent | 554,098,688 B | 528,216,064 B | **520,003,584 B** |
| Store + History allocated | 558,145,536 B | 534,118,400 B | **523,399,168 B** |

C3 saved **8,212,480 B apparent / 10,719,232 B allocated** from C2,
exceeding the prospective ≥7,000,000-B allocated reduction. Against the
original Core control, combined allocation fell **34,746,368 B** (91.35%
of the former 38,035,456-B matched gap to v0.1.6). C3 remains
**5,124,096 B apparent / 3,289,088 B allocated above** the exact-source
[v0.1.6 reference](v016-exact-100k-storage-result-20260924.md). The
public time differs from C2 by only −0.004022583 s in one sample; that
is an observation, not a causal speed claim.

Read-only [pack geometry](evidence/packspace-c3-20260924/candidate-pack-geometry.json)
pins Store SHA-256
`776060b1fa0d2ab0dc0d78672955906762f6d9e4d3f24af13ded71392a2ed2df`.
The Store is 126,933 × 4,096-B pages, zero freelist,
**519,917,568 B apparent / 523,313,152 B allocated**; History adds
86,016 B apparent and allocated. The 16 pooled rows occupy 4,194,304 B
of BLOB capacity and declare 4,051,316 B used, exactly the control
grouping and 142,988-B pooled tail. The 2,001 pooled groups remain
immediately visible on their existing Save path. The other 2,281 payload
pack rows are exact length; the external pooled same-Save/reopen and
payload placement tests passed.

**Decision:** C3 is the better dense-namespace space treatment. It does
not by itself close the remaining 3.29-MB matched allocation difference or
the #229 sparse-history acceptance guard. No C3 repeat or control resample
is planned. The complete raw output, including closed databases and hash
manifest, remains worktree-local at
`benchmark-results/fs-bench-pro/sdk-100k-packspace-c3-20260924-01/`.
