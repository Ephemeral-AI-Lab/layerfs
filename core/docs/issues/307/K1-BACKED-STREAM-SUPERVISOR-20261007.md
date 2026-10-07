# Backed file edits, streamed directories and occupied-slot supervision

> Status: committed locally as `dcdf527584675849e7839ca4118d71ac9aa4b193`, after first parent
> `889836c446507c726a53f0ccf1e4418bd4d0946f`.
> Product subtree `b99c4a3050b88f2ce47ba1daf334710ed90ee267`;
> committed tree `0e83d0ac14c6ff06b4c5a68fa0a1600eca8a6c3b` matches the staged tree.
> [Commit/count confirmation](checks/k1-backed-stream-supervisor-20261007/38-committed-verification.json)
> and [tracker receipt](checks/k1-backed-stream-supervisor-20261007/39-tracker-receipt.json)
> are retained by the next checkpoint; original evidence keeps its source pins.
> S0/S7–S13 remain incomplete and unchecked.

This joint slice implements the file-editor state port, its real Workspace/Daemon
adapter, streamed directory input and fair turns across occupied Supervisor
slots. It continues the [full implementation plan](IMPLEMENTATION-PLAN-S7-S13-20261007.md)
and [K0/K1 design](K0-K1-BACKED-EDIT-DESIGN-20261007.md). It creates no second
database, canonical tree algorithm, provider lane or dependency. The root
reference remains intact. No performance treatment is sampled or requalified.

## Implemented source and custody

Content uses one private tagged split/join engine for memory and backed edits.
Stored, editable Node and already-canonical draft Page references have exact
distinct provenance; a missing draft never falls through to Stored I/O. The
backed route puts growing draft/reference/detached/resolution/emission indexes
behind owned raw records. Each mapping acceptance has an original Attempted
marker and becomes Accepted only after known consumer success. A 51-byte
resolution witness rechecks exact bytes/extents/level and root/non-root fill
before acknowledged identity reuse. FileState acceptance has its own pending
and known-complete marker. First error stops further work without resend.

The memory route keeps its original deferred charge and 8 MiB-derived refusal.
Decoded ExtentNode plus entries/128 bytes, or canonical Page length/128 bytes,
remain its proxy; tags, indexes, provider/Save windows and whole-process residency
are separate. Empty, WholeFile, whole-base streamed construction and no-op routes
intentionally bypass mutable backed state. See [the source contract](../../architecture/50-backed-file-edit-state.md).

Workspace's `IndexedEditRecords` binds explicit OperationOwner/full file scope
to an existing `OverlayScratch`. It acquires/releases no owner or database on
construction or Drop. Direct Overlay and OwnerClient implement the same port.
The adapter admits actual incoming Vec and nested capacities before conversion,
moves nested bytes, records both descriptor allocations' overlap, and preserves
the first original unattempted input or attempted Completion. NotApplied retains
its original credited Completion, returns one bounded deciding copy and ends
later submissions. Copy observations describe returned windows and descriptor
conversion only. Captured reader/root, Save and transport fences still govern
explicit release. See [adapter scope](../../architecture/52-workspace-edit-backing-port.md).

The additive None-exclusion key window uses an indexed four-parameter query,
32 bound bytes and at most 64 full keys/2,048 bytes. Some uses the existing exact
exclusion query. Every 32-byte identity, including zero/all-ones, remains valid.
The existing schema15, bounded row/value jobs, accounting triggers and automatic
last-owner cleanup remain. The borrowed Overlay API and owned service port
have distinct incoming-capacity ownership. See [engine scope](../../architecture/49-indexed-operation-scratch.md).

Streamed directory headers, per-parent fallible cursors and tri-state changed-name
points feed one canonical filesystem validator/update driver. Declared totals,
strict ordering and cursor/point agreements are checked. Effective directory
consumption retains one bounded base page and changed-name lookahead rather than
a complete row/adjacency map. The old resident route and public signatures remain.
Growing demanded/addition/candidate/topology/new-parent/content/count/reducer/
release state and existing resource refusals remain; this is the input-stream
slice, not total-state-independent construction or root topology provenance.
See [streamed input scope](../../architecture/51-streamed-directory-input.md).

Supervisor turns scan its configured slot window from the next position and
select the next occupied attachment, including pending fences. One turn advances
one attachment phase and one existing provider service unit; holes no longer
consume empty serving turns. No new worker, registry, queue or retry is introduced.
The source model of six phases ×32 slots ×131 requests =25,152 previous turn/sleep
opportunities is an accounting model, not an observed histogram or wall-time cause.
Progress/wait reports and safe whole-owner parking remain subsequent work.
See [runtime supervision](../../architecture/45-runtime-supervision.md).

## Failures, repairs and retained checks

The append-only [check directory](checks/k1-backed-stream-supervisor-20261007)
retains exact argv, explicit wall ceilings, stdout/stderr, failures and outcomes.
All normal construction has `LAYERFS_CONSTRUCTION_WORKERS=1`. Tests build with
locked no-run first; every invocation has an outer timeout at most 120 s, and
Linux uses the pinned ARM64 image with child 110 s/kill-after 1 s. No test timed out.
Functional walls include startup/harness/cache effects and are no performance
admission. The selected package graph matters to build reuse; the retained
`build-scope-note.json` explains compilation included in the early command 06.

| Retained failure | Cause and scoped repair |
| --- | --- |
| 03 joint no-run | External streamed test imported RowSource from the wrong public location; corrected its import only |
| 04 joint no-run | External backing test constructed tuple ReleaseOperation as a struct; corrected exact public syntax |
| 08 legacy edit selection | Superseded leaf stayed retained while both split halves were allocated; restored early leaf release before allocation |
| 24 host Clippy | Complex private iterator return type; named it with a private lifetime-aware alias, without behavior/API change |
| 29 native proof preparation | Required output root was absent after the Durable host fixture was constructed; Linux launch and serving loop were NOT_RUN, and Disposable was NOT_RUN. Prepared a different fresh root for command30, retaining the failure |

Failure 08 kept its original bound, workload, counter formula and vector
`[3984,4064,4248,4888,6168]`. Corrected receipt 11 records
`[2208,2288,2448,2768,3408]`; each remains at most 64 KiB and adjacent growth at
most 1,024 bytes. Branches retain their original parent through both joins,
because a singleton returned summary supplies no counted replacement parent.
A new public case observes three equal draft Page children, exercises both
boundaries and a later legal monotone overwrite, then checks roots/bytes/cleanup.
An arbitrary private split schedule with bare repeated aliases remains a scoped
investigation; there is no demonstrated legal public monotone reproducer that
justifies additional root pins/jobs.

Host functional evidence is 241 distinct selected Content bodies: corrected edit
selection 80, unchanged cache 5 and streamed 7 from command 06, plus filesystem 149.
The initial backed 8 bodies in 06 are superseded by corrected backed 9 in 11.
Host also has Overlay 6, Daemon 19, Workspace 36 and SDK 55, totaling 357 bodies.
The unchanged cache/stream bodies are reused; private iterator type naming does
not justify replaying unchanged functional checks. Source pins/failures retain
their original identity. Final compilation/static checks cover that alias.

Linux Content 241 bodies pass in 19. Linux ownership/adapter checks pass in 23:
Daemon 19, Overlay 10, Workspace 36 and portable SDK 12, totaling 77; the total
Linux selection is 318 bodies. The two device-capacity examples remain ignored.
The two new Supervisor slot bodies are macOS-only and are not claimed on Linux.
All-source line-cap review covers 36 changed production files, with maximum 790
physical lines; declaration modules retain the 200 ceiling. A guard is source
coverage rather than semantic/native proof.

The real-record file integration uses public Content construction/edit/read
against actual Daemon Owner jobs for a 3 MiB base. A separate byte oracle splices
the edits, memory/backed roots and counters agree, scratch survives adapter Drop,
and explicit ReleaseOperation permits automatic cleanup. Source/consumer objects
are an external memory fixture; this supplies no authenticated runtime Commit,
phase residency, cold-cache, physical I/O or performance qualification.

The prepared native proof 30 passes both profiles at the final source identity,
under unchanged 40 s Docker/45 s host deadlines and outer 120 s ceiling. The
command wall is 3,811,369,542 ns, a functional observation with uncontrolled cache
and no eligible speed comparison. Each profile retains 131 host deliveries, the
original RemoteRefusal with no resend, 13 path metadata rows/7 regular names,
exact byte/EOF/name/mode/time/raw symlink evidence, automatic idle/terminal cleanup
and 262 consumer fragments with 0 live messages/receive credits at the fence.
The seven regular names include a two-name hardlink and six regular inode
identities. The retained Linux consumer is 71,812,088 bytes, SHA256
`9db613f6f8f37bef4c6cfab9360a934a51d2e99fd01b03388d94bfc55dc76416`;
[binary pin](checks/k1-backed-stream-supervisor-20261007/consumer-binary.json) and
[raw proof](checks/k1-backed-stream-supervisor-20261007/30-real-host-linux-proof-prepared.stdout)
retain their original scope. Source/global-root topology, crash/unknown recovery,
huge acquisition/Commit, physical shrink and E/Q acceptance remain unproven.

Host Clippy 25, Linux Clippy 26 and format 31 pass. The boundary guard 32 scans
705 production Rust/SQL files; 41 tool self-tests pass in 33. These are source/check
coverage and the explicitly selected functional proof, not CI or an aggregate
pre-push gate.

Documentation check 34 validates 13 documents, 223 local links and 13 anchors.
Full staged whitespace check 36 fails only on preserved raw stdout bytes: Cargo
EOF blank lines and the existing edit model's trailing-space diagnostic labels.
Those transcripts remain byte-exact. The separate authored-file check excludes
only `.stdout`/`.stderr` and passes; this exception is no product-source waiver.

## Remaining work and qualification

Captured final-state normalization, authoritative sparse run input, backed
filesystem validation/reduction/release, root-qualified reverse-parent evidence,
single Save producer, conditional history publication, known install and unknown
custody remain required. Initial arbitrary-root qualification must be explicit;
bounded SDK root metadata checks are no full-topology certificate. Native mount/
kernel ownership, API-core/Sandbox/control/Bash integration and later campaigns
remain open. Init's eight retained speed/allocation failures and the six scoped
history-pair passes stay unchanged. E1 sample count stays 0/NOT_RUN; E01's earlier
diagnostic consistency is neither E1/E2 acceptance nor a new-source receipt.

Warning-denying host/Linux checks, the new-source real host/Linux consumer proof,
documentation links/anchors and final staged/committed source identity/LOC are
completed before this local checkpoint is committed. The full goal continues
afterward; this component checkpoint closes no milestone and retires no reference.

## Exact production source size

Production LOC: 163413 -> 165716 (delta +2303).

| Scope | Before | After | Signed delta |
| --- | ---: | ---: | ---: |
| All first-party core implementation | 97996 | 100299 | +2303 |
| Active core members within that total | 54831 | 57134 | +2303 |
| Excluded core predecessors, including retired Server source | 40321 | 40321 | 0 |
| Excluded core integration source | 2844 | 2844 | 0 |
| Root v0.1.6 reference | 65417 | 65417 | 0 |
| Combined product implementation | 163413 | 165716 | +2303 |

The common editor's state relocation is included on both sides. Growth adds
exact tagged records/codecs/acknowledgement witnesses, streamed input and owning
service conversion/custody. No reference/predecessor is deleted, activated or
credited as an algorithmic simplification. No application-adapter production
source exists outside `core/crates` for this commit. Tests, examples, diagnostics,
tools, docs, manifests, lockfiles and third-party code contribute no headline LOC.

[Exact snapshot count](checks/k1-backed-stream-supervisor-20261007/production-loc-source.json)
uses `git archive SNAPSHOT crates core/crates` and
`python3 tools/production_loc.py --root ARCHIVE --json` on the first parent and
staged tree. The unchanged counter SHA256 is
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`.
It counts nonblank/non-comment Rust and shipped runtime SQL, excludes inline/
test-only code and retains all first-party excluded implementation.
[Per-snapshot membership classification](checks/k1-backed-stream-supervisor-20261007/production-loc-classification.json)
uses that same per-file counter and each snapshot's actual Cargo member list.
The final staged comparison is recomputed before commit, then confirmed against
the resulting commit without including the two separately owned untracked notes.
