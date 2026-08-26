# Stage 2 candidate 015 provisional evidence index

Status: `PASS_LOCAL_ONLY`.

This is the local-only custody selected by the user. Product source is
`7e82abcd7320f6a214be336d82488ba0527b6025`, tree
`df13d88eb7e7d2471971b0c58ca6425bb81b0b03`, and ARM64 image
`sha256:f8647b84580c75d4688a18665e4c60cd6dcf5b2d3092cf22bce34dfbd86b59b0`.
Top-level `SHA256SUMS` was generated last after the local comparison and local
restart-durability verifier passed. Cloud deployment and Durable Object sync
are explicitly outside this custody scope.

## Controlling receipts

- [`summary.json`](summary.json): current machine-readable disposition and metrics.
- [`durable/durable04`](durable/durable04): 48/48 fresh-Store durable samples.
- [`durable/verification05.json`](durable/verification05.json): independent `PASS_DURABLE` verification.
- [`live-current/verification.json`](live-current/verification.json): current-source `/var/tmp` and `/tmp` `PASS_LIVE_MOUNT` populations.
- [`upstream-scenario-map.json`](upstream-scenario-map.json): independently derived exact 12-scenario mapping; zero network scenarios.
- [`local-comparison.json`](local-comparison.json): matched native-ARM64, one-CPU, 512 MiB local live comparison and restart-durability disposition.

## Current-source focused proofs

- [`immediate-term`](immediate-term): signal readiness and clean terminal ownership.
- [`repeated-term`](repeated-term): repeated signals during a dirty 100 MiB checkpoint.
- [`unmount-busy`](unmount-busy): bounded fail-closed `EBUSY` unmount.
- [`statvfs`](statvfs): initial, dirty, checkpointed, and cleaned capacity states.
- [`focused/current-external-unmount-success`](focused/current-external-unmount-success): dirty successful external unmount, exactly one checkpoint/publication, exact Verified reopen.
- [`focused/current-crash-metadata`](focused/current-crash-metadata): metadata-heavy post-ack SIGKILL and exact reopen.
- [`focused/current-crash-payload`](focused/current-crash-payload): high-entropy 64 MiB post-ack SIGKILL and exact bytes after reopen.

Failed harness/verifier attempts are retained inside their owning evidence
directories and are not promoted over the qualifying receipts.

## Local Cloudflare comparison boundary

- [`cloudflare-local-512`](cloudflare-local-512): two matched local native-FUSE live populations.
- [`cloudflare-local-restart/receipt.json`](cloudflare-local-restart/receipt.json): acknowledged 64 MiB state is absent after fresh-container restart, so the local Cloudflare state is not restart-durable.
- [`cloudflare-admission.json`](cloudflare-admission.json) and [`cloudflare-deploy-readiness.json`](cloudflare-deploy-readiness.json) are retained historical receipts; deployment is out of scope.

There are zero deployed Cloudflare samples. The comparison covers only local
native FUSE and process-local SQLite. LayerFS passes the local restart test;
Cloudflare's local state does not. This does not claim anything about a
deployed Durable Object synchronization path.
