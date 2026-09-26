# #232 silent Exec progress diagnostics, 2026-09-26

> **Status: Dated planning checkpoint; not release evidence or a product contract.**
> The first attempt below remains FAIL. The timed follow-up is prospective in
> [PLAN_TIMED.md](PLAN_TIMED.md) and has no result here yet.

## First attempt: real mounted callbacks with no caller acknowledgement

The [frozen plan](PLAN.md) selected the registered 10 MiB prepend command
without changing its bytes, worker count, five-second native progress rule or
30-second Exec request. Source `02c6403c98bf06421889aa8c1f1eb31e3c39191b`
had product seal `5816b46f83288b4bc134c254cbf7f5c67585e472a99413c8e6d7add9a5b27d94`,
harness seal `434584eb7ab3db1953fbdb628a44d8c7e2e5012b87b4b6b53c300f380e7016f9`,
and traced release image
`sha256:9ed8a1c61aa29ae0c41abe03592e6c3fca73fb5684fa0e0b95088664cbf6ada8`.
The local `benchmark-results/fs-bench-pro/issue232-exec-progress-freeze-01/FREEZE.json`
pins the exact command, release binary hashes, independently prepared master
Store/history hashes and fresh output. The one attempt and all raw files are
under `benchmark-results/fs-bench-pro/issue232-exec-progress-diagnostic-01/`.
This is an instrumented functional diagnostic, not a new latency arm or a
replacement for either historical FAIL.

The public SDK Exec returned `Failure(Unknown, unknown=true)` after
**5.031934 s** and made no Commit call. Unmount returned `Io`; Sandbox deletion
passed. The complete command took **12.956021 s**, and verification was
`SKIPPED`. The retained daemon log capture attempted and read **132,358 B**
without truncation. It contains **1 SETATTR, 159 READ and 161 WRITE callback
entries** for the mounted file. The last traced WRITE is the 4 KiB insertion
at offset zero. The daemon's own LFT1 `WorkspaceExec` then reports
**9.210890 s, success=true**, showing that daemon execution continued and
completed after the caller's five-second `Unknown`. The earlier historical
receipt remains unchanged.

Raw evidence hashes: `driver.stderr` (callback lines)
`b15b95ce00a23672f8b81805a48c3b8faff0954266d44444e3f56b225dfeeb31`;
`telemetry.lft1` (daemon completion)
`85d598c352797c2ea4305efb12257e83d58c3504bce6f11a8b9b61246c34250a`;
`receipt.json` `3e605acdb6bd5ca88b9d75d49236def7c4295c6d10c8d8f491f1659bc2850c6b`.
The FUSE trace reports callback entry without a timestamp or reply result, so
its count does not prove a particular callback completed near five seconds.
The timed follow-up will locate those entries relative to the caller failure
before any liveness-protocol change is selected. Cache state was uncontrolled;
the latency cell remains `INELIGIBLE`.
