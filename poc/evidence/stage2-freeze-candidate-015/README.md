# Stage 2 candidate 015 provisional evidence index

Status: `PASS_LOCAL_ONLY`.

This is the local-only custody selected by the user. Product source is
`7e82abcd7320f6a214be336d82488ba0527b6025`, tree
`df13d88eb7e7d2471971b0c58ca6425bb81b0b03`, and ARM64 image
`sha256:f8647b84580c75d4688a18665e4c60cd6dcf5b2d3092cf22bce34dfbd86b59b0`.
The prior top-level `SHA256SUMS` is retained under `invalidated-seal-887facd`
because its Cloudflare restart label was too broad. The corrected top-level
manifest is generated only after the local comparison and restart verifier
pass. Cloud deployment and Durable Object sync are explicitly outside this
custody scope.

## Controlling receipts

- [`summary.json`](summary.json): current machine-readable disposition and metrics.
- [`durable/durable04`](durable/durable04): 48/48 fresh-Store durable samples.
- [`durable/verification05.json`](durable/verification05.json): independent `PASS_DURABLE` verification.
- [`live-current/verification.json`](live-current/verification.json): current-source `/var/tmp` and `/tmp` `PASS_LIVE_MOUNT` populations.
- [`upstream-scenario-map.json`](upstream-scenario-map.json): independently derived exact 12-scenario mapping; zero network scenarios.
- [`local-comparison.json`](local-comparison.json): matched native-ARM64, one-CPU, 512 MiB local live comparison and restart-durability disposition.

## Current-source focused proofs

- [`focused/current-immediate-term-custody`](focused/current-immediate-term-custody): exact readiness-to-TERM command, timing, terminal, inspection, and cleanup custody; 17/17 checks pass.
- [`focused/current-repeated-term-custody`](focused/current-repeated-term-custody): two TERM deliveries during a dirty 100 MiB shutdown, exact Verified reopen, and cleanup custody; 22/22 checks pass.
- [`focused/current-unmount-busy-custody-attempt-002`](focused/current-unmount-busy-custody-attempt-002): controlling bounded fail-closed `EBUSY` race with exact command/timing/cleanup custody; 16/16 checks pass.
- [`focused/current-unmount-busy-custody`](focused/current-unmount-busy-custody): preserved failed race attempt; Docker rejected the competing exec before it could return its own `EBUSY`.
- [`immediate-term`](immediate-term), [`repeated-term`](repeated-term), and [`unmount-busy`](unmount-busy): historical product receipts with explicit fail-closed notices that their original execution custody was incomplete.
- [`statvfs`](statvfs): initial, dirty, checkpointed, and cleaned capacity states.
- [`focused/current-external-unmount-success`](focused/current-external-unmount-success): dirty successful external unmount, exactly one checkpoint/publication, exact Verified reopen.
- [`focused/current-crash-metadata`](focused/current-crash-metadata): metadata-heavy post-ack SIGKILL and exact reopen.
- [`focused/current-crash-payload`](focused/current-crash-payload): high-entropy 64 MiB post-ack SIGKILL and exact bytes after reopen.

Failed harness/verifier attempts are retained inside their owning evidence
directories and are not promoted over the qualifying receipts.

## Local Cloudflare comparison boundary

- [`cloudflare-local-512`](cloudflare-local-512): two matched local native-FUSE live populations.
- [`cloudflare-local-same-container-restart/receipt.json`](cloudflare-local-same-container-restart/receipt.json): same-container negative control proving the expected loss of standalone process-local SQLite state.
- [`cloudflare-local-authority-volume-durable/receipt.json`](cloudflare-local-authority-volume-durable/receipt.json): controlling shipped pull/reconcile/push proof; a Docker-local named-volume SQLite authority rehydrates the exact 64 MiB payload after SIGKILL.
- [`cloudflare-local-authority-durable/receipt.json`](cloudflare-local-authority-durable/receipt.json): superseded passing host-bind authority diagnostic; retained because its persistence medium did not match LayerFS's Docker-volume class.
- [`cloudflare-local-restart/receipt.json`](cloudflare-local-restart/receipt.json): historical fresh-container observation, superseded because container removal discarded the writable layer before reopen.
- [`cloudflare-admission.json`](cloudflare-admission.json) and [`cloudflare-deploy-readiness.json`](cloudflare-deploy-readiness.json) are retained historical receipts; deployment is out of scope.

There are zero deployed Cloudflare samples. The comparison covers only local
native FUSE. LayerFS uses its production Store; the positive Cloudflare control
uses the shipped sync path plus a harness file-SQLite authority, while standalone
`computerd` remains process-local. This does not claim anything about deployed
Durable Object synchronization or compare persistence latency across the two
different authority contracts.
