# Stage 2 candidate 014 — PASS_DURABLE_LAYERFS_COMPARISON_UNAVAILABLE

Product source `292be840c31052d85ab6e9441706298af3cd3d15` / tree `e3055bcd7a41921879fa149c11918891517e4522` and ARM64 image `sha256:62b459af3f03dc8bbe97419b8522ed3599ab6d562b12ebe8b8ed5efb7f22f5fc` pass correctness, resource, and live-mount diagnostics. Persistence-inclusive terminal timing is separately controlled.

- `/var/tmp`: SL 3.517 s, Rsum 2.040, G 3.207, Spread 1.041.
- `/tmp`: SL 3.449 s, Rsum 2.126, G 3.691, Spread 1.051.
- Daemon settled RSS 5152 KiB; HWM 15092 KiB; threads HWM 7; FD HWM 11.
- The upstream matrices are `LIVE_MOUNT` diagnostics. Their Cloudflare thresholds do not control restart-durable performance.
- Persistence-inclusive campaign: PASS_DURABLE.
- Durable median sums: live 3.524 s, checkpoint 4.307 s, command-to-durable 7.854 s.
- Full-product Cloudflare comparison: unavailable without deployed Durable Object sync timing and restart authority.
- No benchmark shim, backing tree, SDK/evaluator bypass, workload recognition, network row, tracing asymmetry, emulation, or storage-control shortcut was found. Stage 1.2 remained skipped.
