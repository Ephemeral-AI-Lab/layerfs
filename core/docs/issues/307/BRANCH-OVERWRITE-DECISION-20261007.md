# Owner supersession: overwrite-only Branch publication

> **Status:** Current owner direction, received2026-10-07 while F9 was being
> prepared at `bd5ab61d6`; implemented and proven below. Not release evidence.

Explicit owner decision, dispatched with authorization through the side
conversation: “same-Branch race — we do not need it at the moment, i have not
thought about how to deal with conflict. for now, it should be overwrite only.”

A completed candidate atomically overwrites its Branch head when its one database
write attempt succeeds. Advancing the Branch since capture is not by itself a
refusal. Last means database publication effect, not reply arrival order. No
merge, rebase, expected-head refresh or replay is added. Capture provenance stays
with the candidate: a newly published Commit's parent is the captured head, not
the intervening head. Displaced immutable Commits remain stored; the Branch's
current ancestry can therefore change. UpToDate compares against the current
Branch root within that same atomic transition; a stale unchanged candidate can
still overwrite a different current root.

Content savedness/dependency closure, stack/Branch ownership, scope/profile and
captured base validation remain. The existing Branch base is immutable through
the exposed Branch operations; publishing against another base is still refused,
without silently rebasing the candidate. LayerStack publication and its separate
conditional contract are unchanged. Database contention remains one attempted
BEGIN IMMEDIATE with zero timeout, typed Busy before that write's effects, no
retry or global writer gate. Original known/unknown and local-install custody
remain distinct.

This supersedes F8/F9/F13's same-Branch HeadMoved criterion. Historical source,
traces, receipts and verdicts remain evidence of their original conditional
contract; none is overwrite evidence. Broader conflict resolution is owner-deferred.
The pending F9 original-race proof is withdrawn before execution; its03 build
receipt is retained as a build, not functional closure.

## Deepest-file implementation plan

- Reuse atomic stage_and_commit and exact-token commit_staged. Change Persistence
  `history/commit.rs` to validate captured context, derive parent from the capture,
  compare UpToDate against current root, and perform one overwrite transition.
- Change shipped `commit_advance_branch.sql` to remove the captured-head predicate;
  retain exact Branch/base selection. Remove the obsolete fallback head-query SQL
  and dispatch entry. A zero update under the held transaction is integrity
  failure, not an invented conflict or another attempt.
- Clarify History catalog/records and daemon/control contracts: expected_head is
  captured provenance for Commit, not a head CAS. Keep other typed conditional
  errors where their remaining operations still use them.
- Update the narrowly affected atomic/separate-stage history proofs, daemon
  Store-half proof and native control proof. Add stale-no-change overwrite and
  current-root UpToDate cases, parent/provenance assertions and exact final head.
  Keep real Busy/no-stage and unknown/local-install coverage at the changed path.
- Adapt the unexecuted F9 process proof to deterministic A-then-B publication of
  two candidates from the same starting head; both succeed and final root is B.
  Preserve independent overlays, interleaved Saves and killed-daemon checks.
- Build first, Disposable host then affected Linux binaries, append-only receipts,
  final scoped checks and exact LOC. No historical measurement or E04 is rerun.

## Retained evidence and scope

F8/F13 are closed under this policy by the host/Linux receipts in
[pre-s8-branch-overwrite-20261007](checks/pre-s8-branch-overwrite-20261007/):
01/04 build,02/03/05/06 host execution,07 source manifest,08 Linux build,
09/10/11/12 Linux execution. Each platform passes3 atomic History,6 separate-stage
History,8 Store Commit and6 native-control bodies. The family helper explicitly
selects Disposable for create/reopen; no Durable execution occurs.

The same-root stale request is UpToDate against the current root. A different
stale root and a stale unchanged capture both overwrite; all four transitions
use exactly one successful write transaction and leave no stage. Displaced
Commits remain readable, and both new records keep their captured parent. Each
small daemon Commit still uses3 writes:1 initial reservation,1 publication batch,
1 atomic History. Real process-held Busy before Save and at History leaves no
new stage; later explicit operations work. Original missing-dependency, nested
unknown, lost native reply, known publication/local-install failure and later
mutation custody remain covered. Terminal unknowns are never settled by the
observer; tests explicitly tear down their owned fixtures after owner stop.

F9 is closed by the [shared-process checkpoint](PRE-S8-F9-20261007.md), including
named-volume Linux proof after actual daemon SIGKILL. No same-Branch conflict
selection was executed for F9. Historical F8/F13 HeadMoved traces and source pins
retain their original meanings. Broader conflict policy remains owner-deferred.

Identities: parent `bd5ab61d6bf34570842c3be8faddbc722a7c34ec`; exact changed source,
config/lock, binary and image hashes are in07 and each run receipt. Host Rust1.85.1,
macOS system SQLite3.51.0; Linux Rust1.85.1 with bundled SQLite3.53.2. Repository
ARM64 flags apply. Disposable uses `sqlite-wal-off-v2`; Durable code builds but
execution is `NOT_RUN — deferred by owner for Disposable-only development`.
Tests use their natural functional cache state: mount warms immutable caches,
new daemon processes start fresh caches. No OS-cold or timing admission is claimed.
All original test invocations have100s outer stops; none reached the stop.

Final checks13–16 pass: scoped all-target Clippy with `-D warnings`, formatting,
706-file boundary scan and46 guard self-tests. Cargo notes the already-authorized
fuser patch is unused by the active pre-FUSE graph; no dependency change follows.
New local documentation targets resolve; the Commit proposal's two retired-source
links are now historical path references rather than broken file links.

Production LOC:165678→165669 (delta-9), core100261→100252,
active57387→57378; reference65417, excluded predecessor36325 and excluded
integration6549 unchanged. Exact staged/first-parent comparison and pinned counter
method are in17-loc.json. This is the Branch overwrite policy change plus F9
proof; it performs no further host-mediated transport retirement. Historical
receipts, protected handoffs and root reference are preserved.
