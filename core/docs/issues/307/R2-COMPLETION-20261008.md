# R2 completion: native read path, Ready and normal unmount

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

R2 of the cluster-two rollout is implemented and functionally verified on real
kernel mounts. A Workspace bound by the daemon can be attached as a Linux FUSE
mount, served read-only to any process with mount visibility and permission, and
unmounted through a reversible probe and complete connection drain. R3 and R5
are not started and R4 keeps its earlier component state. Nothing in R6–R9 was
executed and the root reference tree is intact.

Architecture: [native mount session](../../architecture/76-native-mount-session.md)
and [native request service](../../architecture/75-native-request-service.md).
Evidence, including every failed attempt and unrun case:
[R2 completion checks](checks/r2-completion-20261008/00-results.md).

## What R2 delivers

| Handoff step | Delivered |
| --- | --- |
| 1 Callback accounting | Every operation the adapter can receive has one declared disposition and is counted once by opcode and disposal |
| 2 Mount and session | First-party direct mount syscalls and a session owner in Fuse: attach with phase-exact failure, all-loop serving evidence, first-exit fence, detach and drain |
| 3 Daemon assembly | One shared dispatcher over the daemon's existing Overlay Owner and Store, started before control readiness; the registry is the sole mount authority; status covers native phase and work |
| 4 Control and SDK | Additive `Attach`, `Locate`, `Ready`, `Located`, `Retained` records and a native status block; SDK `attach`, `locate` and two-acknowledgement `mount` |
| 5 Normal unmount | Plain `umount2` probe; `EBUSY` restores Ready; detach, joins, request drain, revocation, Close, registry removal; exact `Retained` custody when a step is not established. No process is signalled. Abort authority is kept for R6 |
| 6 Sandbox | `CAP_SYS_ADMIN` and `/dev/fuse` with `Privileged=false`, `no-new-privileges` and the nonroot command identity; inspection requires exactly that configuration |
| 7 Proofs | Host Init, seal and install into a named VM volume; real mounts; independent complete-root oracle; unregistered external access; retained descriptor, `O_PATH` and working-directory Busy; repeated fresh mounts; sustained same-mount work; full normal drain |

## Decisions that need owner review

These were made to finish R2 and are each a contract point, not a detail.

1. **Revocation no longer refuses open handles.** After a successful detach the
   kernel may never deliver RELEASE or FORGET. Overlay revocation is now fenced
   only by live request sources and reads; unreleased file handles, directory
   handles and lookup counts are retired by bounded indexed maintenance. This
   supersedes the earlier component rule and follows the proof plan's FP-21 and
   FP-31 wording (missing FORGET at detach is retired only after drain). Two component
   assertions of the old rule were removed.
2. **The daemon narrows `/dev/fuse` to 0600 and refuses to start without a
   device it owns.** This keeps the command identity from opening a second
   connection. It also means a daemon cannot start control-only on Linux where
   the device is absent.
3. **The Sandbox container now carries `CAP_SYS_ADMIN` and
   `apparmor=unconfined`.** Both are needed for `mount(2)` in the tested Docker
   environment. `Privileged` stays false and commands keep an empty effective
   capability set.
4. **A writable OPEN is refused with `EROFS`** until R3, as is every other
   mutating operation. The mount table still reports `rw`.
5. **No daemon-side in-flight precheck on unmount.** The kernel's `EBUSY` is the
   only fence; a precheck produced a false Busy from post-reply bookkeeping.
6. **Native status is one engine observation.** It omits engine open-handle and
   lookup counts, retirement debt and cleanup state.
7. **Observation deadlines of five seconds** for all-loop serving and for drain.
   Expiry cancels nothing: attach reports its exact remainder and unmount stays
   `Retained`.

## Known gaps

- `abort_bound` was false in every run: fusectl is not mounted in the container.
- Kernel FORGET delivery was not observed; lookup retirement ran through
  revocation maintenance.
- The `Retained` unmount stages and the attach-failure-after-mount path have no
  executed case.
- A re-Attach immediately after a failed Attach can be refused until the revoked
  mount row retires; the caller observes and issues a new Attach.
- FP-9, native FP-8 and the fusectl half of FP-2 are unrun; FP-17, FP-21 and
  FP-31 are partial. The row table is in the checks record.
- Two daemon test binaries need explicit preconditions that were not supplied
  and remain unrun at this identity.
- No resource, memory, cold or timing claim is made. The full-topology proof ran
  under an owner-allowed 15 s limit after two failures at 9 s.
- Durable: `NOT_RUN — disabled by owner until explicit reauthorization`.

## Next

R3 composes ordinary mutation and kernel coherence over this session: writable
handles, WRITE and the namespace mutations that are refused today. R4 completes
the captured namespace and incremental topology, and R5 is the mounted live
Commit with known install. Each remains separate unfinished work under the
[rollout ledger](ROLLOUT-LEDGER-20261008.md).
