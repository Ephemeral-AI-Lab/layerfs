# Owner authorization: narrowly scoped fuser receive-loop lifecycle

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Owner decision2026-10-08 during dispatched R2–R5 work after4236225ee.

The owner was asked whether to authorize the exact
[scoped lifecycle proposal](checks/r2-native-prerequisite-20261008/05-lifecycle-interface-proposal.md)
after the [source prerequisite inspection](R2-NATIVE-PREREQUISITE-20261008.md).
The owner answered: **“Authorize the scoped lifecycle extension.”**

This authorizes the additive lifecycle API for the pinned fuser0.18.0 receiver
loops: observable entry/exit, a startup rendezvous independent of kernel requests,
retained handles/outcomes through partial startup and panic, and explicit complete
join/disposal. It supersedes the time.rs-only restriction for this exact extension.
The signed-timestamp patch, original archive and original evidence remain intact.
No other third-party change, version replacement or general unsafe exception is
authorized. First-party mount/abort/detach, native service and aggregate daemon
Ready/drain keep their existing owners and proofs.

Implementation must record the additional exact file/diff hashes separately from
the immutable timestamp provenance and update the focused guard/self-tests to
accept only the authorized bytes. Public external tests and actual native
qualification remain required. Authorization is not implementation or acceptance.

The R2 readiness prerequisite is now authorized engineering work. It is no longer
waiting for an owner decision. Full R2–R5 remains the active assignment; R6–R9,
optional admin, remote publication and early reference retirement remain excluded.
