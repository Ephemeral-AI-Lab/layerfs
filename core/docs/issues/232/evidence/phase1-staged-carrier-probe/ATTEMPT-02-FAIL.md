# Staged carrier attempt 02: retained shutdown wait

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The source-changed `unmount` case at
`d7a514487dc674e2663ca131e80189119e1188ef` produced the append-only
[attempt-02](attempt-02/) receipt. Linux `umount2(MNT_DETACH)` succeeded
while the staged descriptor remained open; the mount disappeared from the
container mount table. The test then waited for its child, while the child
waited in `fuser::BackgroundSession::join()` with that descriptor still open.
The Docker Exec supervision timed out after 30.005 seconds. No final stage
cleanup or no-publication proof was emitted. The still-running test processes
were stopped by removing the owned container and volume. **This is another
failed probe case**, not a GO or a product failure.

The wait dependency is explicit in the test: parent `child.wait()` preceded
`drop(file)`, and child `bg.join()` preceded the test-only stage disposal.
The next source identity will dispose the stage on daemon unmount/shutdown,
emit an observable cleanup marker while the caller descriptor remains open,
then let the caller close it so the FUSE session can finish. Only `unmount`
will be attempted; no earlier case is repeated.
