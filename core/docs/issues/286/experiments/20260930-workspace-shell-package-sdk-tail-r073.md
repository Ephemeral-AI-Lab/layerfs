# Family7 SDK tail r073 —INCOMPLETE

**Status: Dated diagnostic benchmark checkpoint; numeric cache INELIGIBLE.**

Clean measured source `375b454787b4ff321ac3d0dabb680df6b0b50dea`, tree `ab43640f9cd7a4bbbbae513c590309286b544d9d`.
[Compact receipts](20260930-workspace-shell-package-sdk-tail-r073-receipts.json)
retain both selected cases, raw/proof/extra custody manifest hashes, changed
release binaries/image and reused closed masters. Original workload,25s/15s
command and9s proof bounds unchanged. No earlier family/performance arm replay.

| SDK case | Complete command ns /bound | SDK phases | Separate proof | Cleanup | Result |
| --- | ---: | --- | --- | --- | --- |
| workspace-shell-package-many-1025-sdk-v2 | 25007718875 /25000000000 | final receipt absent; retained SDK span Exec8749987000ns, Commit9386892917ns with error | NOT_RUN | UNKNOWN at timeout; retained state/external teardown recorded separately | FAIL /TIMEOUT |
| workspace-shell-package-many-129-live-g2-sdk-v2 | NOT_RUN /15000000000 | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN after prior cleanup failure |

The progress fix passed the original5s outer silence boundary: actual daemon
Commit and SDK wait both9.385–9.387s. Commit returned an error, then lifecycle
cleanup exceeded the25s wall and the outer harness killed the driver before its
final receipt. The exact typed error was lost because the example printed only
at the end of cleanup. No interpretation of successful diagnostic delivery as
successful canonical/local Commit. Separate proof skips both non-complete rows.

Post-timeout labelled custody inspection matched exact measured image
`sha256:b7b7f1fd7034f7291cc85b9a29cae5595d4dc68d5fff79d0a3a88f33d4226e66`,
SDK label `bench-3309e2de2b801f8b-47904`, run53bde8b60aa0f0a256b7992a3808a6ae
and exited container eb0504ca6683 (137, not OOM). Retained logs show shutdown Busy;
its356 private backing files/1458176 apparent bytes were copied before owned
container/volume removal. External teardown succeeds; it is outside the timed
attempt and cannot promote its graceful cleanup verdict. Unrelated resources
were preserved. Raw extra evidence is a separate append-only diagnostic directory,
not a mutation of r073's manifest.

Next: one labelled count-driven native1025-file diagnostic, using the existing
prepared fixture/public APIs and `LFS_CAPACITY_DIAGNOSTIC=1`, prints exact dirty,
saved/canonical calls, typed outcome and before/after physical custody. It is
not a repeat SDK sample or speed gate. Record interim SDK outcome/post-status
before cleanup in the existing example so future timeouts retain typed custody.
No product workaround, deadline/quota/worker lift or streaming refactor is made
before that cause is identified. Six passing r071 SDK/full oracles and r072 native
progress/custody remain source-linked reuse; retained129 still needs its first run.
Production135672(reference65417/Core70255), PhaseB+233; this round/diagnostic
harness edit has production delta0. All numeric rows remain INELIGIBLE.
