# Handoff: R3 native mutation and kernel coherence

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Work in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` on local `main`. R2 is
implemented and functionally verified; its ledger state is "IMPLEMENTED — owner
acceptance pending". This handoff assigns **R3 only**: ordinary mutation through
the native mount and the kernel coherence that goes with it. R4 keeps its earlier
component state, R5 is not started, and no R6–R9 execution or early retirement
of the root reference is authorized. Do not push, open a pull request, start
another worktree or publish anything remotely.

## Checkpoint

R2 commit `04b04ef37844697e82507bf0abdccdfc072dbee0`, first parent
`a6f90cf0feae9b2336dcb6dd84e1ec2d942af285`. Inspect HEAD and status before
editing; a documentation-only commit carrying this handoff may follow.

| Product | Git tree |
| --- | --- |
| `core/crates` | `d0f378aecf394b6e63727027584324500a09e8ff` |
| root reference `crates` | `498dd1917812ae90efb8841f57e22bfc284e96fb` |
| preserved `core/crates/layerfs-fuse-legacy` | `be11d96ec9db31b897109323379fec9f3e009dae` |

Read first: [R2 completion record](R2-COMPLETION-20261008.md),
[R2 checks](checks/r2-completion-20261008/00-results.md),
[native mount session](../../architecture/76-native-mount-session.md) and
[native request service](../../architecture/75-native-request-service.md).

## Binding instructions

Read root and core AGENTS, both root handbooks, the [303 index](../303/README.md)
and the relevant operation, engine, FUSE and integration contracts. For R3 the
governing text is the [S8 specification](S8-SPECIFICATION-20261008.md) sections
5.2–5.3, 6.4, 8.3, 8.4 and 9 with invariants I-4, I-5, I-6, I-7 and I-10; the
[implementation plan](S8-IMPLEMENTATION-PLAN-20261008.md); the
[proof plan](S8-PROOF-PLAN-20261008.md); the
[file layout](FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md); and the
[rollout contract](CLUSTER-TWO-LOC-AND-ROLLOUT-20261008.md#4-combined-implementation-rollout).

- Global Store execution is Disposable/WAL/synchronous=OFF only, selected
  explicitly. Durable is `NOT_RUN — disabled by owner until explicit
  reauthorization`. Overlay is one MEMORY/OFF/EXCLUSIVE database per daemon.
  Export `LAYERFS_CONSTRUCTION_WORKERS=1`. No `fsync`-family call on Disposable
  backing.
- Every daemon opens the global Store directly from a shared named VM volume. No
  host data server, no `layerfs-server`, no legacy fallback, no command
  supervisor, registration, automatic timeout, implicit Commit or implicit
  unmount.
- Third-party code: only the already authorized fuser 0.18.0 timestamp patch and
  scoped lifecycle extension. No other edit, fork, replacement or registry patch,
  and no new dependency where an existing crate serves. Run
  `python3 -B core/tools/check_fuser_integrity.py` before native fuser builds.
- One attempted operation. No automatic retry, busy handler or replay. Preserve
  original failures and unknown custody.
- Product source only under `crates/<package>/src/`, no test hooks. New
  production files at most 999 lines; `lib.rs`/`mod.rs` at most 200 lines of
  declarations. Fuse, Daemon, Overlay and Bridge keep `#![forbid(unsafe_code)]`.
- Every test command has an explicit wall limit of at most 120 s. Build with
  `--no-run` first. Never loop, background or output-filter a test run. A test
  that reaches its limit has failed and is diagnosed from source before any
  rerun. Component binaries here used 100 s; native application proofs default
  to 9 s, and the owner allowed 15 s for the R2 full-topology proof only.
- No CI exists and `tools/preflight.sh` is retired. Report checks and gaps; make
  no cold, timing, storage or resident-memory claim without the measurement
  workflow.
- Every commit records `Production LOC: <before> -> <after> (delta <signed>)`
  with the pinned counter and subtotals, and ends with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Commit locally on
  `main`; never push.

## What exists

**Native surface (R2).** One kernel connection per attached Workspace with a
shared dispatcher, Ready evidence, reversible unmount and connection drain. The
read path is wired: LOOKUP, GETATTR, read-only OPEN, READ, READLINK, OPENDIR,
READDIR, RELEASE, RELEASEDIR and FORGET. FLUSH, FSYNC, FSYNCDIR and STATFS answer
inline.

**Refusals R3 replaces.** In
[callbacks.rs](../../../crates/layerfs-fuse/src/request/callbacks.rs) every
mutating callback already exists, is counted, and answers `EROFS`: writable
OPEN, SETATTR, MKNOD (regular), MKDIR, UNLINK, RMDIR, SYMLINK, RENAME, LINK,
WRITE, CREATE, FALLOCATE and COPY_FILE_RANGE. The xattr family, ACCESS,
READDIRPLUS, locks, BMAP, IOCTL, POLL and LSEEK answer `ENOSYS`; MKNOD of a
special file answers `EPERM`. The mount table already reports `rw`.

**Mutation decisions (component-verified, not native).** Workspace exposes the
existing namespace and file decisions as a resumable
[MutationPlan](../../../crates/layerfs-workspace/src/mutation/plan.rs):
`prepare_mutation`, `job`, `accept`, `supply`, with `Need`/`BaseFacts` rounds.
Preparation performs no SQL and no immutable demand; only a nonempty Needs result
permits continuation. The synchronous
[driver](../../../crates/layerfs-workspace/src/mutation/driver.rs) runs the same
plan. Its [component record](checks/r2-mutation-plan-20261008/18-results.md)
covers 30 public tests on host and Linux.

**Ports.** [Fuse ports](../../../crates/layerfs-fuse/src/ports.rs) define
`MountServices` and `RequestServices` for the read path only. The
[daemon adapter](../../../crates/layerfs-daemon/src/service/filesystem_port.rs)
implements them over the real Owner and Store; `overlay/file_port.rs` and
`overlay/native_job.rs` hold the existing typed jobs.

**Not present.** No Fuse mutation handler, no `coherence/` module, no writable
native handle, no mutation service port.

## Target layout

All in `core/crates/layerfs-fuse/src/` unless stated:

| Home | Responsibility |
| --- | --- |
| `operations/write.rs` | WRITE and append at the kernel's offset; split truncate out only if useful |
| `operations/create.rs`, `link.rs` | CREATE, MKDIR, SYMLINK, MKNOD (regular), LINK |
| `operations/rename.rs`, `remove.rs` | RENAME including replacement; UNLINK, RMDIR |
| `operations/flush.rs` | FLUSH/FSYNC semantics; no Store seal, no durability claim |
| `operations/unsupported.rs` | The refusals that remain |
| `coherence/reply_order.rs` | Published frontier against reply attempts (I-6) |
| `coherence/attributes.rs` | Alias and cached-attribute coherence |
| `coherence/pages.rs` | Cached pages; install continuity is R5, not R3 |
| `request/callbacks.rs`, `attributes.rs` | Writable OPEN, SETATTR, entry replies |
| daemon `service/filesystem_port.rs` and `overlay/` jobs | Mutation ports and bounded typed jobs on the existing owner |

Do not add a second mutation engine, a second encoder or a registry in Fuse.
Control records, Bridge and SDK should not need to change for R3; if one does,
say why before changing it.

## Required behaviour

From the rollout contract: writes, append, truncate and regrow, mappings,
rename, link and unlink, removed working directory and `O_PATH`, permissions and
times are exact. Changes made by a process with no Exec registration behave
identically. No per-WRITE notification deadlock and no early ownership
reclamation. The specification's rules that carry the weight:

- **I-5.** A successful mutation reply follows local publication of all its
  effects in one atomic owner job. A failed or lost reply never undoes it.
- **I-6.** Every attribute-bearing reply is computed from published state at or
  after the request was received; never from a view older than a mutation whose
  reply was already attempted.
- **I-10.** WRITE replies the full count or an error, always at the kernel's
  offset. `Position::End` is never used on the native path.
- **One mutation transaction per request** (§6.4). Need rounds precede the
  effect. Entry-bearing successes acquire lookup custody in the same job.
- **No kernel notification** in any coherence row (§8.3). Entry and attribute
  lifetimes are never the mechanism that makes an answer correct.
- **Shared-mapping stores** arrive as background WRITE with the per-request
  cache flag, clipped to current size, without credentials, on the first
  writable handle of that inode. Accept the flag, require only a live handle for
  that inode, never change size. Mount-wide writeback caching stays unrequested.
- **Refusals that stay** (§8.4): xattr `ENOSYS` (sticky); special-file MKNOD,
  set-id bits, ownership change, and hard links to a directory or symlink are
  `EPERM`; symlink targets longer than one page minus one are refused at
  creation.

## Order of work

1. **Close the two unexecuted R2 paths first.** R3 depends on both.
   - Observe real kernel FORGET delivery and the resulting exact lookup
     decrement. In every R2 run `forget_units` was 0 because detach aborted the
     connection first.
   - Stage one unmount that ends `Retained` and confirm custody, status and the
     later reply. No test has driven those stages.
2. **Writable OPEN and handle custody.** R2 refuses it before any engine custody
   exists. Decide the processing association for a writable handle and its
   RELEASE, including the handle a mapped store will arrive on.
3. **WRITE and SETATTR** over the mutation plan: write, append, truncate, shrink
   and regrow, `O_TRUNC` (OPEN then size-0 SETATTR with no handle), `chmod` and
   `utimens`, with reply ordering under I-6.
4. **Namespace mutations:** CREATE, MKDIR, SYMLINK, LINK, UNLINK, RMDIR, RENAME
   and replacement rename, each one atomic job with lookup custody for
   entry-bearing replies.
5. **Removed references and aliases:** open-unlinked content, removed working
   directory and `O_PATH`, hard-link aliases sharing one kernel inode.
6. **Shared mappings.**
7. **Proofs, documentation and the ledger**, then stop. Do not continue into R5.

The mount accepts some mutations and refuses others until step 4 completes.
Keep each intermediate commit honest about which operations are wired.

## Proofs R3 owns

| Row | Requirement |
| --- | --- |
| FP-10 | Write and `O_APPEND`: cached read, fresh open, stat and daemon truth agree |
| FP-11 | Create, unlink, replacement rename visible at once; `ENOENT` not cached |
| FP-12 | Lookup racing CREATE/UNLINK/RENAME of that name never returns a stale binding |
| FP-13 | Truncate, shrink, regrow, `O_TRUNC`: no discarded tail byte reappears |
| FP-14 | Hard-link aliases: write through one name read from cache through the other |
| FP-15 | Shared mappings: `msync`, `munmap` without it, map–close–store–exit |
| FP-16 | Reply ordering: a GETATTR held across a WRITE is discarded by the kernel |
| FP-17 | Mutation half: mode-000 unreadable from cache, `chmod` at once, ctime after `chmod`/link/rename, symlink length limit |
| FP-18 | Refusals stay refused; after the first `ENOSYS` no further xattr request arrives across many small writes |
| FP-28 | Open-unlinked and rotation: descriptor content exact |
| FP-29 | Removed cwd/`O_PATH` survives maintenance and cache revalidation |

Use real mounts and ordinary filesystem syscalls from processes that were never
registered with the daemon. FP-20's mutation probe (requests racing the unmount
probe, mutations persisting after `EBUSY`) also becomes runnable. Rows left
partial by R2 are listed in the R2 checks; close what R3 makes reachable and
restate the rest.

## Standing risks

- **Owner decisions from R2 are still unreviewed.** Chiefly: Overlay revocation
  is fenced only by live request sources and retires unreleased handles in
  bounded maintenance. R3's writable handles will be retired by the same path. A
  contrary ruling changes that work, so do not deepen the dependency beyond what
  R3 needs. The others are listed in the R2 completion record.
- **Drain gets harder.** Dirty mappings and in-flight writes can outlive a
  command. The five-second drain deadline is an observation; expiry must leave
  `Retained`, never a guessed success.
- **Abort control was never bound** (`abort_bound: false`): fusectl is not
  mounted in the container. That is R6 work; do not build on it.
- **Re-Attach right after a failed Attach** can be refused until the revoked
  mount row retires.

## Environment

- Host: macOS ARM64. Use `cargo +1.85.1 ... --locked`. The machine default is
  1.96.0, whose Clippy fails on newer lints in the untouched `layerfs-content`;
  that is a toolchain mismatch, not a regression to fix. macOS has no `timeout`;
  wrap commands in your own bounded runner.
- Linux: image
  `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`,
  repository bound at `/work`, `CARGO_HOME=/work/core/target/cluster2-linux-cargo`,
  `CARGO_TARGET_DIR=/work/core/target/cluster2-linux`, locked Cargo 1.85.1.
  Observed kernel `6.12.76-linuxkit`, FUSE ABI 7.41.
- Native runs: `--device /dev/fuse --cap-add SYS_ADMIN --security-opt
  apparmor=unconfined`, `Privileged=false`. Store files on the container's
  native filesystem or a named in-VM volume, never under the repository bind
  mount.
- Serialize builds, tests and proofs with a nonblocking lock on
  `core/target/r2-completion.lock` or a successor in the same worktree.
- Reusable test support in `core/crates/layerfs-daemon/tests/support/`:
  `mounted.rs` (in-process Service with real mounts), `native_install.rs`,
  `complete_fixture.rs`. The full-topology proof is
  `core/crates/layerfs-api/sdk/examples/sandbox_native_mount.rs`.
- Three daemon test cases need explicit preconditions and were not run:
  `complete_installed_roots::huge_native_namespace_is_complete_after_install`,
  `shared_processes` on Linux and `host_handoff` on the host. They are unrelated
  to R3; report them as unrun rather than repairing them in passing.

## Preservation

No owned container, volume, Cargo process, test process or native mount is left
running. Do not touch these unrelated running containers:

- `9cf2fe345496b45d79ff272e6c698552948f1e92ea106926826a770592623b24`
- `ce75ac504df9d0e58cedcdd0df93fe7ef39c3b66ef3c13b0bb6b580def64516c`
- `d2433851ea5990e273b0673fdbd5c180917f1216f3c81bdfb4517da0207e2c2a`
- `d2550144998b4b71de98cb9ecf6448f87c7b99a3c2f69ea778a29413fffa5ffb`

Leave the older exited `layerfs-e04-*` containers alone. Remove only containers
and volumes you create.

Preserve these unrelated untracked files and do not stage them:
`HANDOFF-PRE-S8-SERVERLESS-20261007.md`, `HANDOFF-S7-S9-RESUME-20261006.md` and
`S7-S9-SPEED-TEST-PLAN.md` in this directory, and
`checks/multi-workspace-model-confirmation-20261008/04-commit-confirmation.json`.
Do not rewrite raw evidence. Do not touch the retained failed pressure fixture at
`/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-installed-56560-full-native-handoff`.

## Source accounting

| Checkpoint | Combined production LOC | Migration |
| --- | --- | --- |
| `04b04ef37844697e82507bf0abdccdfc072dbee0` | 176905 → 179432, delta +2527 | None; all growth in active core members |

Current totals: core 114015, active 71141, excluded predecessors 38878, excluded
integration 3996, root reference 65417. Use `tools/production_loc.py`, SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`, over exact
first-parent and final staged archives with exact active Cargo-member paths. The
method is the script form recorded in
[production-loc.json](checks/r2-completion-20261008/production-loc.json). A
documentation-only commit reports the unchanged total and delta 0.

## Done means

R3 is finished when every operation in the target layout is wired or remains a
declared refusal, the proofs above have real-mount receipts with every failed
attempt retained, the architecture documents and rollout ledger describe the
implemented state, each commit carries its LOC comparison, and a completion
record states plainly what is partial or unrun. Then stop and report.
