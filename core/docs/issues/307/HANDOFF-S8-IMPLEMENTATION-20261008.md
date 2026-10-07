# Dispatch prompt: S8 implementation, checkpoint C1 (native floor)

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Prepared 2026-10-08 on local `main` at `32d969776` (product source pin
> `f0797c646`). This is an assignment only when the owner dispatches it.
> Preparing this file started no agent, product implementation, build, test or
> measurement.

---

You own the first S8 implementation checkpoint for LayerFS: the smallest
genuine native vertical slice. An installed sealed Store, the existing overlay
Owner and the public Workspace are attached to a native FUSE mount with exact
readiness; files are read and stat-ed through the mount with kernel permission
checks; `/bin/bash -c true` runs as the Bash identity; and an explicit terminal
unmount detaches the mount and joins every loop.

This dispatch authorizes product implementation of checkpoint C1 in `core/`,
its external tests and examples, locked builds, the scoped checks and native
functional proofs listed below, the affected architecture documents, and local
commits. It does **not** authorize checkpoints C2–C6, any timing or measurement
campaign, any ranked optimization candidate, S10 work, a new dependency beyond
the recorded lockfile growth, third-party edits, remote issue edits, push,
release, deployment, a new worktree or another user-facing chat. Continue
independent work when one decision needs the owner; ask only for choices that
are theirs.

## 1. Checkout and preserved state

Work in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` on local `main`.

| Identity | Pin |
| --- | --- |
| Product source commit | `f0797c646d82835ec922c4bb62fa2374ec8843cf` |
| Product source tree | `461a53713b35e44dd62793375acad6ebe7359582` |
| Specification parent | `32d969776151588aec5aee1e9296658b8d00908d` |

The specification set is committed later as documentation only. At dispatch,
inspect the actual HEAD and status, compare the affected product inputs with the
pin, preserve newer work and reconcile it explicitly. Never reset or check out a
pin to erase later work.

These three notes were untracked at preparation. Leave them untracked,
unchanged and unstaged; they are neither an assignment nor an authority:

| Path beneath `core/docs/issues/307/` | SHA256 |
| --- | --- |
| `HANDOFF-PRE-S8-SERVERLESS-20261007.md` | `a7fb0474c4ed154d654659c7b0d0cc40545297498ec30792d10d9f43780d50b7` |
| `HANDOFF-S7-S9-RESUME-20261006.md` | `39ab313e0b38b1e47f6620262feabb8d94770248c9edfe77ef9dca728c1234d7` |
| `S7-S9-SPEED-TEST-PLAN.md` | `52f09b72e4d3bb3a28dbbd0b07fc8692311a346baa5e03420dd8edea67cec027` |

Preserve containers `9cf2fe345496`, `ce75ac504df9`, `d2433851ea59` and
`d2550144998b`, every unrelated process and worktree, and every historical
receipt. The experiment branch at `1451b68a720bbe2175a103dd9b35693ad05e2be1` is
read-only evidence: read it with `git show`, never check it out or move its
worktree.

## 2. Read first

1. [Root guide](../../../../AGENTS.md) and [core guide](../../../AGENTS.md).
2. [S8 specification](S8-SPECIFICATION-20261008.md): the single decision owner.
   Sections 3–6, 8.1, 9, 10 and 11 govern C1.
3. [Implementation plan](S8-IMPLEMENTATION-PLAN-20261008.md): activation,
   deepest-file matrix, checkpoint C1.
4. [Proof plan](S8-PROOF-PLAN-20261008.md): sections 3 and 4 for the C1 rows.
5. [Mechanism ledger](S8-MECHANISM-EVIDENCE-20261008.md): what is mandatory and
   what is only a candidate.
6. The #303 [mount](../303/workspace-api/mount.md),
   [Exec](../303/workspace-api/exec.md),
   [unmount](../303/workspace-api/unmount.md),
   [engine](../303/daemon-sqlite.md), [FUSE](../303/fuse.md) and
   [integration](../303/06-cluster-one-integration.md) contracts, read with the
   reconciliation table in specification section 2.
7. The retained [kernel review](checks/s8-specification-20261008/02-review-kernel-fuse.md)
   for exact kernel and fuser citations, and the
   [finding ledger](checks/s8-specification-20261008/05-finding-ledger.md).

Where a plan or this prompt disagrees with the specification, the specification
governs; report the defect rather than choosing silently.

## 3. Scope of C1

Build, in the order of the plan's C1 table:

1. Record the environment facts from the actual sandbox: kernel release, page
   size, `/dev/fuse`, the capabilities needed for `mount`, namespace entry and
   identity change, and cgroup v2 delegation. A missing requirement is an
   explicit readiness failure and is reported; nothing falls back silently.
2. Cargo activation A-1 to A-7, including the reviewed lockfile diff and the
   relocation of the excluded predecessor as its own delta-0 commit.
3. `Attach`/`Ready` and `Locate`; the native state and gauges on the existing
   registry entry; the mount session owner; the kernel profile and its
   negotiation receipt.
4. The request service in its final shape, the read-class steps with one
   read-only compound owner job each, OPEN/RELEASE on existing `OpenFile`
   custody, bounded directory cursors, FORGET as counters, and the refused
   opcodes.
5. The read service changes R-1 to R-3.
6. The launcher mode, identity drop, propagation contract, cgroup custody,
   `Exited` and `Quiescent`, with the three pipes owned under the one-chunk
   bound.
7. Normal terminal `Unmount`.

Done means all of these hold at one final identity:

- FP-1, FP-2, FP-3, FP-4, the read side of FP-17, and FP-22 pass as external
  native proofs, each deterministic with a bounded wait and an explicit stop.
- H-1, H-2, H-3, H-4, H-7, H-8, H-10 and H-15 are recorded from receipts and not
  falsified. A falsified hypothesis is reported as such with its receipt; it is
  not tuned away.
- The scoped checks of plan section 6 pass.
- Affected architecture documents describe the source as implemented.
- A dated C1 record under this directory lists what was built, every check and
  its outcome, every failure and gap, and the per-commit LOC comparisons.

Not in C1, even if convenient: mutation requests, mapped writes, stream
backpressure proofs, `ExecCancel`, `ForceUnmount`, group retirement, several
Workspaces, debt-coupled admission, any candidate from the mechanism ledger, any
timing.

## 4. What must not be built

The plan's section 5 is binding. In particular: no blocking interim adapter in
which a loop waits for an owner job or a Store read; no per-inode lookup table
and no SQL in FORGET; no second registry, private immutable cache or mutable
mirror; no use of fuser's mount helper, lazy detach or unmount handle; no kernel
notification, writeback, passthrough or permission removal; no timeout, implicit
Commit or automatic unmount around Exec; no writer gate, wait or retry around
the Store session; no port of the excluded predecessor's request model.

## 5. Owner choices and how C1 treats them

None blocks C1. Do not resolve them yourself.

| ID | Choice | Treatment in C1 |
| --- | --- | --- |
| P-1 | errno and early refill when a create meets Store `Busy` at serial exhaustion | Not reached: C1 has no create |
| P-2 | no-new-privileges for Exec children | Implement as an explicit configuration value with no hidden default; run C1 proofs with it set as the specification recommends and state that the owner has not yet ruled |
| P-3 to P-5, P-7 | Performance budgets, controls, residency proof, historical cases | Not reached: C1 takes no timing |
| P-6 | Varying `max_background` | Not reached: the promoted value is used |

If the actual environment cannot supply a C1 requirement (for example no
delegated cgroup v2 subtree), stop that part, report the exact observation and
continue with whatever does not depend on it.

## 6. Execution rules

- Explicit Disposable/WAL/OFF only; Durable is
  `NOT_RUN — disabled by owner until explicit reauthorization`. The overlay
  stays MEMORY/OFF/EXCLUSIVE. No `fsync`, `fdatasync`, `sync_data` or `sync_all`
  on Workspace backing.
- No new dependency where a locked crate supplies the capability. No
  third-party edit except the recorded fuser 0.18.0 signed-timestamp patch; run
  `python3 -B core/tools/check_fuser_integrity.py` before the first native
  build. Docker verification is accepted for that correction; the Linux
  fractional signed-minimum result stays a recorded platform limitation.
- Export `LAYERFS_CONSTRUCTION_WORKERS=1`. FUSE dispatch concurrency is the
  specified two loops per mount and `K` service workers, never a hidden helper.
- Root-owned execution, one Cargo, test or native proof at a time. Build first
  with locked Rust 1.85.1 from the repository root and its ARM64 inputs; an
  explicit `RUSTFLAGS` repeats that profile.
- Every test command gets an explicit 100 s wall stop (ceiling 120 s): host
  `perl -e 'alarm shift; exec @ARGV' 100 <binary>`; Linux
  `timeout --kill-after=1s 100s <binary>`. A test that reaches it has failed as
  a hang: diagnose from source and bounded output before a changed rerun. Never
  loop or background a test to wait it out. Every spawned peer exits even after
  another panics.
- Linux uses bundled SQLite, the host uses system SQLite; record the actual
  versions. Retained image:
  `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
  Cargo target and cache stay worktree-local. Store files live on a named in-VM
  volume or a native container filesystem, never under the repository bind
  mount. Compare source hashes before blaming a stale bind-mount view.
- One attempt per operation. No automatic retry, readiness polling sleep,
  failed-operation replay, guessed cleanup or inferred success. Preserve the
  original failure and unknown custody.
- Product source only in `src/`; proofs are external. New product files at most
  999 physical lines; every `lib.rs`/`mod.rs` at most 200 and declarations only.
  No test-only hook and no benchmark-selected behaviour. No CI or preflight
  wrapper.
- Functional proof only. Any number observed in passing is a diagnostic and is
  never reported as a speed result. No historical receipt is rerun, regenerated
  or relabelled.
- Every local commit records exact first-parent, staged and committed production
  LOC with Core, active, reference, excluded predecessor and excluded
  integration subtotals, using `tools/production_loc.py` (SHA256
  `c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb` at
  preparation; a changed counter is stated). Label relocation and replacement
  honestly. Commit messages end with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## 7. Report back

In the main chat: the commits with their LOC lines; the actual HEAD and
working-tree state; what C1 built against the plan's matrix, with every
deviation and its reason; each proof and hypothesis with its outcome and
receipt path; every failure, hang, gap and unverified environment fact; any
specification defect found; and the owner choices now needed for C2. State
plainly what was not run. Do not start C2, message another chat, push, or edit a
remote issue.
