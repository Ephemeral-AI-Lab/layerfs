# Review of the reused-transport SDK route receipt

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

This is a read-only review of source `16281ff9e5d67bd092e8b6d3e3286e5bfd1a6aa0`,
the [reused-transport receipt](evidence/sdk-route-reuse-20260924-01/report.json),
and [issue comment 5805701156](https://github.com/Ephemeral-AI-Lab/layerfs/issues/236#issuecomment-5805701156).
No source was changed and no performance arm was rerun for this review. The
receipt manifest verifies, the live route exited successfully, and its
`functional_status=PASS`, `performance_admission=INELIGIBLE` and uncontrolled
cache declaration are supported. The 402.519 ms route and 107.896 ms
post-Create sum match the retained LFT1 report. Neither is a speedup proof.

## Corrections required for an authentic progress claim

1. The statement that v0.1.6's **30.271 ms** four-call figure is absent from
   the [comparison report](evidence/sdk-route-v016-20260924-01/report.json)
   is incorrect. The `v016.phases` fields sum to **30.271459 ms** (Mount
   14.668209 + Exec/drain 4.911833 + Commit 8.052917 + End(Clean) 2.638500).
   The `current.phases` fields sum to 294.331876 ms. Neither sum qualifies a
   cross-version speed claim because APIs, topology, dependency identities and
   cache state differ.
2. The new reporter's `owner_lookup.hello_count` counts only *fresh host-side*
   `owner.hello` spans. Reused-socket Hello calls in
   `core/crates/layerfs-sandbox/src/session.rs` are not wrapped in that span.
   The primary daemon's raw LFT1 stream records checked `SandboxHello`
   requests with IDs 1, 3, 5 and 7 before Mount, Exec, Commit and Unmount.
   Thus the table's `1,0,0,0` means **one newly opened host Hello span**, not
   one Hello request across the four operations. The session count may have
   fallen, but the checked Hello request count did not.
3. The asserted **5 → 2 Service connections** is not directly observed:
   Bridge exposes no connection counter in this receipt. Code reuses a
   `Transport` keyed by delivery thread across operations, with a two-second
   idle limit; it is not restricted to one connection per phase. Five
   semantic upstream requests for Mount and Commit are visible, but their
   exact connection count is not. Report connection reduction as an intended
   code mechanism until a direct accept/connect count proves it.
4. `core/crates/layerfs-daemon/src/run.rs` holds the global
   `Mutex<HashMap<ThreadId, Transport>>` guard across `transport.call`, which
   includes TCP/Noise I/O and Service processing. Consequently unrelated FUSE
   delivery threads *do* contend and serialize on that mutex. The comments
   and issue claim of independent per-thread transport are contradicted by
   the implementation. The sequential live route does not test this load
   behavior; fix and cover concurrency before treating reuse as load-bearing.
5. `Sessions::lease` silently discards a retained socket after a failed
   reused Hello and invokes `check()` for a fresh connection in the same API
   call. It does not replay a mutation, but it is an error-driven transport
   fallback; the handoff and `core/AGENTS.md` require one attempt without
   such fallback. Return the failed check or explicitly revise the contract
   prospectively. The 2-second client idle bound also does not prove a
   dropped old session has cleared the daemon's one-session slot before a
   replacement connects.
6. Workstreams 2 and 4 are **partially instrumented**. Mount has attach and
   FUSE-mount child spans, and Create has substeps; Commit has only one
   `daemon.commit` child covering the entire native Commit. Projection and
   upstream counters were added to `WorkspaceStatus`, but no FUSE count or
   cache-outcome row is retained in this receipt. The previous Create swing
   cannot be attributed from one newly split Create window. State these as
   remaining investigations, rather than all four workstreams completed.

The issue body still describes Workspace/Exec as skeletons only, whereas the
later user-directed route work is implemented. Keep that original scope
history visible and clarify the expanded scope in an issue update or a
separate tracked issue. Preserve the existing receipt and failed attempts
unchanged. A future source identity needs direct connection/Hello/FUSE counts,
concurrency proof, and nested Commit spans before a stronger claim.
