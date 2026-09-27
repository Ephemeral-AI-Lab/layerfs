# Issue 266: healthy WRITE reply permit wait

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The [post-reply cause diagnostic](issue266-post-reply-diagnostic-spec.md) at
`4006d9893` retained a FAIL at callback 129 after 128 accepted writes. Its
failure-only record names `begin_projection_mutation`: revision 128,
`Ready`, bound delivery, `mutation_held=true`, one reply permit and nearly the
full 10-second callback deadline remaining. The generic one-fd writer could
start callback 129 only after callback 128's WRITE reply attempt. This proves
that the optional post-reply snapshot held a healthy origin permit across the
next sequential WRITE. It explains a class of failure compatible with the old
callback-258 FAIL; the old receipt itself did not log its refusal branch.

The treatment waits on a condition signal only when the prior projected WRITE
has entered its reply attempt and holds the sole mutation/reply permit with
`Ready` coherence and a bound notifier. Admission is rechecked under the same
state lock after waking; it cannot pass a stale revision or failed binding.
The wait ends at the original callback deadline, never a new timeout. An old
observation permit, an active mutation that has not reached reply, an unbound
or failed notification, and other invalid states retain immediate Busy.
Release of the prior permit signals waiters. The originating permit remains
held through reply and optional status snapshot. No operation or notification
is retried, no worker is added, and the data publication path is unchanged.

At the frozen treatment source, take one 512-write attempt with the same
post-reply interval-128 diagnostic image, prepared old-head master, one-fd
writer, public mount/Exec/Commit route and independent oracle. This directly
tests the observed failure mechanism; it is diagnostic, never a latency
comparison. Retain all FAIL/INELIGIBLE evidence. Check 512 actual FUSE WRITE
callbacks, exact old/new heads, all bytes, 512 changed runs, 1,024 extents,
resource charges and daemon's positive close or retained outcome. Focused
tests under 30 seconds cover a waiting worker, active/unbound/failed immediate
refusal, publication/read-your-writes, and clean close after Commit.

If the 512 treatment and independent proof pass, use the ordinary unsampled
image for one #248 4,097 public Exec/Commit gate at that same frozen source.
Keep its 25-second complete-command exception, the #249 product Exec timer,
one construction worker and uncontrolled cache contract unchanged. Record
every result and exact identities; a functional PASS with uncontrolled cache
is still latency `INELIGIBLE`.
