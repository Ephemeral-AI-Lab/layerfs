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

The minimal repair in `03d4914ee36da6d303ab268e9102519d1755a8e4` reuses
explicit Discard End for active sessions when their final owner drops. It
preserves cleanup error diagnostics and avoids panics in destruction. The
focused owner-drop regression passed once (0.02 seconds test execution; 8.62
seconds including its scoped build). The exact real Docker/FUSE lease proof
then passed in 5.211185208 seconds, with independent correctness, resource,
observation and cleanup validation reporting no issues or violations.

The corrected attempt is
`attempts/workspace-lease-lifecycle-proof-s1-verify-4264e5969411/`; qualification
is in `qualification/owner-drop-03d4914e/`. The original failed attempt remains
unchanged. Its leaked diagnostic was copied and hashed, then that exact stale
state was removed in an explicit recovery recorded under
`qualification/lease-owner-drop-failure/recovery.json`. This recovery does not
relabel the original cleanup result.

Earlier passing results remain at their actual sources. Their reuse requires
the exact source proof and successful Create/End balance (or initialization
without a Workspace), establishing that active-owner drop was not exercised.
The original 600-second proof receives the same additional predicate check;
it is not repeated or relabeled as a new-source run.
