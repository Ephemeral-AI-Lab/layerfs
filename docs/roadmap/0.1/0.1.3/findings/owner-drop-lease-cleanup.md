# Owner-drop lease and mount cleanup

The Phase 1 `workspace-lease-lifecycle-proof` failed at source
`fb5b34f7a882e257cd3647591fbd6c7f6ac6c2ec` on the VM8 environment. Evidence:
`attempts/workspace-lease-lifecycle-proof-s1-verify-a25c0cd6205c/` under the
Phase 1 campaign directory. This is a required functional and cleanup failure,
not a Phase 2 optimization finding.

The first owner's active lease correctly rejected a second owner with
`WorkspaceBusy`. Explicit Clean End released it. The second owner then created
and read the same placement successfully. After dropping that second Client,
the first Client's immediate Create failed with `Workspace(Io(InvalidRequest))`.
The original error is preserved in `proof-outcome-before-cleanup`. Subsequent
cleanup checks found a 139-byte `mountinfo.txt` under the second owner's
Workspace directory. The final reported error is the spool-observation cleanup
failure. Container supervisor cleanup passed; no OOM or timeout occurred.

Source tracing finds no `Drop` cleanup for `Workspaces`. Implicit projection
destruction disconnects a daemon-backed mount without awaiting its acknowledged
close. The branch lease can be released before the daemon retires the old mount
reservation, so the next Create encounters a duplicate placement. Implicit
Workspace destruction also clears its spool without removing the complete
Workspace state directory. Explicit End already provides the required ordered
projection close, state cleanup, and lease release.

The proposed minimal repair reuses explicit Discard End for active sessions
when their final owner drops. It must preserve error diagnostics and avoid
panics in destruction. A focused local owner-drop regression and the real
failed lease proof are required before claiming the repair passes. At this
checkpoint the implementation and runtime confirmation are pending. Earlier
passing results remain at their actual sources; reuse requires a reviewed
source proof and evidence that this active-owner-drop path was not exercised.
