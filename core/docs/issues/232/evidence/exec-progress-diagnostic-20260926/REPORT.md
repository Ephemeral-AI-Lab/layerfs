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

## Timed follow-up: callbacks continue beyond the caller failure

The planned `-02` command stopped before its measured child because the v2
runner tried to recreate the first attempt's append-only case directory. It
left `build.json` and `cursor-key.txt` in the fresh outer output, but no Exec
attempt or result receipt. Source `649f6009088dc53c8cd6591faa9a17bc8dfa17b6`
routed v2 case files into their own fresh output directory. Its harness seal
was `0cf78d892e8a0e8eb5ef0131a19f03ae5bbfff4a9bf295120876bac516e49d73`;
product seal remained `4f42832c0cd431098b9218ba49509692fde749cd713d3c91e39413a9be268f7c`.
The local `issue232-exec-progress-timed-freeze-03/FREEZE.json` pins the exact
command, release binary hashes, release Linux image
`sha256:a2c17b3b2ed771e28e6a336e30e07e0881e8bec38c67c95a7e9f13804b2294c9`,
and a newly prepared, independently copied 10 MiB master. The one timed
attempt is in `issue232-exec-progress-timed-diagnostic-03/`.

The public SDK Exec again returned `Unknown` after **5.030130 s**, with no
Commit, unmount `Io` and Sandbox deletion PASS. The complete command took
**11.002276 s**; verification was `SKIPPED`. The retained trace has **1
SETATTR, 160 READ and 161 WRITE callback entries**. It records **78 callback
entries after 5.000 s** from its first entry; the last WRITE at offset zero
entered at **8.472533 s**. The largest gap between callback entries was
**0.092858 s**. The daemon's `WorkspaceExec` scope lasted **8.574280 s** and
reported success. These are callback *entries*, not proof of individual replies;
combined with the completed Exec scope, they show real mounted work continued
well after the caller stopped receiving control-channel progress. Cache state
was uncontrolled, so no latency admission follows.

Raw SHA-256: `driver.stderr`
`0e8e3c98430765dc4eb12028f05e76904c055434e949669ea7fb04c39ccec0d3`;
`telemetry.lft1`
`63f8e9757ff9a54b5b9a7569c150bacf2a5000c6137874f74141f0c70f7a3121`;
`receipt.json`
`adcd87dc7a237017e18bd8f7c74e414b1880bf6635d3a284a4ff485630d1ebc8`.

The product correction will send bounded authenticated Exec progress only
after an accepted Workspace revision advances. A silent `sleep 6` control
must still hit the five-second no-progress boundary. The new source needs a
separate functional attempt and independent verification; neither diagnostic
above is replaced.
