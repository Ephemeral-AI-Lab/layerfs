# #241 baseline Exec after complete missing-Inspect retention

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Frozen before the sole public SDK attempt at source `fdcdba0f366da6f80a13bde4252e38cb9d83e2df`.

Use a fresh validated writable copy of the closed 1 MiB master, one Branch,
one Sandbox and one mounted Workspace. Call public SDK Exec once with exactly
`printf baseline > .position-baseline`; then unmount, delete the Sandbox and
confirm absence. The [plan](plan.json) fixes binary, image, master, copy,
telemetry and output identities. There is no range edit, Commit, retry,
registered position or latency admission. Cache is uncontrolled.

Require both missing-path Inspect refusals and the subsequent ReserveInodes to
use the retained daemon Service session: no fresh TCP, Noise or Hello child
may appear in those three daemon operations. Require a typed exit-0 Exec,
host acceptor summary, running/non-OOM Docker state, unmount and confirmed
Sandbox deletion. Missing observations classify the diagnostic `INCOMPLETE`;
any new ReserveInodes connection or failed cleanup is `FAIL`.
