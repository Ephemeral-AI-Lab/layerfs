# Issue 266: post-reply snapshot cause diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The first changed-source 512 case at `14859869f` had 512 actual FUSE WRITE
callbacks, functional and cleanup PASS, and no refusal. Its optional status
snapshots ran before the WRITE reply. This result does not identify the old
callback-258 `EBUSY` branch.

This single cause diagnostic restores the historical v1 ordering for the
*optional* snapshot: the originating mutation permit remains live through the
WRITE reply attempt, then the interval-128 snapshot runs, then the permit
drops. The bounded failure records added at `14859869f` remain. No real
mutation, notification, deadline, kernel capability, worker count, workload,
writer, cache policy or verification oracle changes. The image still selects
the interval-128 diagnostic, not the ordinary unsampled gate image. This
changed-source result is a causal diagnostic only and cannot be a latency arm
or replace either the retained #261 512 FAIL or the first #266 512 receipt.

Use the sealed old-head master and an independently writable byte copy; rebuild
only binaries and image whose compilation inputs changed. Make one public
mount → one Exec with the same one-fd 512-write shell command → one explicit
Commit only if Exec succeeds. Retain all output, including FAIL and incomplete
cleanup, and identify the actual FUSE callback count. The specific refusal
record must distinguish projection admission from payload acquisition and
publication; the projection record must expose Ready/Pending/Failed, binding,
mutation/reply permits, revision and remaining deadline. A Ready predecessor
that has finished notification and attempted its WRITE reply, together with a
new sequential writer callback refused while its permit remains held, supports
the reply-gap cause. Any other recorded stage or state redirects the fix.

The independent oracle, if reached, checks exact old/new heads, all bytes and
512 changed runs. Preserve daemon close/retained log, resource charges, Exec,
Commit and complete-command walls. This diagnostic uses the existing 15-second
complete-command bound; it does not change the #249 product Exec timer or the
25-second #248 gate exception. One attempt at this source identity only.
