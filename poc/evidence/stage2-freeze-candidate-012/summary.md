# Stage 2 Docker/Linux FUSE — final source-bound result

Status: `PASS_OPTIMIZED`.

The admitted product path is the native Linux route `layerfs-fuse -> MountedWorkspace -> Engine/Core -> Store`. Stage 1.2 remains skipped. The campaign uses no benchmark shim, backing tree, SDK/evaluator bypass, `.bench` recognition, network scenario, tracing asymmetry, emulation, storage-control shortcut, or weakened gate.

## Frozen identity

- Source commit: `88e12ff0268afb380f0f8f44d3ca9d4639be65cc`
- Source tree: `d5f459921e8f8347a83062747e08905ed7bfec21`
- ARM64 image: `sha256:39d13adfb9f2f1a20313d09f23ea1d3be7fcd5535a12eb1afd3a6698b1800fc1`
- Executable BLAKE3: `4e0b899aff1dcd26d494bef941162ee04c60db9646458e496fe5c38915429bf8`
- Upstream `fs-bench.sh` SHA-256: `0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef`
- Authoritative CPU envelope: exact Docker `--cpus 1` CFS quota; no cpuset substitution.

## Correctness and lifecycle

- Exact 100 MiB checkpoint: 362,257,292 ns, with zero checkpoint throttling.
- Same-daemon sequential read: 1,640.43 MiB/s.
- Restart sequential read: 842.26 MiB/s.
- Same-daemon and restart bytes, random ranges, mmap, hard links, portable metadata, shrink/re-extension zeroes, unlink-open semantics, repeated orphan fsync, and namespace absence all pass.
- Dirty pre-ack SIGKILL bytes are discarded; acknowledged fsync bytes reopen exactly; no SIGKILL fabricates a terminal receipt.
- Mounted splice scans/writes exactly the three inserted bytes, reads zero content payload bytes, returns typed remount-required, and reopens exact new bytes.
- Accepted dirty closed unlink and dirty rename-replacement targets drain all ranges/spool ownership at checkpoint.
- Checkpoint, splice, and rollback share Conflict/Incomplete publication failure classification; later mutation is rejected after rollback conflict or ambiguity.
- Terminal mounted ownership is root-only (`lookup_refs=1`, `live_nodes=1`, `inode_mappings=1`) with handles, pending/dirty nodes, dirty ranges, directory changes, spool live/dead/physical, operation Q, and Store connections all zero. Authoritative `logical_workspace_bytes=0`.

## Authoritative performance

Both populations use the exact 12-scenario `SCENARIOS` filter with `REPS=3 WARMUP=1 RANDOMIZE_TARGETS=1`, contain 24 unique rows, and have zero ANSI-stripped FAIL markers and zero network scenarios.

| Control | SL | Rsum | G | Spread | Result |
|---|---:|---:|---:|---:|---|
| `/var/tmp` overlay | 2.999 s | 2.175 | 3.203 | 1.006 | `PASS_OPTIMIZED` |
| `/tmp` tmpfs | 2.993 s | 2.243 | 3.575 | 1.015 | `PASS_OPTIMIZED` |

Every Cloudflare per-row ratio cap passes.

## Resource and publication equations

- CFS throttling ratios: 1.083% (`/var/tmp`) and 1.402% (`/tmp`), both below 5%.
- Cgroup memory peaks: 284,860,416 bytes and 282,300,416 bytes, both below 536,870,912; OOM and OOM-kill deltas are zero.
- Separate low-overhead FD diagnostic: baseline 10, high-water 11, limit 74. Authoritative FD high-water is intentionally `null` with that linked reason so no sampler runs inside timed populations.
- Store connection HWM is 1 and terminal is 0; lookup-reference HWM is 1,202 and terminal is 1.
- Ten-second idle daemon CPU is 0 ns against the 25,000,000 ns limit.
- Each fresh authoritative Store has exactly one genesis publication and zero campaign checkpoints: `publication_commits = genesis(1) + mounted_checkpoints(0)`, with one started/committed transaction and zero rollbacks.
- All task-owned containers, volumes, processes, mounts, and Store/spool journal/scratch residue are absent after cleanup; the immutable image is intentionally retained.

## Invalidated diagnostics retained

Candidate 011’s cpuset populations remain non-authoritative diagnostics. Its exact-quota readiness attempt with a high-frequency in-cgroup shell sampler exceeded the throttle gate (8.60778%) and is preserved. Removing that measurement load—not changing product workers or gates—produced valid exact-quota readiness and final campaigns. The first candidate-012 FD sampler had a PID/RSS parsing failure; the timed populations were unaffected, and the rerun produced the retained FD result above.

`SHA256SUMS` is generated only after every other retained artifact is final.
