# R0 owner direction, correction and proof disposition

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Input HEAD `1a6bb53ef14e1860d8f222df11394e5a654bb34d`; 2026-10-08.

This prospective reconciliation is authorized by the fresh R0–R9 owner dispatch.
No product code, kernel profile, old proof receipt/verdict or numerical gate is
changed. Required SDK organization is ProjectApi/WorkspaceApi/SandboxApi;
ordinary Sandbox/runtime/external executor owns commands and standard streams,
exit/status/cancellation. Optional WorkspaceApi.exec chooses mounted cwd and
only delegates. FUSE admission/mutation/capture requires no execution identity.
Daemon owns filesystem/Commit work and complete drain; no supervisor/launcher,
per-Exec cgroups, registration or custom Exec protocol. Force never kills callers.

## Retained genuine filesystem corrections

The historical [six-correction ledger](../s8-spec-review-fixes-20261008/02-correction-ledger.md)
stays byte-identical. Its original R1 daemon ResourceTerminal mechanism is
withdrawn. The six filesystem obligations below arise from R2, the two parts of
R3, R4, R5 and R6; all remain mandatory and prospective, not new PASS claims.

| Obligation | Historical row | Current required proof |
| --- | --- | --- |
| Indexed lookup count/lease custody, exact removed-inode metadata, checked decrements | R2 | FP-28/29/31 and H-4/8; independent lookup/open/processing/captured owners |
| Full daemon-work drain after receiver-loop exit | R3 drain | FP-21/32 and H-15; receiver joins alone never authorize Close |
| Refuse active namespace control producers before force effects | R3 admission | FP-23-FS/32; no abort/detach before refusal |
| Reversible normal-unmount probe with ordinary FS service | R4 | FP-20; EBUSY restores control admission with no terminal syscall error |
| One connection-specific abort and one plain detach with exact custody | R5 | FP-23-FS/33 and H-19; after abort EBUSY is Retained/aborted-but-mounted, external caller may survive |
| Actual callback-entry admission and separate R+N receive/handoff accounting | R6 | FP-34/H-10; no pre-admission payload copy/owner job, terminal credit wakeup |

Runtime buffered output/EOF/status/descendants remain independent correctness
requirements under actual runtime ownership. They are never filesystem unmount
admission or daemon drain conditions. Caller cleanup/cancellation stays separate.

## Prospective withdrawal and replacement

Original unexecuted rows are preserved exactly in
[original-withdrawn-proof-rows.json](original-withdrawn-proof-rows.json), against
the original file hash in [input hashes](02-original-document-hashes.json).
Historical experiment/component receipts retain every ID, pin, status and failure.
These dispositions do not re-register, execute or relabel any timing selection.

| Original prospective ID | Disposition and successor |
| --- | --- |
| FP-5 | WITHDRAWN daemon launcher/three-pipe assertion; FP-5-Runtime proves actual access/identity/visibility/protected descriptor setup |
| FP-6 | WITHDRAWN daemon custom stream selection; FP-6-Runtime proves ordinary stream order/completion/backpressure/no cap |
| FP-7 | WITHDRAWN daemon ResourceTerminal/gauge conjunction; FP-7-Runtime proves independent status/stream/filesystem observations |
| FP-19 | WITHDRAWN ExecCancel→Exited selection; FP-19-FS proves parked syscall disposition; explicit cancellation is runtime-owned |
| FP-22 | WITHDRAWN launcher topology assumption; FP-22-FS proves actual runtime/external mount visibility, normal Busy and separate mount drain |
| FP-23 | WITHDRAWN process-signalling/terminalization selection; FP-23-FS retains active-control refusal, exact abort/detach/drain/custody |
| FP-25 | WITHDRAWN combined Exec/control selection; FP-25-Routes retains original Mount/Attach/Locate custody and separate ordinary-runtime lost-result ownership |
| FP-30 | WITHDRAWN daemon output/ResourceTerminal/unmount conjunction; FP-30-Runtime proves fast exit/slow output/EOF/disposal independently |
| M-9 | WITHDRAWN daemon process/stream mechanism; M-9-Runtime owns actual Sandbox ordinary execution and access setup |
| H-15 | Retain ID and filesystem terminal predicate; remove old Exec result-disposal premise |

FP-1–4 use ordinary runtime/external Bash with no registration. All filesystem
rows outside the withdrawals remain required, including exact permissions,
coherence/mmap, lookup/open/cwd/O_PATH, active producer, capacity/drain and cleanup.
P-1 and P-3–P-7 original rulings are preserved. P-2 original launcher wording is
historical; actual runtime setup applies the explicit no-new-privileges/helper
barrier. No per-command cgroup readiness gate survives.

## Additional correctness findings

Pinned fuser reply methods return (). An adapter can count one explicit reply
attempt but cannot observe exact send success/failure or kernel delivery through
that API. Internal library error logging is uncorrelated diagnostic evidence.
Current spec/plan reports that limit; no third-party patch is extended.

Copied-mount Busy/detach depends on actual parent-mount propagation topology,
not merely making a FUSE child shared. Setup/executor owns visibility. FP-22-FS
must prove actual support; same-namespace access can be implemented independently.
Force still attempts one plain detach: an external cwd/descriptor/mapping may
produce EBUSY after abort while the caller survives, retaining original effects.

Captured namespace review found real R4/R5 gaps: neutral retained-reader namespace
port and changed-name points; bounded Content validation instead of complete
vectors/maps; checked incremental parent/cycle evidence without whole-base walk;
exact cross-parent native rename witness; fresh-child release correctness;
constructor reader/operation custody composed through existing Store Commit.
These are source findings, not completed implementation or measured bounds.

## Scope and preserved state

Original guide edits/hashes are saved before reconciliation. Existing execution
ownership and shared-frontier guide edits are deliberately incorporated because
they are within R0; no unrelated guide hunk exists in the original diff. Protected
untracked notes remain unchanged/unstaged. All original historical receipts,
product trees, config/lock/manifests, root reference and destination manifest are
unchanged. No Cargo/test/mount/measurement/container/process/worktree action ran.
Two authoring scripts initially failed before completing their edit batch (one
syntax error, one missing heading delimiter); both were corrected from bounded
output, with partial document state inspected. These are authoring failures,
not product tests or proof/benchmark outcomes.

R0 checks are document ownership/scope, link/anchor/whitespace, protected and
historical hashes, exact product/staged scope and LOC. Rust/runtime checks are
inapplicable. Durable execution is NOT_RUN — disabled until owner reauthorization.
