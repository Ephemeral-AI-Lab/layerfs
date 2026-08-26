# Stage 2 local persistence-aware comparison 016

Status: `REVISE_RESOURCE_THROTTLE_AND_ENVELOPE_MISMATCH`.

The evidence itself verifies `PASS_EVIDENCE`: all 48 Cloudflare samples and all
36 measured LayerFS samples pass their product correctness, physical durability,
restart, exact-manifest, source/image, daemon-CPU, memory, OOM, and cleanup
checks. The cross-product timing comparison remains diagnostic because:

1. Cloudflare's measured-population CFS throttle ratio is `10.307112%`, above
   the preregistered `5%` limit; and
2. Cloudflare used a 512 MiB hard memory limit, while the retained LayerFS
   `durable04` population used a 3 GiB hard limit. LayerFS's actual peak stayed
   below 512 MiB, but that does not make the enforced envelopes identical.

No threshold was weakened and no unchanged population was rerun after these
facts were known.

## Command-to-durable medians

Each cell is the median of three measured samples after one warmup. `Live` is
the exact upstream scenario command. `Persist` is LayerFS checkpoint or
Cloudflare `pullOnce` plus named-volume SQLite WAL checkpoint/database fsync/
directory fsync. `Total = Live + Persist` exactly.

| Scenario | LayerFS live | LayerFS persist | LayerFS total | Cloudflare live | Cloudflare persist | Cloudflare total | CF/LF total diagnostic |
|---|---:|---:|---:|---:|---:|---:|---:|
| Create 1,000 files | 342.748 ms | 880.603 ms | 1,224.234 ms | 988.430 ms | 3,114.328 ms | 4,093.846 ms | 3.344x |
| Stat 1,000 files | 886.267 ms | 885.801 ms | 1,772.069 ms | 1,632.255 ms | 2,863.896 ms | 4,496.764 ms | 2.538x |
| Remove 1,000 files | 502.078 ms | 0.074 ms | 502.135 ms | 1,301.976 ms | 464.018 ms | 1,748.984 ms | 3.483x |
| Make directory tree | 713.599 ms | 687.773 ms | 1,397.432 ms | 1,359.225 ms | 1,637.846 ms | 2,994.614 ms | 2.143x |
| Find directory tree | 736.050 ms | 688.933 ms | 1,423.818 ms | 1,578.717 ms | 1,674.410 ms | 3,276.460 ms | 2.301x |
| Write 64 MiB | 49.394 ms | 162.068 ms | 211.462 ms | 136.525 ms | 79.846 ms | 223.967 ms | 1.059x |
| Copy 64 MiB | 124.545 ms | 299.413 ms | 424.053 ms | 454.319 ms | 43.447 ms | 496.833 ms | 1.172x |
| Read 64 MiB | 53.706 ms | 161.664 ms | 215.865 ms | 246.779 ms | 79.584 ms | 319.294 ms | 1.479x |
| Pure read 64 MiB | 106.645 ms | 0.061 ms | 106.715 ms | 12.164 ms | 24.844 ms | 47.874 ms | 0.449x |
| Pure copy 64 MiB | 171.741 ms | 162.521 ms | 334.399 ms | 219.914 ms | 35.066 ms | 255.310 ms | 0.763x |
| Overwrite 64 MiB | 50.088 ms | 161.636 ms | 212.682 ms | 127.666 ms | 61.966 ms | 188.077 ms | 0.884x |
| Git init + commit 100 files | 160.774 ms | 246.365 ms | 403.974 ms | 556.747 ms | 820.293 ms | 1,395.461 ms | 3.454x |
| **Sum of medians** | **3,897.635 ms** | **4,336.913 ms** | **8,228.839 ms** | **8,614.718 ms** | **10,899.544 ms** | **19,537.483 ms** | **2.374x diagnostic** |

The numerical CF/LF column is evidence-retained diagnostic output, not an
authoritative fair-comparison claim. LayerFS's own durable timings remain valid
for LayerFS; the Cloudflare timings are complete but population-resource-invalid.

## Evidence boundary

- LayerFS: production Store, source `7e82abcd7320f6a214be336d82488ba0527b6025`.
- Cloudflare: shipped pull/reconcile/push path with a disclosed Docker-local
  named-volume SQLite authority; no Durable Object or deployment claim.
- Every Cloudflare sample performs a pre-timed durable prep/restart/rehydration,
  a command-to-authority durability timer, post-ack SIGKILL, same-container
  restart, fresh authority reopen, reconcile/push, exact native-FUSE manifest,
  acknowledged cleanup, and zero container/volume residue.
- `cloudflare-population` and `cloudflare-population-attempt-002` are preserved
  failed harness attempts. `cloudflare-population-attempt-003` is the complete
  48-row population whose aggregate resource gate fails.

The machine-readable result is [`comparison.json`](comparison.json); the
independent synthesizer is [`verify.py`](verify.py).
