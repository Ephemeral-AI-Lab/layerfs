# R2 native lifecycle prerequisite at dispatched R1 source

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Inspected 2026-10-08 at `4236225ee70da89a264308167549f95e660c5dcb`.
> R2–R5 is dispatched. No native implementation or acceptance is established here.

Subsequent same-day [owner authorization](FUSER-LIFECYCLE-DECISION-20261008.md)
approves the scoped lifecycle extension proposed below. The decision wait is
resolved; implementation and verification remain required. Original inspection
receipts and the proposal's pre-authorization status are preserved.

The dispatched product matches the verified R1 handoff. The remaining immediate
native dependency gap is an observable lifecycle for **each** of the two fuser
receive loops. The selected public API does not expose that lifecycle. This
checkpoint records the gap before activating a replacement or claiming Ready.
It does not relax I-3/I-8/I-9/I-14, complete R2, or withdraw independent R4 work.

## Inspected state

[Inspection receipt](checks/r2-native-prerequisite-20261008/01-inspection.json)
records the exact commit, core/reference trees, manifest/lock/config/counter
hashes, three reusable R1 binary hashes, protected notes and runtime inventory.
Core product tree is `e3a61dfd814a579b58f56b63754ec3dbce83d67e`; reference is
`498dd1917812ae90efb8841f57e22bfc284e96fb`. Twelve members remain active; the
incompatible dormant FUSE source remains excluded. No archived draft was applied.

Docker Engine29.5.2/API1.54, Desktop4.76.0(228118), Linux ARM64
6.12.76-linuxkit and the pinned R1 image are available. This inspection created
no container, Store, volume, mount or process owner. Actual device/mount security
and changed deployment powers remain NOT_RUN. The four protected containers were
running and were not operated on. The existing goal observation returned null.

The [focused integrity check](checks/r2-native-prerequisite-20261008/02-fuser-integrity.json)
passes the authorized checked-in patch and103 manifest/lock/config checks.
It does not verify an installed registry copy or native behavior. R1 tests were
not resampled: the inspected product/build inputs remain identical at their
existing proof scope. Durable execution remains
`NOT_RUN — disabled by owner until explicit reauthorization`.

## Exact source findings

[Machine evidence](checks/r2-native-prerequisite-20261008/03-source-evidence.json)
pins the source hash and line intervals in
[fuser session.rs](../../../vendor/fuser-0.18.0/src/session.rs).

| Source boundary | What the source establishes | Missing fact |
| --- | --- | --- |
| `Session::from_fd`, lines198–221 | Completes the library handshake path before returning | No receive loop has yet been spawned |
| `Session::run`, lines291–307 | Creates private receiver join handles; each spawn uses `?` | No public start receipt; partial spawn failure drops earlier handles without joining them |
| Join loop, lines309–332 | On its ordinary path, consumes all joins and returns the first receiver I/O error | A first join panic returns early; an arbitrary `Err` does not establish all joins |
| `SessionEventLoop`, lines514–555 | Private receive/dispatch loop, including error/ENODEV exits | No public loop-entry/exit observer or caller-owned step interface |
| `BackgroundSession`, lines558–594 | Exposes the outer `run` thread | No individual receiver handle |

Consequently, successful `run()` completion can be used for joined-loop evidence.
Errors must retain the original result and unestablished join disposition. This
is not a reason to require every failure to become a successful teardown.

The unresolved earlier boundary is Ready. A wrapper can observe its own thread,
`init`, or a filesystem callback, but none is the start of every configured
receive loop. A finite shared-descriptor probe can be served entirely by one
receiver. Making an admitted callback wait for another kernel request would
violate I-9; filling or withholding credits solely to force that rendezvous would
not prove independent admission progress. Callback-installed thread-local exit
guards can witness termination of threads that actually entered callbacks, but
do not establish entry of a second unobserved loop or own the library's joins.
Proc task-directory metadata supplies neither of those missing interfaces.

The pinned nix0.31.3 source has no safe `pidfd_open`/thread-pidfd wrapper.
Its peer-process/fanotify pidfd support is not a receive-loop lifecycle API.
This is an inspected API limitation, not a claim that every conceivable OS
observer is impossible. Raw libc/unsafe, a replacement protocol engine and a
larger third-party patch are not selected alternatives under the current rules.

## Concrete decision proposal

The [lifecycle interface proposal](checks/r2-native-prerequisite-20261008/05-lifecycle-interface-proposal.md)
defines the smallest missing dependency contract and its prospective verification.
It requests an additive, narrowly audited lifecycle extension to the pinned
fuser0.18.0 session implementation, with the timestamp correction preserved.
It is **not applied, built or qualified**, and is not an authorization to modify
the dependency. No patch/hash allowlist has been broadened.

The exact policy requiring an owner decision is
[core's fuser exception](../../../AGENTS.md#fuser-provenance-and-authorized-patch-checks):
“with only `src/time.rs` changed by the recorded upstream timestamp fix and
regression tests.” The [S8 activation plan](S8-IMPLEMENTATION-PLAN-20261008.md#2-cargo-activation)
also requires an incompatible safe-wrapper dependency to be reported before a
workaround. The current implementation assignment explicitly preserves both rules.

Permission to implement R2–R5 therefore does not itself authorize this additional
third-party change. A decision can authorize the narrow lifecycle extension,
or retain the present patch restriction while a conforming alternative is
established. Neither choice waives native proofs or permits Bound to mean Ready.

## File plan and remaining implementation

The [deepest-file plan](checks/r2-native-prerequisite-20261008/04-deepest-file-plan.md)
uses current source owners. It retains one shared Fuse dispatcher, the existing
SQL owner/Store/registry, atomic indexed lookup custody, full normal drain and
the separate captured-namespace producer. It is an implementation plan, not code.

R2 native readiness and FP-1/FP-2/FP-21 acceptance remain unestablished. R3's
mounted mutation proofs and R5's mounted Commit oracle depend on that floor.
R4 component construction and asynchronous engine prerequisites remain independent
authorized work; this source gap does not certify or prohibit them. R6–R9 remain
outside this dispatch. No performance or storage claim is introduced.
