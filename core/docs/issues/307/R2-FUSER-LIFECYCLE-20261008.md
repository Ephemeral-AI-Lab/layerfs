# R2 dependency checkpoint: observable fuser receiver lifecycle

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Implemented after e1cc4ef2e under the owner's scoped2026-10-08 authorization.
> This is dependency qualification, not completed LayerFS R2 or R2–R5.

## Implemented API and ownership

The [authorized extension](FUSER-LIFECYCLE-DECISION-20261008.md) adds
`Session::into_runner`, `SessionRunner`, `SessionMonitor`, `SessionSnapshot`,
`SessionPhase`, `ReceiverOutcome` and `SessionOutcome` to pinned fuser0.18.0.
The [implementation](../../../vendor/fuser-0.18.0/src/session/lifecycle.rs)
uses the existing receive/dispatch body, existing safe dependency capabilities
and fixed lifecycle state. Ordinary Session::run remains unchanged apart from
delegating to that body with an empty start callback.

Each receiver enters a rendezvous after allocating its buffer and before its
first receive. Serving requires all configured spawn acknowledgements and loop
entries, then releases that startup rendezvous. The event depends only on the
session owner and receive threads; it requires no ordinary filesystem request.
First loop exit/unwind monotonically revokes Serving. Monitor observation
deadlines return current state without cancellation, replay or disposal.

The consuming runner owns every successfully created JoinHandle. A later spawn
failure releases startup waiters as unserved and joins them. A receiver panic
does not skip sibling joins: the monitor reports Stopping while the session
owner retains and waits for the sibling. Each original join return/I/O error/
panic payload is in the outcome. Filesystem destroy runs after all joins and
its original panic is retained independently. Joined states disposal, while
`is_clean()` separately requires successful startup, loops and cleanup.

The new API does not abort or detach a connection, signal a caller process,
implement a native request dispatcher or drain daemon jobs. Mount ownership
remains the Session's original policy; LayerFS uses from_fd with its separate
retained device and explicit first-party mount/detach authority. A monitor or
outer thread alone cannot authorize Overlay revocation or Close.

## Dependency integrity

The [original timestamp provenance](../../../patches/fuser-0.18.0/provenance.json)
and patch are byte-preserved. The [lifecycle provenance](../../../patches/fuser-0.18.0/lifecycle-provenance.json)
separately pins its four changed/added files and exact diff. Its
[reconstruction receipt](checks/r2-fuser-lifecycle-20261008/02-patch-reconstruction.json)
applies that diff once to the prior checked-in dependency and verifies every
resulting authorized file and unchanged time.rs.

The focused guard requires both exact records and patches,85 original file
identities plus one added module, and the fixed allowed lifecycle file set.
It rejects any unauthorized source, provenance, diff, missing file or redirect.
The new tool test exercises both lifecycle-record/diff tampering and absence;
existing source tampering checks also cover all new lifecycle files.

No dependency package/version was added or updated. The independent platform
harness now names its existing locked nix0.31.3 directly; its lock diff adds
only that dependency edge. Core Cargo membership/lock remains unchanged and
replacement layerfs-fuse is not yet active. No registry package was edited.

## Selected proofs and actual results

[Selection](checks/r2-fuser-lifecycle-20261008/01-selection.json),
[source hashes](checks/r2-fuser-lifecycle-20261008/03-source-identity.json),
[prebuild integrity](checks/r2-fuser-lifecycle-20261008/04-prebuild-integrity.json)
and [build transcript](checks/r2-fuser-lifecycle-20261008/06-linux-build.txt)
pin the one selected identity. Locked all-target --no-run, actual binary builds
and warning-denying all-target Clippy pass on the retained ARM64 Linux image.
The image is `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.

| Case | Independent public observation | Result |
| --- | --- | --- |
| ready | Both loops Serving after buffer entry, zero ordinary callbacks; two DESTROY replies and both joins; destroy once | PASS |
| panic | Real Filesystem callback panic revokes Serving; live sibling stays owned; explicit fixture endpoint shutdown allows both joins; original panic retained | PASS |
| io-error | Closed external protocol endpoint makes both loops fail with original InvalidData; both joined, no clean-success claim | PASS |
| destroy-panic | Both receivers succeed; original destroy panic separately retained and called once | PASS |
| partial-spawn | Actual cgroup pids.max2 permits caller plus receiver0; receiver1 spawn returns EAGAIN; receiver0 exits the unreleased startup rendezvous and joins | PASS |
| invalid | Zero receiver configuration fails before any spawn; no Serving; exact startup error and cleanup | PASS |
| native | Actual /dev/fuse, safe mount, completed handshake, both loops Serving before ordinary callbacks, root stat, one plain detach, two successful joins | PASS, dependency scope only |
| timestamps | Rebuilt existing public parser/reply proof retains all five signed timestamp cases | PASS, parser scope only |

The [original case receipts](checks/r2-fuser-lifecycle-20261008/18-proof-summary.json)
record exact commands, hashes, container IDs,8s wall stops, exit states and
acknowledged removals. Every case was run once; none timed out. Complete case
walls are313893708–503781000ns, including owned container lifecycle; these are
budget observations, not speed comparisons or cold performance. Fixture waits
are bounded at2s. All selected runtime containers and the separate build container
were removed after exact observed successful exits. No Store/volume was created.

The new proof binary SHA256 is
`2a41c3e84f386f4ca533461e4b8412943fba85cf4f4a417bb80341f296ceda4a`;
timestamp binary SHA256 is
`af116fbb0dae82e5f65ee13158936d838b027579a11959bc1ed0399ed7e5d803`.
Native deployment is Privileged=false with only the recorded device/SYS_ADMIN/
AppArmor settings. It is a root dependency fixture; ordinary-user protection and
full LayerFS mount security are not proved by it. Natural caches establish no
residency qualification. Global Store/Durable execution is NOT_RUN.

[Final checks](checks/r2-fuser-lifecycle-20261008/24-check-results.json) pass:
48 tooling tests under100s,759-file product guard, core/harness formatting and
explicit formatting of the changed dependency source. No Rust test failure,
proof hang or budget relaxation occurred. There is no aggregate CI claim.

## Limits and next composition

No public fixture forces a chosen individual receive-buffer allocation to pause;
that separate delay injection was not run. The real partial-spawn failure and
startup rendezvous support the stated source mechanism without a product test
hook. The native proof covers Linux ARM64/two shared-fd loops only. Historical
fractional signed-minimum native timestamp FAIL remains unchanged; the parser
PASS is not a repair of that kernel boundary behavior.

The dependency prerequisite is implemented at this scope. R2 still needs the
real replacement Fuse service, event-driven admission/Store/semantic plans,
atomic indexed lookup/open/cookie custody, native control/SDK assembly, deployed
permissions and full normal drain. The earlier Pending Future checkpoint supplies
completion events, not those mechanisms. R3 mutation/coherence, R4 complete
captured namespace/bounded topology and R5 mounted Commit remain unfinished.
No owner approval is pending for the authorized lifecycle scope. R6–R9 and
reference retirement remain outside this batch.

The exact commit accounting compares first-party product source using the pinned
counter; dependency source remains separately classified and reported. No source
reduction or product milestone is inferred from excluding third-party patch bytes.

Production LOC:170710 ->170710 (delta +0). Core105293, active62419,
reference65417, excluded predecessors37431 and excluded integration5443 are
unchanged. Separately, the affected dependency Rust files total1198 ->1505
nonblank/non-comment/non-test lines (delta +307), including296 lines in the new
lifecycle module. This is explicit dependency growth, not a zero-sized change.
The [snapshot comparison](checks/r2-fuser-lifecycle-20261008/25-exact-production-loc.json)
uses the same pinned counter for both snapshots and separately counts those
dependency files. There is no relocation, retirement or algorithmic shrink claim.

The [document/preservation check](checks/r2-fuser-lifecycle-20261008/26-document-preservation.json)
resolves163 local Markdown targets, verifies the source hashes used by the
proofs and preserves the four original untracked files. Full whitespace is
FAIL exit2 on five context-space lines in the exact saved lifecycle patch;
source/docs/JSON whitespace passes. The reconstructible patch is byte-preserved,
and that full-check failure is not relabelled.
