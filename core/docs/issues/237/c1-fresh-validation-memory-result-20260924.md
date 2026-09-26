# #237: fresh C1 validation state memory result

> **Status: Research; informative and not a product contract.** One
> prospectively frozen release diagnostic against the retained direct-inode
> control. The candidate passed full reopened readback but missed its memory
> gate and is being reverted. Neither raw time is a speed-admission result.

## Treatment and custody

The candidate kept the public `validate::check` result unchanged while its
private fresh-build call omitted unused zero-count `additions` rows. It
reused the already-sorted directory updates for build reachability, queued
only directory children, and identified unbound new parents without first
storing every bound child. The generic update's result semantics, canonical
directory/inode engines and Save boundaries were unchanged. The
[prospective plan](c1-fresh-validation-memory-plan-20260924.md) froze an
**8-MiB whole-call peak RSS reduction** requirement before C1 source edit.

The retained [control](evidence/c3-inode-fusion-20260924/candidate/receipt.json)
and new [candidate](evidence/c1-fresh-validation-20260924/candidate/receipt.json)
used harness seal
`96e74ab69821c13c33649c5873003151c72d0502a7657980860686047c1614a6`.
The [freeze](evidence/c1-fresh-validation-20260924/freeze.json) pins
candidate instrumented product seal
`aea0b49350972248e0ef02d2bbad6cb8ada6b45362869182f14bf800e37f548f`;
the temporary [instrument diff](evidence/c1-fresh-validation-20260924/instrument.diff.gz)
and [hash manifest](evidence/c1-fresh-validation-20260924/SHA256SUMS.json)
retain custody. The candidate made one real `Client::init_project` call
on a fresh independent copy of the same seed-1 SHAKE 100k/500-MB manifest,
with a fresh Store/History and 0/126,206 resident source payload pages at
launch. Metadata/dentry cache state was unqualified. Its separate full
[reopened oracle](evidence/c1-fresh-validation-20260924/candidate/verification.json)
passed all 101,001 paths, portable metadata, file sizes and SHA-256 of
500,000,000 bytes. The raw Store and source copy remain under
`benchmark-results/fs-bench-pro/issue237-c1-fresh-val-20260924-01/`.

## One-shot result and decision

| Measure | Retained control | C1 candidate | Candidate minus control |
| --- | ---: | ---: | ---: |
| File-loop-end peak RSS | 105,332,736 B | 106,364,928 B | +1,032,192 B |
| Tree-input peak RSS | 105,332,736 B | 106,364,928 B | +1,032,192 B |
| Tree-build-end peak RSS | 127,057,920 B | 121,618,432 B | −5,439,488 B |
| **Whole-call peak RSS** | **127,631,360 B** | **125,091,840 B** | **−2,539,520 B (−2.42 MiB)** |
| Raw public-call time, diagnostic only | 5.410089250 s | 5.462784292 s | +0.052695042 s |
| Store apparent / allocated bytes | 519,819,264 / 527,106,048 | 519,888,896 / 522,694,656 | +69,632 / −4,411,392 B |
| Full reopened oracle | PASS | PASS | — |

The candidate's namespace interval high-water fell, but earlier file-work
high-water was 1,032,192 B higher in this independent operation. The
**2,539,520-B whole-call reduction missed the frozen 8,388,608-B gate**.
The source is rejected for the memory objective and will be reverted; no
unchanged arm is resampled. The 52.7-ms raw slowdown is also not a qualified
speed result because directory/inode metadata cache state is unknown, and
physical pack placement differs between independent Stores. Exact root ID
equality is unavailable because the public SDK chooses fresh random stack
and scope identities; full logical readback did pass. See the
[arithmetic](evidence/c1-fresh-validation-20260924/comparison.json).

Focused `layerfs-content` tests passed in the isolated prototype checkout,
including canonical reference roots, topology refusals, empty directories
and sorted-page cases; formatting passed there. No full Core source gate is
claimed for the rejected prototype. The registered #236 debug SDK 100k
selection and #229 sparse-history compactness remain separate.

The conditional file-job frontier still does not trigger: candidate
namespace Save peaked at **125,091,840 B**, above file-loop high-water
106,364,928 B. The kept direct-inode treatment's 17.22-MiB reduction
remains the measured improvement; a truly bounded fresh builder still
requires record custody, exact serial order, bounded validation and
cache-accounted backing rather than another in-memory map tweak.
