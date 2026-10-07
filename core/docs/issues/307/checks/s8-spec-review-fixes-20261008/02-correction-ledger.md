# S8 specification review: six corrections

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Owner request on 2026-10-08: apply all six findings from the review of
`77cf51686552ab6999d722827afb4f43032ac6ee`. This ledger records corrections to
the proposal and its companion plans. It closes the documentation findings;
it does not close an S8 implementation, functional or performance gate.

The first parent is `ac576a5fb1a58eb46380e711a4bfb3006b8435fa`, the concurrent
owner-rulings checkpoint. Its P-1 through P-7 rulings and companion edits are
preserved. No numeric limit, kernel profile, benchmark verdict or owner waiver
is changed by this correction. Product inputs remain identical to that parent.

## 1. Findings and dispositions

| ID | Defect in the reviewed proposal | Corrected rule | Owning specification | Prospective proof |
| --- | --- | --- | --- | --- |
| R1 (P1) | Quiescent could release an Exec while pipe data, a pending chunk, shell status or a send still had an owner | ResourceTerminal requires root reaping, empty group, stdin disposition, both output EOF/disposal outcomes, all pre-completion record dispositions and stopped I/O owners. Quiescent is one independent observation. Final notification custody is separate; no send is proof of host consumption | [§5.4](../../S8-SPECIFICATION-20261008.md#54-exec-process-group-and-streams), I-12 | FP-7, FP-30: fast exit, slow consumer, final chunk after EOF, lost connection and explicit incomplete-output disposal |
| R2 (P1) | Counter-only FORGET discarded independent kernel inode custody; removed cwd/O_PATH attributes depended on reclamation timing | Use indexed mount/incarnation/inode counts backed by S6 lookup leases, independent open/processing/captured owners, atomic positive-entry acquisition and exact checked decrements. Resident windows stay bounded; owning SQL and O(K) retirement work are counted | [§5.3](../../S8-SPECIFICATION-20261008.md#53-lookup-references-and-open-handles), D-4/D-6/D-8 | FP-28, FP-29, FP-31; H-4/H-8: exact metadata after removal, partial/batched decrements and drain-qualified bounded retirement |
| R3 (P1) | Forced unmount could race a running Commit and retire state after receiver-loop exit while service, owner or Store consumers were still active | Refuse force before effects while a namespace control producer is active. After terminal stop, drain all native callbacks, service jobs, owner completions, publication tickets, Store consumers and Exec I/O before native-group revocation/Close. Preserve original known/unknown outcomes; a run() error is not join evidence | [§5.1](../../S8-SPECIFICATION-20261008.md#51-workspace-and-mount), [§11.2](../../S8-SPECIFICATION-20261008.md#112-probe-forced-stop-and-complete-drain), I-14 | FP-21, FP-23, FP-32; H-15: active Commit at each phase and attempted work surviving loop exit |
| R4 (P1) | Normal unmount injected ENOTCONN before the kernel could return EBUSY, contradicting the usable-before-effect Busy result | ProbeUnmount freezes control admission while filesystem admission/service continues. One plain detach attempt; EBUSY withdraws the control probe without terminal filesystem errors. Known detach establishes the terminal native phase | [§11.1](../../S8-SPECIFICATION-20261008.md#111-admission), [§11.2](../../S8-SPECIFICATION-20261008.md#112-probe-forced-stop-and-complete-drain) | FP-20: external cwd/kernel reference and concurrent reads/stats/mutations across the probe |
| R5 (P2) | MNT_FORCE was treated as abort-only, followed by a second unconditional unmount even though the first call could already detach | Bind a connection-specific fusectl abort descriptor at Attach. Force uses one abort write, local consumer disposition, then exactly one plain detach attempt. Missing abort capability refuses before effects. Failed/unknown abort or post-abort detach retains its original effects and custody; no fallback/retry | [§11.2](../../S8-SPECIFICATION-20261008.md#112-probe-forced-stop-and-complete-drain), [§11.3](../../S8-SPECIFICATION-20261008.md#113-outcomes) | FP-23, FP-33; H-19: exact syscall counts, success and partial/error outcomes, and no normal-Busy label after abort |
| R6 (P2) | A promised pre-read credit gate was unavailable in pinned fuser, and waiting before callback return contradicted the claimed loop model | Gate at callback entry before copying payload or submitting work. Charge N fixed receive slots separately from R admitted handoffs; held native units are at most R+N. This capacity-only wait is an explicit exception, woken by credit or terminal phase; admitted owner/Store waits park off-loop | [§6.2](../../S8-SPECIFICATION-20261008.md#62-admission-and-backpressure), I-8 | FP-34, H-10: exhausted handoff capacity, no uncharged copy, terminal wakeup and later cross-Workspace progress |

These corrections are carried through the
[deepest-file plan](../../S8-IMPLEMENTATION-PLAN-20261008.md),
[proof plan](../../S8-PROOF-PLAN-20261008.md),
[mechanism ledger](../../S8-MECHANISM-EVIDENCE-20261008.md) and
[C1 handoff](../../HANDOFF-S8-IMPLEMENTATION-20261008.md). Indexed lookup custody,
basic complete Exec disposition and normal drain-qualified ownership retirement
belong in C1 because its first correct unmount already needs them. Mutation,
forced/adversarial and cross-Workspace extensions retain their checkpoint scope.
FP-30 through FP-34 and H-19 are new prospective rows; existing rows retain their
identifiers and have corrected requirements, not new measured verdicts.

## 2. Source basis and limits

- The pinned [fuser receive loop](../../../../../vendor/fuser-0.18.0/src/session.rs)
  reads before invoking callbacks and exposes no pre-read admission hook. The
  same file's run() uses early error returns during spawning/joining, so an error
  alone cannot establish every loop's exit. No new third-party patch is proposed.
- [S6 lifetime](../../S6-LIFETIME-CONTRACT.md), the existing
  [lookup owner](../../../../../crates/layerfs-overlay/src/lifetime/lookup.rs)
  and the [fuser forget contract](../../../../../vendor/fuser-0.18.0/src/lib.rs)
  establish independent lookup/open custody. The native aggregate/index/group
  wiring is new implementation scope, including explicit schema/version review.
- The [Store Commit composition](../../../../../crates/layerfs-daemon/src/store/commit.rs)
  runs independently of FUSE receivers. Loop shutdown cannot cancel its
  construction, Save, publication or installation.
- Linux v6.12's [fusectl abort handler](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/control.c#L31-L42)
  aborts the connection without unmounting. Its write return alone does not prove
  loop exit or mount detach. [MNT_FORCE](https://github.com/torvalds/linux/blob/v6.12/fs/namespace.c#L1773-L1814)
  continues into unmount processing, which is why it is not the selected
  abort-only step. Actual mounted-kernel/capability qualification remains future
  work; no native reproduction was run for this document correction.

## 3. Historical evidence and verification scope

The original [review disposition ledger](../s8-specification-20261008/05-finding-ledger.md)
and all original reviewer reports/receipts are unchanged. This correction
supersedes its counter-only dispositions at K-c8/K-e8 and related C-H4 handling,
its incomplete run()/terminal-sequence interpretations at K-c4/K-e10, and its
choice of abort mechanism at K-e17. It also supersedes the reviewed proposal's
Quiescent release, terminal normal-probe fence, blanket read-only/zero-SQL count
claims and implicit pre-read admission gate. The original source observations
and historical performance verdicts remain recorded at their original identities.

Checks for this checkpoint cover Markdown links/anchors, status/claim scope,
proof identifiers, retained owner rulings, protected notes, unchanged prior
receipts, whitespace, exact changed-file scope and first-parent/staged production
LOC. They do not claim Rust compilation, runtime correctness or performance.
No product, dependency, harness or architecture-as-implemented file changed;
Rust builds, tests, Clippy, native mounts and measurements are inapplicable to
this documentation-only checkpoint. Durable execution remains
`NOT_RUN — disabled by owner until explicit reauthorization`.

No container, process, worktree or remote issue was changed. The three protected
untracked notes stay unchanged and unstaged. Production migration/retirement is
none; exact subtotals are recorded with the commit and its LOC receipt.
