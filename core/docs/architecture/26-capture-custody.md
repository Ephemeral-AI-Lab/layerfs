# Exact captured state and bounded custody observations

> **Status:** Current implemented architecture; S2 closure after `7019801f9`, not complete native/runtime/Commit qualification.

Schema5 records `workspace.captured_revision` alongside the sealed generation.
Capture sets both atomically from existing published state, then advances active
and resets only active dirty counts. The installed base cannot change while that
capture is retained. Validation checks generation, frozen revision and base root;
modified or stale values cannot install, read or release another capture.
Known install and known closed release clear both paired fields in one transaction.

`Overlay::retained_capture` is a route-qualified point observation of the exact
existing capture. It distinguishes a live/closed Workspace without a capture from
an absent incarnation. `pending_publications` pages revision/generation tickets
through `(ns,revision)` with64-key windows. Observation neither repeats the
operation nor releases a ticket or infers upstream history disposition. A caller
must retain exact uncertainty and settle only the actual reply-attempt/fenced
operation it owns. There is no automatic recovery, rollback or resend.

Each minted Route also contains a process-local monotonically allocated engine
identity. All route entry checks refuse another owner even if local SQL keys match.
This local capability is not a remote authenticator; native control/runtime must
still validate their authenticated daemon/Workspace incarnations and authority.

Captured cell reads validate the exact retained capture, so construction with
existing custody can read bytes during logical close. Current live reads still
reject close. Install/release invalidates the capture token and its read surface;
independent reader/orphan ownership and live generation cleanup remain S6.
The daemon exposes typed bounded capture/name/cell/ticket observations with charged
reply windows, without holding SQL across content demand or whole operations.

Actual EXPLAIN and correlated VM/row counters, source identities, covering host/
Linux checks and precise milestone/resource boundaries are in the
[S2 exit audit](../issues/307/S2-EXIT-AUDIT.md). Ticket/capture probes are scope
counts, not cache-cold latency measurements. Installed physical debt and aggregate
residency are not accepted from these tests.
