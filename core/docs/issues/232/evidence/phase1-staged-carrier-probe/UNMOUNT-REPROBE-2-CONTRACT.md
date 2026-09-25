# Unmount follow-up 2: stage disposal before session join

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Frozen before this source-changed `unmount` attempt. The two prior failures
> remain in [attempt-01](attempt-01/) and [attempt-02](attempt-02/).

After Linux `umount2(MNT_DETACH)` succeeds with the staged descriptor still
open, the test daemon must discard the stage and write
`unmount-stage-cleared` **before** joining its FUSE thread. The caller must
observe that marker before it closes the descriptor. Only then may the child
finish its join. The raw event log must show zero publications, no retained
stage, the mounted-fs identity and a confirmed absent mount after the case.
Any missing marker, timeout or nonzero exit is a retained failure; this case
gets one attempt at the new source identity.

Probe source SHA-256
`7c283f3d5664fb3c418fedc6b70f7b601af4553a437f389bf3b42de27bf5e744`;
locked release binary `range_ioctl_staging_probe-f90289f2e0dce2b6` SHA-256
`ccebe58336953778b5c730cd938da1e79738f9453ed56548ff762d5cf56c2b88`.
The published `fuser` 0.18.0, image, kernel and locked build command are as
in [CONTRACT.md](CONTRACT.md). The new commit identity goes in the run receipt.
