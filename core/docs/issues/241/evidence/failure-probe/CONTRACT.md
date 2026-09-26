# #241 mounted failure probe: frozen cases

Source base: `3ef5824f01ceb51a326f9bfb4f77ace35cb657e7`. This is a
test-only carrier diagnostic using published `fuser` 0.18.0 on Linux FUSE.
It is not a Workspace mutation, Commit, latency, or cold-cache claim. Every
case gets one attempt and a distinct raw directory; failures stay visible.

The input is the prior provisional 4,128-byte `LFR1` request, command
`0x5020f541`, with a 4 KiB insertion at offset 4,093. The fixture is a
virtual 8,192-byte regular file. The daemon records the exact callback request
and publication/reply decision. The test does not claim to capture raw FUSE
wire reply frames; caller errno and the absence of a reply are observed at
the syscall boundary.

| Case | Action | Expected observation | Limit |
| --- | --- | --- | --- |
| `ro_mount` | Mount FUSE `ro`; open the file read-only; send `_IOW` ioctl | Actual `ro` mountinfo; either kernel refuses before callback or daemon detects mounted RO and returns `EROFS` before mutation; size/mtime unchanged | A daemon-side refusal proves the required adapter check, not automatic kernel protection |
| `closed_fd` | Mount `rw`; open and close a real file descriptor, observe `release`, then ioctl that closed numeric fd | Kernel returns `EBADF`; no ioctl callback or mutation | Does not send a stale FUSE handle to the callback |
| `lost_reply` | Mount `rw`; callback records publication and deliberately exits its process before any reply | Caller gets an error, accepted marker remains, and no automatic retry; request/callback count recorded | Process death is deliberate fault induction; it does not prove spontaneous reply failure or durable publication |
| `notifier_after_unmount` | Mount `rw`, retain `fuser::Notifier`, unmount, call real `inval_inode` | Record actual provider return/errno after kernel connection loss | This is a real send, but cannot be a post-publication failure inside a live ioctl callback |

No case is sampled for performance. Cache state is uncontrolled and irrelevant
to the semantic failure observations. The complete Docker-exec wall is recorded
for operations accounting only, with no numerical pass gate. If a case cannot
reach its intended kernel/provider state safely, mark it `INCOMPLETE`, not
`PASS`. No test injection may be mislabeled as a spontaneous failure.

## Prospective correction after the first four attempts

The first `lost_reply` attempt exposed a harness defect: its callback logged a
`published` marker but had not written an inspectable accepted state. Its
assertion also omitted the observed `ECONNABORTED` errno 103. Keep that raw
attempt `INCOMPLETE`; do not rerun it. Add one distinct case,
`lost_reply_state`, at a new source/binary identity. Its callback writes a
test-only 12,288-byte accepted-state artifact with the exact 4 KiB insertion,
then exits before replying. The caller must observe an error, the daemon exit
code must be 23, and the parent must verify the artifact's entire contents and
absence of an automatic retry. This proves test-only custody across deliberate
reply loss, not Workspace publication or crash durability. One attempt only.
