# Unmount follow-up: one source-changed case

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Frozen before the follow-up live attempt; the original failure stays in
> [attempt-01](attempt-01/) and [its failure note](ATTEMPT-01-FAIL.md).

Only `PROBE_CASE=unmount` is selected. The source now issues Linux
`umount2(MNT_DETACH)` while the staged file descriptor is open, checks the
background FUSE session exits, then discards its test-only stage on shutdown.
This directly tests the cleanup event that ordinary unmount could not create
while the descriptor was open. The follow-up must retain the mountinfo line,
callback/frame log, caller event, final `stage_present=false` and
`publications=0`, exit status and container/volume cleanup. An `EBUSY` or
missing final record is a failure, never rewritten as GO.

Probe source SHA-256
`b96f3b5d4c310e2a767f822d873d6de29973a8dae1903143e9479e6763f2c2fe`;
locked release binary `range_ioctl_staging_probe-f90289f2e0dce2b6` SHA-256
`a6a9696e9327de71580c43bae50d7c0262857a6c289b929d4b05826445543145`.
Published `fuser` 0.18.0, image
`sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8`
and the locked release build command from [CONTRACT.md](CONTRACT.md) remain
the same. The new source commit is recorded in the follow-up receipt after
this checkpoint is committed.
