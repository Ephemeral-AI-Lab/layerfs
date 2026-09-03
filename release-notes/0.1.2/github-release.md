# LayerFS 0.1.2 Developer Preview

LayerFS 0.1.2 adds universal owner-side regular-file range editing and reports
complete SDK-only edit benchmarks across 1, 10, 100 and 500 MiB files.

## Highlights

- One SDK range-edit operation covers overwrite, append, prepend, insertion,
  deletion, growth, shrinkage, truncation and zero extension.
- Localized edits reuse unchanged content through the shared piece-tree engine;
  FUSE presentation refresh invalidates the edited inode without remounting.
- A daemon close/remount race is fixed: watcher and mount reservations retire
  before close acknowledgement, without sleeps or retries.
- Three complete edit families: 56 cases, 560 baseline/candidate performance
  samples and 112 independent source-arm verification proofs.
- Fresh namespace initialization and Store-footprint measurements complete the
  five-family release inventory. Performance and full verification are separate.

## SDK prepend results

One 4 KiB prepend, five candidate samples per size. Times are medians in ms.
These source-bound measurements are from the completed SDK campaign, not the
obsolete full-file temp-copy workload.

| File | Edit | Commit | Combined |
| --- | ---: | ---: | ---: |
| 1 MiB | 1.674 | 2.919 | 4.680 |
| 10 MiB | 2.164 | 2.779 | 4.883 |
| 100 MiB | 2.967 | 4.289 | 7.257 |
| 500 MiB | 3.344 | 11.122 | 14.300 |

See the [benchmark report](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.2/release-notes/0.1.2/benchmark-results.md)
for all operations, N, median/min–max tables, source identities, accepted
20/20/30 ms ceilings, three narrowly approved Edit-parity exceptions, and
memory-observation limitations. Combined medians are calculated independently.
The claim is localized, size-stable edits with bounded Commit latency—not
size-independent Commit or universal hardware/OS performance.

## Compatibility and limitations

This is a source-only Developer Preview. It retains the current SQLite Store
and canonical identity format; use matching SDK and daemon versions. No
executables, crates.io packages, or runtime images are published. Keep independent
backups: live-process acknowledgement is not a crash/power-loss durability promise.

The compatible Store remains above the original primary footprint goal.
Alternative storage designs, including authenticated physical packs, are
far-future, unscheduled exploration in #18—not a near-term optimization promise.

Memory tables distinguish sampled observations from native lifetime peaks;
they do not prove continuous category ceilings or exact edit-phase attribution.
Historical failed attempts remain preserved. The original SDK evidence and
the later daemon-fix/release-refresh source are identified separately.

## Assets

Source archives, `Cargo.lock`, `LICENSE`, and `SHA256SUMS` accompany this release.
Read the [manual](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.2/docs/versioned/0.1.2/README.md)
and [verification record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.2/release-notes/0.1.2/verification.md).
