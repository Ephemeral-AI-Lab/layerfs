# #232 silent Exec progress diagnostics, 2026-09-26

> **Status: Dated planning checkpoint; not release evidence or a product contract.**
> The first and timed attempts below remain FAIL. The progress correction and
> subsequent Commit-failure diagnostic do not qualify the registered case.

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
The timed follow-up below locates those entries relative to the caller failure.
Cache state was uncontrolled; the latency cell remains `INELIGIBLE`.

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

## Release product correction: Exec passes its old silence boundary

Commit `563c104dd1a628cd4f345798fb38c5ff19208546` added an authenticated
Exec progress record only when the mounted Workspace's accepted revision
advances, at most once per second. It kept the five-second native silence rule,
30-second Exec deadline and bounded frame count. The client accepts only the
exact one-byte marker for Exec, without treating it as result data. The release
native-connection test passed. The full locked Core release tests, warning-denying
release Clippy, formatting, boundary guard and its nine self-tests passed after
the source correction (the first full test build needed the existing bridge
test to import `Write`; the first Clippy run required a scoped argument-count
expectation).

The local `issue232-exec-progress-product-freeze-01/FREEZE.json` records the
clean source, release host driver hashes, release aarch64 Linux daemon image
`sha256:d3c54f9546cfaa77716593c910c6658025fe51d05b714bff5c31287932da0811`,
new independent master and exact registered command. Its one attempt is under
`issue232-exec-progress-product-01/`. The SDK Exec child reached Commit after
**8.656598 s**; the Commit child lasted **0.117568 s**. This demonstrates that
the caller passed the former five-second silence failure at this source. The
Service's retained LFT1 records an `EditFile` failure after **0.064555 s**.
Cleanup then ran until the complete command hit **15.008113 s**, so the harness
stopped it before the driver printed a final receipt. The row is **FAIL** with
`sample_count=0`, verifier `SKIPPED`, and no confirmed cleanup; the performance
and cache gates remain ineligible. Raw SHA-256: `run.json`
`7bea7de962390e2dacb7214926365c0839baa457e632829c5ae93d62e7f3154e`;
`receipt.json`
`d723b1c8a9b899c47dde16f1c6ec4d12923e0c020174ca1b21a36baea413d148`;
`telemetry.lft1`
`144a15946ad7033ab3fa174940e0ad5670e37972053bcc036c838af133c22817`.

At that same release product image, the opt-in public SDK `sleep 6` control ran
once with a release test executable. It returned typed `Unknown` after
**5.043494 s**, made no mutation, and confirmed Sandbox deletion and absence.
Its `result.tsv` reports `PASS_DIAGNOSTIC`; SHA-256
`bcbc72399c00d4760070113f2703750b9ec93952b5328250c39cd020f3c68630`.
This checks that an unchanged Workspace revision does not generate a progress
record. It is a functional control, not a performance sample.

## Commit cause diagnostic: preparing failure is known

The [prospective plan](PLAN_OUTCOME.md) added an opt-in pre-cleanup result line
to the release driver at source `35b5876d97f9207f6ff2b489932d1cd91b41e4aa`.
The source has product seal
`fd1bd14d83ab0b398dd263114d6f02807b5cea7b8fcdb327f10ba5a65fd75da6`
and unchanged harness seal
`0cf78d892e8a0e8eb5ef0131a19f03ae5bbfff4a9bf295120876bac516e49d73`.
The frozen local `issue232-exec-outcome-freeze-01/FREEZE.json` pins release
host driver hashes, a new independent master, and release Linux image
`sha256:81bcd35aa877a8e249c162700c343ef187d0431b28df3073c3145885084c6a90`.
The one labelled attempt in `issue232-exec-outcome-diagnostic-01/` retained:

```text
commit failed: Commit(WorkspaceCommitFailureWire {
  generation: 1, phase: Preparing, disposition: KnownBeforeCommit,
  cause: Failure { code: Io, unknown: false, cleanup: None, history: None },
  known_stage: None, observed_stage: None, known_outcome: None,
  observed_outcome: None, installed_revision: None
})
```

The same fixed 15-second command wall ended this diagnostic during cleanup:
**15.009389 s**, `sample_count=0`, no driver receipt, verifier `SKIPPED`,
unconfirmed cleanup, status **FAIL**. No Docker container with that case name
remained afterward, which is only an observation and does not promote cleanup
to PASS. The exact internal cause of the Service `Io` has not been isolated.
Raw SHA-256: `driver.stderr`
`9a554f0b672ff7d348912a2904f94c8b365ad24f834b3ef6c03daff3058a6112`;
`receipt.json`
`cd088a6a230e6cd3f16b86fa0f7804473c8ec4285716ef86a982423f809c412a`;
`telemetry.lft1`
`ee8618e16ee9d624b359c3579993e6af34fea1021a04c4d7ec2dee17688b068d`.

The five-second `Unknown` is resolved for this mounted mutation at release
profile. The registered 10 MiB v2 row remains blocked by a known-before-Commit
`Io` in preparation, plus missing verifier and cleanup proof. Neither source
identity supplies release admission or a speed claim.
