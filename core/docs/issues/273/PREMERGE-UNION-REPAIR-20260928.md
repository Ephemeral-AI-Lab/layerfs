# #273 → #264 semantic union repair: retained red suites closed, admission not complete

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> This is a new source-bound addendum, not a rewrite of the earlier [partial
> checkpoint](PREMERGE-FINALIZATION-PARTIAL-20260928.md) or any receipt. First
> parent `f54bbb1231eed71deabe880cae162613e7333ca7`, first-parent product
> tree `8a0127da4fef46ef08261a5ef862d8536039356b`; the new source changes
> are in the same commit as this document. Nothing was merged into a GitHub PR,
> and the registered numeric campaign was not run.

## Code and reasoned dispositions

The active view's identity-relative canonical queries were already ported in
`f54bbb123`. The active route now permits charged, identity-relative materialized
paths through 65,536 bytes / 256 components: existing 4,096-byte inline Node
paths remain intact, an extended locator is allocated and Budget-charged only
when needed. A rename prepares all affected resident paths and charges before
its one index publication; errors discard preparation. A depth-increasing
move checks never-resident inherited descendants against the component bound;
a same-depth longer name no longer forces an all-descendant path-length walk.
Canonical C1 paths are **still bounded at 4,096 bytes**. The two 4,097-byte
active moves return to 4,096 before Commit; no canonical format changed.

A retained child pins the resident ancestry it needs after intermediate
`forget`s; mark/sweep operates on the charged Node table with no uncharged
persistent registry. Rmdir/replacement detaches a held directory (readable
through a previously opened handle, not mutable or lookuppable by its removed
serial). An old handle keeps its pinned name view but reports current `..`
parent following a successful directory move.

The original #269-era test assertions were **not deleted**. Two expectations
belonged to the retired RootOwner mutation route and were transferred to the
active route's physical owners, with stronger checked observations:

- The private disk refusal uses a prospectively stated **4,095-byte** allowance
  above the measured no-active-pages baseline (the next active index page is
  4,096 bytes). A growing rename refuses before publication; both revision
  **and allocated physical bytes** remain unchanged. The old 8,192-byte
  allowance had admitted one active page and so could not prove that refusal.
- Rename has no RootOwner escrow: exactly one active revision and one 4,096-byte
  index page publish. Commit then creates *separately owned* canonical roots;
  their observed slots and reservation remain held until checked `close_clean`.
  The test checks their full refund after clean close (slots **0 → 26 → 0**,
  reserved bytes **0 → 262,144 → 0**, and physical canonical page allocation
  returns to the before value). It does not misattribute the Commit root's
  reservation to the earlier rename.

No test budget was raised, no source was relocated out of the product count,
no RootOwner mutation route was resurrected, and no benchmark arm was taken.

## Functional results at the staged source (new proofs, not prior receipts)

Host macOS Darwin 25.4 arm64 / Rust 1.85.1; Docker Desktop Linux aarch64,
`rust:1.85.1-bookworm` ID `sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`,
private ext4 volume at `/data`, privileged `/dev/fuse`, `SYS_ADMIN`, `MKNOD`;
`LAYERFS_TEST_BACKING_ROOT=/data`, `TMPDIR=/data`,
`LAYERFS_STAGE_TEST_ROOT=/data`, `LAYERFS_CONSTRUCTION_WORKERS=1`,
Cargo target `core/target/issue273-linux-owned` (worktree-local). Linux command
`cargo test --offline --manifest-path core/Cargo.toml --locked -p layerfs-sdk --test inherited_workspace -- --nocapture`:
**11/11 PASS**. Included never-resident and resident >4,096 moves and
move-backs, canonical old/new Commit heads, symlink by serial, aliases,
replacement, cycle, detached-parent refusal, selected G1/G2 bytes, 4,095-byte
quota refusal and checked clean refund. The preceding same-worktree attempts
at the changing source were 4/11, 8/11 and 10/11; none was relabelled PASS.

On the same Linux ext4 profile, direct
`cargo test --offline --manifest-path core/Cargo.toml --locked -p layerfs-workspace --test readable -- --nocapture`:
**18 PASS, 0 FAIL, 2 ignored** (the two ignored mounted FUSE/notifier cases
require their registered route drivers); `-p layerfs-sdk --test agent_route`:
**3/3 PASS**. MacOS locked package tests:
`cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-workspace`
**17/17 PASS**; same command `-p layerfs-sdk` **9/9 PASS**.
`cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace --all-targets -- -D warnings`,
`cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check`,
product boundary scanner, its nine self-tests, `git diff --check`: PASS after
final-source checking; no CI or retired aggregate preflight was invoked.
The Linux test containers and private volumes were removed; local Cargo targets
were retained for incremental builds, not used for a performance claim.

## Per-commit production LOC

The first parent and final staged tree were counted using the same
`python3 tools/production_loc.py --json --root <git-archive-snapshot>` on
`git archive HEAD` and `git archive $(git write-tree)`. Core
**70,408 → 70,582 (+174)**; reference **65,417 → 65,417 (0)**; combined
**135,825 → 135,999 (+174)**. Tests/docs do not count. The source adds charged
extended-path and ancestry handling; it is not a speed/space claim.

## Still-gating matrix / no merge authorization

| Requirement | Status on this new source |
| --- | --- |
| #269-era SDK union 11, readable 18 (two mounted ignores), agent 3 | PASS with the explicit Linux ext4 profile above; ignored mounted cases **NOT_RUN** through their route drivers. |
| Whole step-8 Linux route-driver matrix, independent tiny/Base/Zero/Payload/slot epochs, handles/aliases, known C1+C5 failure/unknown physical cleanup | **NOT_RUN** on this source. Targeted inherited G1/G2 and quota/close cells alone do not fill it. |
| Original #248 public 4,097-write functional gate with anchored C1 edit-load zero | **NOT_RUN** on this source. Earlier source-bound functional results do not become new proof. |
| Clean-tree rebuilt daemon image and live SDK lease route | **NOT_RUN**. Existing SDK-lane image is a different source. Deadline/Budget/known C1+C5/uncertain release falsifiers remain **NOT_RUN**. |
| Broad locked Core all-packages test | **NOT_RUN** on this tree; earlier first-parent aggregate timed out and cannot be promoted. Scoped package checks above are not full admission. |
| Common VM/backend/device cache state and phase-local peaks | **PARTIAL/falsified** per earlier checkpoint; `memory.peak` reset unchanged, privileged observer unsealed. |
| Registered SDK clean/one-edit controls, matched control/product numeric campaign | **NOT_RUN / INELIGIBLE**; historical nine numerical rows remain INELIGIBLE. |
| #256 many-file/package scale and #270 pure-move path-local C1 Commit | **NOT_PROVED**. |

Live PR heads/dispositions and the #276 owner comment were unchanged when this
work started; see the [partial checkpoint](PREMERGE-FINALIZATION-PARTIAL-20260928.md).
**Recommendation: NO MERGE for #262/#272/#274/#263/#269 yet.** Closing the
retained semantic tests removes the *first* blocker, not the step-8, SDK,
capability, numeric, release or owner authorization gates. Do not close #276.
