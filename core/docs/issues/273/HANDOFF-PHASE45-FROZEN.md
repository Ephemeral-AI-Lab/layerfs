# #273 phase 4.5 frozen functional candidate

> **Status:** Handoff, not a contract or a performance result. Steps
> 4.5.2–4.5.5 are complete at the functional/count/custody scope in the
> [append-only log](PHASE4.5-LOG.md). Frozen source
> `4ae36ad3a9c70b32b66c8280ac9496e9f1e345a9`; last product change
> `a2359620a7966314fbb2a96c98e8da958df72c6c`. Checkpoint 5 is **NOT_RUN**
> until the owner reviews this candidate. Keep PR #274 draft and issue #273
> open. No merge is authorized by this handoff.

## Identity and authority

- Worktree: `/Users/yifanxu/.codex/worktrees/issue273-active-head/layerfs`;
  branch `codex/issue273-active-head`; origin
  `https://github.com/Ephemeral-AI-Lab/layerfs.git`;
  [draft PR #274](https://github.com/Ephemeral-AI-Lab/layerfs/pull/274).
- [Implementation spec](PHASE4.5-IMPLEMENTATION-SPEC.md) →
  [research audit](PHASE4.5-RESEARCH-AUDIT.md) →
  [phase log](PHASE4.5-LOG.md), with repository and `core/AGENTS.md` rules.
  The [v1 record](ACTIVE-FORMAT-AND-EVALUATION-v1.md) retains historical
  receipts and unchanged pack v1; new index attachments select v2.
- [Frozen identity record](evidence/phase4.5/hot-publication-20260928/FROZEN-CANDIDATE.json)
  pins the source, product inputs, root `.cargo/config.toml`, locked
  dependencies, release profile, test/harness sources, executables, runtime
  images and closed fixture. SHA-addressed executable copies live under
  `core/target/issue273/binary-archive/`; verify each hash before reuse.
- One writer/Cargo target owner in this worktree. Use its `core/target`,
  locked release builds and the repository-root ARMv8 flags. Do not patch a
  dependency, run `tools/preflight.sh` or claim CI. No new dependency or
  third-party source modification was introduced.

## Implemented source to read

`backing/active/hot_cursor.rs` owns the charged selected node copies and eight
cursors, with a 64-slot/1 MiB current hot reservation ceiling. Bindings check
the selected root plus ancestor `(slot, epoch, page version)` and exact
lower/fence range; the leaf's own version does not invalidate another file's
binding. I/D/P require presence. P/R bindings are shared across cursors.

`hot_path.rs`, `splice.rs` and `index.rs` publish EOF and advancing inherited
Base/Zero writes, merge edits sharing a leaf, carry through recorded parents,
and install one matching root/directory/pack revision. Ineligible work uses
the generic route in that same revision. Generic admission/eviction performs
only necessary normalization, keeps shared Hot closure and physical birth,
and reassigns freed slots with increasing epochs. Generic publication also
refreshes shared P/R bindings when there is no tiny-WRITE seed.

`retirement.rs`, `pages.rs`, `lifetime.rs` and `reclaim.rs` use selecting-pin
cohorts and recorded physical birth. Unpinned owners receive their release
attempt in the triggering operation; final pin release visits its own
cohort. Unknown identity/allocation/unlink keeps charged custody. Candidate
and collection reservations precede allocation, and a known successful C1
outcome is never replayed after C5 fails.

`commit/active_reconcile.rs` prepares G1 dirty rows, extents and saved facts
off the state gate. Installation rechecks each live inode revision, retains
intervening G2 and transfers the charged patch rather than retaining complete
successive clones. Installation still pays affected index work under the
gate. Upload retains charged O(E_f) scratch; overall CPU/RAM is not constant.
Fresh open-unlinked files save private content without introducing an
unbound canonical inode, and continue using an installed saved base across
later Commits.

## Proof and qualifications

The [evidence catalog](evidence/phase4.5/hot-publication-20260928/README.md)
contains raw receipts, logs, source-derived diagnostics, separate failed-run
cleanup, checksums and per-commit production LOC. The final public set at
`4ae36ad3a` is the original 14 cases plus `active_cleanup_failure`,
`active_hot_publication` and `active_hot_continuity`; all 17 cases / 42 named
checks pass. Every row has `performance_claim=false`, `cache_claim=null` and
its own independent byte copy of the closed fixture. No functional wall is a
speed arm or release gate.

The locked ARMv8 release backing binary passes 32 tests on owned Linux ext4,
including growing EOF/Base/Zero, byte-balanced leaf/branch carries, both
selecting-pin release orders, eight-cursor eviction/epoch reuse with old
views, live-cache Budget refusal and reconcile scratch refusal. Other scoped
Linux suites pass: readable 18 (2 mounted-only ignored), keyed tree 1,
attachment 3, backing ownership 6, pieces sequence 36. Host release Workspace
tests, workspace/all-targets Clippy with warnings denied, fmt, the 349-file
boundary guard and its 9 self-tests pass. Native release binaries and the
Linux tests build with `--locked --offline`.

The real mounted continuity process keeps the same PID, fd and inode through
G1 SaveFile/C5, acknowledges G2 writes and continues after Commit without
pause/restart/remount. Independent old/new native roots, aliases, full
fixture bytes, held orphan handles and exact named clean-close refunds are
checked. The failed-cleanup proof deliberately retains a Published receipt,
new bytes and charged failed owner while clean close returns Busy; its
environment is removed afterward, not presented as a clean product close.

Historical nonpassing attempts remain nonpassing: stale root/terminal-fence
bugs, the initial epoch witness, public oracle/count assumptions, shared P/R
refresh, fresh orphan lowering, explicit lookup leaks, C5 duplicate patch
reservations and the obsolete cleanup injection. The temporary FileExt
compile failure is retained too. The log links each failure and its fix.

Not run: checkpoint-5 matched performance arms, cold-cache admission,
RSS/cgroup proof, concurrent SDK Exec, #264 integration and the full Core C1
test command. Preserve the two historical C1 `filesystem_ordering` failures
(19 objects against 18) until their lane resolves them. The two ignored
mounted-only readable tests are not claimed to pass. No CI/preflight result,
latency PASS, 2× improvement, constant-RAM Commit or release admission exists.

## Production LOC and next authorized work

Each implementation/proof commit records exact first-parent production LOC
and exclusions. The [comparison record](evidence/phase4.5/hot-publication-20260928/phase45-commits-loc.json)
and phase log give every commit: Core 65,136 → 67,158 (+2,022), reference
65,417 unchanged, combined 130,553 → 132,575 (+2,022) across this continuation.
The evidence/status commit is docs only, with the same 132,575 production LOC
and delta 0. No relocation, scope change or reference retirement is counted
as simplification.

After owner review, use the [checkpoint-5 prompt](HANDOFF-CHECKPOINT5.md).
Its control `48b51e874`, registered matrix/extra cases, cold contract,
one-sample rule and limits are unchanged. Do not rerun successful functional
checks on an unchanged product; if source/harness changes, identify and run
only affected proofs and rebuild matched identities where required. Fully
complete #273 before the main lane merges. #264/PR #269 owns mounted namespace
and charged ancestry; do not reimplement it or add a 256-node/128-dirty cap,
and do not block this lane on its integration.
