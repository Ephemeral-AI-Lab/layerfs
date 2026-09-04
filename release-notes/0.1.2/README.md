# LayerFS 0.1.2 Developer Preview

> **Status:** Released for LayerFS 0.1.2 Developer Preview.

**v0.1.2 delivers localized SDK file editing with millisecond-scale operation
times through 500 MiB, backed by five complete benchmark families.** The
[source-only Developer Preview is published](https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.2).

## What shipped

- **Universal regular-file range editing:** the public SDK expresses overwrite,
  append, prepend, insertion, deletion, growth, shrinkage, truncation and zero
  extension through the same range-edit operation. A single edit and an explicit
  same-file batch are supported; release performance cases each use one edit.
- **Localized content work:** the Workspace piece tree retains references to
  unchanged data; Commit processes final changed runs through the existing
  canonical-content machinery. Benchmarks call the real SDK rather than
  reconstructing edits with full-file temporary copies.
- **Lower FUSE presentation overhead:** owner-side edits invalidate the affected
  inode and resume the existing mount instead of tearing down and rebuilding it.
  Complete callbacks are drained before invalidation to prevent stale cache fills.
- **Correct close acknowledgement:** the daemon now drains the old watcher and
  releases the mount reservation before acknowledging close, fixing the
  immediate-remount `InvalidRequest` race without retries or sleeps.
- **Faster benchmark iteration:** reusable pristine input Stores, independent
  writable sample clones, family-local runners, and performance collection
  before separate final verification. Preparation reuse does not skip or cache
  the measured SDK operation.

## Measured optimization results

The comparison below uses **the same public SDK operation and identical
benchmark harness on both source arms**, with five samples per cell. It is not
a comparison against the old POSIX temp-copy benchmark or a direct v0.1.1
release-to-release speedup claim. Edit times are **median (min–max), in ms**;
reduction is calculated from the unrounded medians.

| Operation | File | Baseline Edit ms | Optimized Edit ms | Median latency reduction |
| --- | ---: | ---: | ---: | ---: |
| Head overwrite, 4 KiB | 1 MiB | 18.590 (15.186–23.099) | 2.643 (1.376–4.508) | 85.8% |
| Head overwrite, 4 KiB | 500 MiB | 16.151 (13.236–19.496) | 2.602 (1.537–2.947) | 83.9% |
| Prepend, 4 KiB | 1 MiB | 20.612 (15.781–23.596) | 1.674 (1.415–3.975) | 91.9% |
| Prepend, 4 KiB | 500 MiB | 20.187 (10.816–41.251) | 3.344 (1.970–4.394) | 83.4% |

Across all 56 candidate cases, **Edit medians range from 1.527 to 5.221 ms**.
These are per-case medians, not a claim that every individual sample falls in
that interval. The representative comparisons above are not a pooled speedup.
All operation/size pairs and both source arms are in the
[detailed SDK report](sdk-edit-benchmark-results.md).

### Prepend plus Commit across file sizes

One 4 KiB SDK prepend, **N=5 per size**. Edit and Commit columns are medians;
the combined column includes its observed min–max range. All times are ms.

| File | Edit median | Commit median | Combined median (min–max) |
| --- | ---: | ---: | ---: |
| 1 MiB | 1.674 | 2.919 | 4.680 (4.058–10.286) |
| 10 MiB | 2.164 | 2.779 | 4.883 (3.230–6.271) |
| 100 MiB | 2.967 | 4.289 | 7.257 (4.963–11.718) |
| 500 MiB | 3.344 | 11.122 | 14.300 (9.947–16.455) |

Combined medians are calculated from combined samples, not by adding separate
medians. The result is **localized, size-stable editing with bounded Commit
latency**, not identical operation times or size-independent Commit. Publication
and metadata work still contribute to Commit's increase at larger sizes.

### Memory and content-processing evidence

Across the 280 candidate SDK samples, the largest recorded **native process
lifetime RSS peak was 10.922 MiB** and **native cgroup lifetime peak was
6.652 MiB**. These are distinct resource scopes, not additive measurements or
exact edit-phase peaks. Maximum recorded Commit chunking work was **64 KiB**,
matching the largest replacement cases; edit-caused FUSE payload writes and
Workspace spool writes were **zero** throughout the candidate campaign.
The evidence supports localized work through the measured 500 MiB tier, not
an unmeasured 100 GiB or other-environment guarantee.

## Complete benchmark and verification coverage

| Family | Cases / controls | Performance samples | Separate verification proofs |
| --- | ---: | ---: | ---: |
| `edit_length_preserving` | 12 | 120, both arms | 24, both arms |
| `edit_length_changing` | 32 | 320, both arms | 64, both arms |
| `edit_canonical_chunk_count` | 12 | 120, both arms | 24, both arms |
| `init_namespace` | 4 | 12, release candidate | 4 |
| `store_footprint` | 3 | 9, release candidate | 3 |
| **Total** | **63** | **581** | **119** |

The edit families cover exact **1/10/100/500 MiB** siblings. Namespace covers
100/1,000/10,000/100,000 files; its 100,000-file, 500 MB subsequent-cache
initialization median is **3.011 s (2.990–3.032 s), 166.0 MB/s**, N=2, with the
first sample reported separately. Store-footprint uses three fresh Stores per
control; its primary durable median is **661,913,600 bytes**, still above the
original 600 MB goal. These supporting results are fresh observations, not a
paired optimization comparison. See [namespace and Store tables](supporting-benchmarks.md).

Final native checks passed **237 tests, 0 failures**, with one pre-existing
ignored test; formatting, warning-denying Clippy and exact-release-source
[GitHub CI](https://github.com/Ephemeral-AI-Lab/layerfs/actions/runs/33814669743)
also passed. Issues [#20](https://github.com/Ephemeral-AI-Lab/layerfs/issues/20)
and [#12](https://github.com/Ephemeral-AI-Lab/layerfs/issues/12) are closed.

## Acceptance, provenance and remaining limits

- Nominal Edit/Commit/combined median targets are **10/10/20 ms**; explicitly
  approved accepted ceilings are **20/20/30 ms**. Three narrow Edit-parity
  exceptions remain disclosed; Commit/combined spreads are diagnostic.
- Memory uses acknowledged-window samples and native lifetime bounds. Sampled
  category maxima do not prove continuous ceilings or exact-phase attribution.
- SDK baseline: `dc7aeff9`; measured SDK candidate: `3337728e`. Supporting-family
  refresh: `e978edd1`, including the later daemon close-order fix. Published tag:
  `d4da2c805745b82449aa6996238bbf86de93650f`. Original evidence is not relabeled
  as a later-source run; selectors below pin full identities and hashes.
- The withdrawn earlier publication and obsolete POSIX/temp-copy benchmarks
  remain archival only, not sources of the SDK optimization claims above.
- [#18](https://github.com/Ephemeral-AI-Lab/layerfs/issues/18) remains **far-future,
  unscheduled alternative-storage exploration**. Physical packs add storage
  design complexity and are not a promised near-term optimization.
- This remains a source-only Developer Preview: no prebuilt executables,
  crates.io packages or runtime images. Keep independent backups; live-process
  acknowledgement does not promise crash/power-loss durability.

## Detailed evidence and documentation

- [Architecture shifts, diagrams and complexity analysis](../../docs/roadmap/0.1/0.1.2/architecture_shift.md)
- [Benchmark results and acceptance scope](benchmark-results.md)
- [Complete SDK edit timing and memory tables](sdk-edit-benchmark-results.md)
- [Refreshed namespace and Store tables](supporting-benchmarks.md)
- [Verification and final gate](verification.md)
- [SDK evidence selector](sdk-edit-evidence.json)
- [Release-refresh evidence index](release-evidence.json)
- [Release contract](release-contract.md)
- [Limitations](limitations.md)
- [Artifact status](artifacts.md)
- [Release announcement](github-release.md)
