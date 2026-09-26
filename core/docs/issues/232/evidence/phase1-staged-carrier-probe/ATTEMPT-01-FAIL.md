# Staged carrier attempt 01: retained unmount failure

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Probe source `d75d4a9a569987ff95d131c15166cc4ad5028497` and the exact
[contract](CONTRACT.md) produced the append-only [attempt-01](attempt-01/)
records. Eight named cases exited zero. `unmount` exited 101 after its BEGIN
callback: `fuser::BackgroundSession::umount_and_join()` returned Linux `EBUSY`
(16) while the staged file descriptor remained open. The test panicked before
its final cleanup log. This is a **failed probe case**, not a provider NO-GO
and not a successful stage cleanup proof. The mount remained active inside
the test container until it was detached with `umount -l`; the container and
owned volume were then deleted. No product or benchmark result used the mount.

The cause is visible at the exact failing test source line and in published
`fuser` 0.18.0: `umount_and_join` calls the mount's ordinary unmount before
joining the background thread. An open descriptor prevents that unmount.
The probe's intended case is an **actual forced detach with that descriptor
still open**, followed by daemon shutdown and stage disposal. A source-changed
follow-up will run only `unmount` once, in a fresh mount and output directory;
the eight passing cases are retained rather than repeated. That follow-up
also captures mountinfo, which attempt 01 did not retain despite the planned
custody field. Attempt 01 therefore cannot be a complete all-case GO verdict.
