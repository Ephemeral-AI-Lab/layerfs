# Fast verification: first qualified result

Source: `7948df2de269e5ffd47a232ffd8091ff83f8869f`. This is a development
verification profile, not a replacement for the exhaustive Phase 1 gate.

The representative `tiny-stat-1`, seed 1, passed the separate fast verifier
using an existing qualified input and its retained full-proof certificate.
Independent result validation reported no issues or resource violations;
supervisor and mutable-sample cleanup both passed.

| Observation | Retained exhaustive reference | Fast profile |
| --- | ---: | ---: |
| Runtime process | 170.565 s | 6.916 s |
| Preparation | 6.242 s | 6.645 s |
| Complete sample, including preparation and cleanup | 177.173 s | 13.895 s |
| Fast CLI invocation | — | 15.862 s |

The observed complete-sample time is 12.75× lower; runtime is 24.66× lower.
These are comparisons of retained observations, not a controlled product
performance claim. The profiles perform different verification coverage, and
earlier ambient activity was not established as identical. Both records bind
the same Docker VM environment identity, with eight VM CPUs and the unchanged
two-CPU/two-GiB benchmark-container limits.

## Actual coverage

- Authenticated the current namespace and all 101,144 global inode records,
  checking namespace membership, metadata and alias relationships.
- Checked all 101,144 native namespace names and types after a fresh FUSE
  mount.
- Read 60 selected regular paths, totaling 500,131 bytes, and checked 209
  selected native metadata paths. This read-only representative has no changed
  or deleted paths; the separate negative qualification covers changed-data
  and absence failures.
- Bound unchanged content references to the qualified full certificate.
- Explicitly skipped reading 100,440 untouched regular bodies, totaling
  524,612,319 bytes, and skipped 100,935 untouched native metadata checks.

The result is `fast_iteration_verified`, with `fully_verified=false` and
`verification_pass=false`. It does not establish present availability or
integrity of every skipped stored content object, or correct FUSE readback at
every unvisited pathname. Root/reference equality is not a claim that those
bytes were reread. The full canonical census was not performed.

## Qualification and retained evidence

The small Python certificate/report model passed in 0.079 seconds. The first
host aggregate failed because a batch API stripped the canonical envelope
before inode decoding. That failure is retained. After using the authenticated
canonical-object API, the affected host aggregate passed in 1.711 seconds,
including nine canonical/certificate negatives and eight native negatives.
The passing Python model was not rerun.

Evidence is under `benchmark-results/fs-bench-pro/phase1-v013/`:

- Fast attempt: `attempts/tiny-stat-1-s1-fast-verify-4173c0ddecb8/`.
- Exhaustive certificate/reference:
  `attempts/tiny-stat-1-s1-verify-409ab5f7f943/`, source `f5f8a698`.
- Independent fast-result validation:
  `qualification/fast-verification/representative-7948df2d-validation.json`.
- Small qualification results: `qualification/fast-verification/`.

All earlier full proofs remain intact. Fast checks contribute no missing full
slots. The [VM8 admission cohort](vm8-admission-cohort.md) separately resolves
the user-authorized VM change while preserving the original VM4 observations.
