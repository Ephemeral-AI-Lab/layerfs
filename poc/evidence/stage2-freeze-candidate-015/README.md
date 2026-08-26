# Stage 2 candidate 015 provisional evidence index

Status: `PASS_DURABLE_LAYERFS + PASS_LIVE_MOUNT_DIAGNOSTIC + CLOUDFLARE_COMPARISON_BLOCKED_EXTERNAL + REVISE`.

This is provisional custody, not the terminal seal. Product source is
`7e82abcd7320f6a214be336d82488ba0527b6025`, tree
`df13d88eb7e7d2471971b0c58ca6425bb81b0b03`, and ARM64 image
`sha256:f8647b84580c75d4688a18665e4c60cd6dcf5b2d3092cf22bce34dfbd86b59b0`.
Top-level `SHA256SUMS` remains intentionally absent until the deployed matched
Cloudflare comparison is available and the terminal verifier passes.

## Controlling receipts

- [`summary.json`](summary.json): current machine-readable disposition and metrics.
- [`durable/durable04`](durable/durable04): 48/48 fresh-Store durable samples.
- [`durable/verification05.json`](durable/verification05.json): independent `PASS_DURABLE` verification.
- [`live-current/verification.json`](live-current/verification.json): current-source `/var/tmp` and `/tmp` `PASS_LIVE_MOUNT` populations.
- [`upstream-scenario-map.json`](upstream-scenario-map.json): independently derived exact 12-scenario mapping; zero network scenarios.

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

## Cloudflare comparison boundary

- [`cloudflare-admission.json`](cloudflare-admission.json): authenticated Containers entitlement blocker.
- [`cloudflare-deploy-readiness.json`](cloudflare-deploy-readiness.json): locally validated wrapper commit `151b053b514e7bd0eb4b64481fe89335c43e7109`, exact computerd hash, validation results, and null platform metrics.

There are zero deployed Cloudflare timed samples. Local Wrangler bundling,
source-built computerd, and local Docker build attempts are deploy-readiness
only and are not performance or durability evidence.
