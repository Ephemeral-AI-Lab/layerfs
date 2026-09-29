# #273 → #264: focused completion prompt for the next agent

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Owned, clean branch `codex/issue273-finalize-owned`, source
> `60a043dd7ec48a9526227e8d6c6a8610ebfaa88b`; product last changed at
> `f00644479a9b7dfe0b74e02438eed6d60e30dc0a`. Phase 4.5 implementation
> already exists. This is a **pre-merge completion** assignment, not permission
> to merge, approve a release or promote any old numeric row.

## Paste this prompt to the next agent

You are the sole construction worker for the owned #273 → #264 **pre-merge**
lane. Work from `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-273-finalize` on
`codex/issue273-finalize-owned` (first run `pwd`, `git status`, `git rev-parse
HEAD`; do not infer the directory from another checkout). Read repository
`AGENTS.md`, `core/AGENTS.md`, the relevant Core architecture/component
contracts, benchmark and release rules, and the linked source-pinned records
below. Keep this branch isolated; do not alter other owners' worktrees or PR
heads. **Stay focused on resolving the three ordered gates below, plus their
required regression/custody proofs. Persist through diagnostics and fixes;
do not end at a promising prototype or a partial receipt.**

**Never declare completion merely because a local diagnostic passed.** If a
necessary owner format/profile decision or unavailable independently verifiable
host capability is the *actual* stop condition, ask for that exact decision
with source evidence, work any independent engineering parts meanwhile, and
report it explicitly. Do not fabricate approval, relax limits, change the
workload/oracle, replay performance until it passes, or report INELIGIBLE
numbers as PASS. Preserve every failed and partial attempt append-only.

### Non-negotiable owner boundary

[#252 permanently removed the direct Workspace range-edit entrypoint and the
FUSE ioctl](../252/REPORT.md). **Do not reintroduce either, under any name,
feature, private test bridge or SDK façade.** `WorkspaceApi::exec` must keep
executing *any caller-supplied command* via the ordinary mounted POSIX/FUSE
path. An optimization may be conditional on verified bytes/ownership, but it
must be implemented behind generic file reads/writes and canonical SaveFile,
not selected by command text, test case, benchmark identity or magic fixture.
A generic ordinary `dd` copy of Base bytes from a later offset to an earlier
one must not become unstageable merely because the optimization matched.

### Gate 1 — bounded lowering, correctness first

Current **FAIL**: `core/target/issue273-next-stage-lowering-01/result.json`
(SHA256 `600737f01fca1a82ae92f49a009ee88b7bc8c7bc6a2deb69c797720f22587e67`).
Its native helper copies almost the whole 64 MiB tail with fixed-offset
writes; it reaches `Backing(Acquire, StorageFull)` *before Stage* under the
unchanged 64 MiB Workspace private quota. Do not call Stage the source of this
FAIL. Current `WorkspaceApi::exec` already runs ordinary FUSE mutations; it
is not a hidden range-edit carrier.

The [generic-path investigation](LOWERING-GENERIC-EXEC-RESEARCH-20260929.md)
contains source and raw local diagnostic locations. One **unmerged, reverted,
dirty** read-provenance experiment passed the original native Stage case and
a new independent full-byte oracle: exactly **67,108,662 final bytes**, with
**180,224 actual charged physical bytes**, unchanged **67,108,864-byte
quota**, **35.529 s / 60 s** functional command wall. **This is not a clean
product PASS.** The local-only patch is `core/target/issue273-generic-experiment-unmerged.patch`
SHA256 `f7f4c5089ebf1a2d061899da6b9e16e58e0a75781468c4ad005d62fe7b0c1ec7`.
Study it as research, **never apply it verbatim**: the next public SDK Exec
falsifier did a valid out-of-order FUSE `dd` copy and Commit failed
`Preparing/KnownBeforeCommit/InvalidInput`. C1's existing SaveFile parser
accepts retained Base origins only in forward order. An attempted recursive
`ReadFile` during the SaveFile upload then deadlocked behind the daemon's
single authenticated-transport mutex and timed out with `Unknown` at the
original bound. Both failure logs remain; do not hide or retry them.

**Resolve the generic contradiction.** Investigate and prospectively specify
either (a) a version/compatibility-reviewed **internal, authenticated C1/
Bridge SaveFile** means for the Service itself to read bounded backward or
duplicated canonical Base ranges while building the same exact final bytes,
without daemon-recursive RPC or a file-size-proportional spool; or (b) a
**provably bounded, charged private frontier escrow** compatible with the
current forward-only C1 grammar, with old-pin and failure custody. The
[owner decision request](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5883171583)
is outstanding: check for a new ruling before modifying the wire/format.
Neither route licenses a public range API or ioctl. A result-size-sized cache,
silent non-monotone SaveFile refusal after acknowledged WRITE, new timeout or
quota, and dependence on one chosen shell command are unacceptable.

Prove **both** at a *new clean product/test source*: (1) the registered native
`linux::stage_lowering` with exactly the three ordered edits
`[100,110)→abc`, insertion `12345` at 200, deletion `[500,700)`, unchanged
64 MiB prepared fixture, independent full-byte saved-C1 oracle, replacement
count 8, 64 MiB quota, 10 s Stage deadline and 60 s complete command limit;
(2) a real public SDK `WorkspaceApi::exec` running an arbitrary ordinary
FUSE read/copy/write that **reorders** immutable Base bytes, then a known
successful SDK Commit and independent full exact old/new bytes. Verify
physical `st_blocks*512` against charged backing, pin/refund and error
custody, plus nonmatches, concurrent/pinned old G1, later G2 writes and
changed root/identity semantics. Do not use a Stage-only or in-process test
as a substitute for SDK Exec. The dirty SDK diagnostic also found `Io` for a
32 KiB pinned SDK read on an otherwise unmodified 512 KiB file while 16 KiB
worked: retain it as a separately scoped unresolved API observation, never
silently assert 128 KiB reads work.

### Gate 2 — actual headroom completion escrow

Current **FAIL**: `core/target/issue273-next-stage-headroom-01/result.json`
(SHA256 `847c7e40964ddd6e41c2256c906b1117fc4579e2212542d0596047387aaca97c`):
`Backing(Allocate, StorageFull)` during Stage with the unchanged **2 MiB
quota** and deliberately occupied ordinary disk headroom. Design and prove
real completion escrow, including precharged ownership, transfer from reserved
to allocated, refusal before publication, verified physical accounting,
refund and failure/unknown custody. No arbitrary release of a charged owner,
no speculative refund of pinned data, and no quota increase. Otherwise obtain
an explicit **owner disposition**; lack of a reply is not a waiver. Retain the
original FAIL and run the registered `linux::stage_headroom` at a distinct
clean fixed identity. Re-run affected G1/G2, mounted WRITE/Commit, lost
result, cleanup and known-C1/local-C5 cases after any product change.

### Gate 3 — numeric method and matched admission

Nine historical numeric rows remain **INELIGIBLE**. The frozen matched control
is **NOT_RUN**. Before a new speed/resource selection, independently verify
an enforceable and symmetric private/VM/backend/device/**host cache** contract
and a **phase-local** resource observer. Docker Desktop accepting a write to
`memory.peak` without resetting its lifetime value does **not** provide a
phase-local peak; #283 defers only that host observation, not the memory gate,
a LayerFS leak finding or numeric-profile approval. If that host cannot supply
proof, obtain an explicit owner numeric-profile ruling or use an approved
capable host—never call the old nine rows PASS. Freeze identities and method
prospectively, then run one sample per registered case/arm, the distinct
frozen control, required independent byte verifiers and complete cleanup.
Same source, host, cache enforcement, limits and measurement boundary must
be recorded for both arms; no warm-credit, best-of, extra workers, lifted
limits, rewritten receipt or replay disguised as a new sample. Release-only
locked binaries for Core SDK Init. Report raw and ineligible results even if
numerically attractive. **Do not merge while numeric admission is unproved
without an explicit owner ruling.**

### Cross-cutting verification and honest stop condition

The earlier token diagnostic, checked-release loss, live SDK deadline,
known-C1/local-C5 held lease and fixed-16-MiB response Budget have scoped
clean-source **functional PASS** receipts pinned in the
[ordered handoff](PREMERGE-ORDERED-LEASE-FIXES-20260929.md) and
[latest functional handoff](PREMERGE-C5-BUDGET-FOLLOWUP-20260929.md).
They do **not** establish new-source regression PASS after product edits or
numeric admission. SDK stopping/cancellation is still **NOT_RUN**: add a
separate deterministic live SDK stopping/refusal proof or obtain an explicit
owner disposition; a deadline or lost-release test is not its substitute.
The old unlogged token FAIL's precise cause cannot be reconstructed. #256 many-file
scale and #270 pure-move C1 Commit remain deferred **NOT_PROVED**, not
silently passed. Re-test all impacted Core SDK, daemon, Workspace, FUSE and
C1/Service paths (including old/new pinned bytes and checked custody) at the
final product source; obtain an owner decision if a *new* blocker conflicts
with the contract.

For **every commit** calculate first-parent and exact staged-tree production
LOC using `python3 tools/production_loc.py --json --root <snapshot>`;
record reference, Core and combined before/after and signed delta even on
test/docs-only commits. The current unchanged production reference is
**65,417**, Core **70,670**, combined **136,087**. A production algorithm,
format or boundary change updates its `core/docs/architecture/` source
description in the **same commit**, advances its source pin, and respects the
999-line implementation and 200-line `lib.rs`/`mod.rs` caps. Product `src/`
contains no test hook, inline test, fake or test feature. Never patch, fork
or vendor third-party code, add Workspace fsync, change the single
construction worker or disable ARMv8 AEAD flags. Use locked worktree-local
Cargo targets. For host Clippy use `cargo +1.85.1 clippy --manifest-path
core/Cargo.toml --locked --workspace --all-targets -- -D warnings`—**not**
`RUSTFLAGS=-D warnings` alone, which overrides mandatory repo-root aarch64
build flags and already caused a separate failed check. Run locked Core tests,
examples, fmt, product-boundary scanner, boundary self-tests and all affected
external Linux/Docker routes at frozen source. This repository has **no CI**
and retired `tools/preflight.sh`: claim neither. Keep commands bounded under
three minutes each; break a large proof into independent components instead
of stretching a timer or deadline.

Retain each original FAIL, dirty diagnostic and new result under fresh
append-only paths with source/product/harness/image hashes, commands,
observations and cleanup. `core/target/` is **local-only gitignored evidence**;
never present it as a published GitHub raw-evidence URL. Publish a committed
source-pinned report and owner progress/ruling request without merging PRs or
closing issues until the actual gates are solved. On a genuine external stop,
name the missing decision and next executable step, preserve clean code and
failures, and **do not label the pre-merge lane complete**.
