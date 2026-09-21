# Cancelled native pipe must terminate standard I/O loops

> **Status: implemented shared prerequisite; native pipe regression verified, not a performance claim.**
> Observed 2026-09-22 during round 32; implementation parent
> `3e5d6a66a9f4e4df4a8a19087e63909a18046c6d`.

The owner reported near-one-core CPU consumption from a layerfs-daemon process.
Read-only host inspection identified PID 11323, a macOS release executable under
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-216-concurrency`, running for 16:53:32.
Its parent was an orphaned `/usr/bin/time -l` wrapper; stdin/stdout/stderr were
pipes. This is distinct from this worktree's completed Docker builds/tests.

A one-second native stack sample captured 91 samples per thread: main waited in
Client::call_until's scoped-thread join, while layerfs-upload remained in
Submission::read → Frame::read_optional → std::io::default_read_exact → Pipe::read.
Eighty-eight of its samples were inside the monotonic-clock path. The old process
and current source share the relevant Pipe implementation; no inference about the
old binary's entire source identity is made from the other worktree's current HEAD.

The source cause is permanent cancellation returned as ErrorKind::Interrupted from
Pipe::ready. Standard Read::read_exact and Write::write_all retry that error. Once
cancelled, ready returns it before deadline checking on every call, so partial
frame reads or terminal writes can spin indefinitely and prevent scoped worker
join. A larger timeout cannot fix this, and a cancelled operation cannot become
eligible for replay.

The selected correction returns a terminal I/O error for cooperative cancellation
at the shared Pipe boundary. Genuine OS EINTR handling remains separate. External
watchdog regression tests must establish that cancelled read_exact and write_all
terminate rather than consume a core forever. Portable Source and every Pipe caller
are reviewed separately; no generic retry framework or new worker is introduced.

Original process inspection and stack sample are retained under this worktree's
round-32 check outputs. Ending another worktree's process requires explicit owner
authorization under repository AGENTS.md; source changes here do not modify its
running executable. Actual correction/check outcomes and disposition follow in
this record after verification.

## Correction and observed disposition

Pipe::ready now returns ErrorKind::ConnectionAborted for permanent cooperative
cancellation. Its actual OS EINTR/poll handling is unchanged. Every current Pipe
caller shares this correction. Portable Source callers propagate errors directly;
they do not invoke standard Read retry loops, so that separate contract is unchanged.
Product input seal: `41eb35f3c8be9f524570868cef18ebeef020ceac247c5aea4dc56477209cde35`.
The correction changes zero production LOC (112 →112) and adds one explanatory
physical line (131 →132); source classification and the counter are unchanged.

External pipe_cancel tests run each standard helper in a reaped child with a
50 ms Pipe deadline and a two-second failure watchdog. Before the fix, both
read_exact and write_all exceeded the watchdog and were killed/reaped; the original
Cargo exit101 and logs remain evidence. After the fix, both pass, as do the existing
blocked-pipe write deadline and portable Source checks (four focused PASS tests).
Round32's final affected host/Linux checks include the new tests. This proves
termination of the affected shared boundary; it does not reconstruct the old
process's original request or claim an exact measured CPU reduction for all traffic.

The owner explicitly authorized ending PID11323. Its executable path was rechecked,
SIGTERM was sent only to that process, and exit was observed without escalation.
No other worktree's source or process was changed. The original sample and exact
termination record remain with round32 checks. Subsequent build/test Docker
containers are capped at two CPU and compilation uses two jobs; heavy checks are
serialized. Functional mounted receipts verify their configured two-CPU quota.
These changes respond to host load and do not relax timeouts or create benchmark
performance evidence.

[Original regression](evidence/control-mount/checks/pipe-cancel-before-01.json),
[fixed regression](evidence/control-mount/checks/pipe-cancel-after-01.json),
[native stack sample](evidence/control-mount/checks/other-daemon-cpu-sample.txt),
[authorized process termination](evidence/control-mount/checks/other-daemon-termination.json).
