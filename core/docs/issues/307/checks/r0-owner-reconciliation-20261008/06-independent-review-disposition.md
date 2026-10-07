# R0 independent review disposition

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Three read-only subagents reviewed input HEAD `1a6bb53ef14e1860d8f222df11394e5a654bb34d`.
> No subagent edited files, built, tested, measured or cleaned resources.

| Reviewer | Concrete finding | Disposition |
| --- | --- | --- |
| native_review | Daemon Exec wire/supervisor/launcher/cgroups and admission/drain contradiction | Withdrawn in current spec/matrix/dispatch/primary docs; runtime-owned successors |
| native_review | fuser public replies return (); send failure only logged internally | Exact one attempt/disposal counted, exact send/delivery unavailable; no new patch |
| native_review | Partial-spawn/panic run() error not all-loop join evidence | Retained custody required; complete daemon-work drain and explicit readiness observation retained |
| native_review | Child shared mount alone does not establish copied namespace Busy/detach | Actual runtime/executor parent topology and FP-22-FS; no assumed propagation recipe |
| native_review | Caller-held cwd/descriptor can keep aborted mount busy while process survives | One plain detach attempted; post-abort EBUSY Retained, no process kill or second attempt |
| namespace_review | CapturedFileEdits/Content streamed backed route and existing Store Commit reusable; complete namespace/custody/topology absent | R4/R5 requirements recorded without claiming closure or new driver |
| namespace_review | Missing neutral captured-namespace port adapter home | Separate +1 P destination amendment; original inventory unchanged |
| rollout_review | Old C1-only and S0–S13 dispatch/remote tracker assignments contradicted latest scope | Both dispatches route to R0–R9 local ledger; old prompts historical source only |
| rollout_review | Withdrawn IDs still in live references; runtime rows incorrectly all called native | Live references corrected to distinct successors; proof scopes split explicitly |
| rollout_review | SDK organization ledger implied implemented facades | Required contract wording; R1 remains NOT_STARTED |
| rollout_review | Old excluded Sandbox depends on retired API-core/host wiring | Preserve/relocate before actual replacement, never activate as-is; locked std/process capabilities reviewed at R1 |
| rollout_review | Overlay/source topology still candidate/upstream/capability wording | Selected one-owner Overlay profile, separate direct Store, local borrowed Save |
| rollout_review | LOC exact snapshot extraction and nested SDK classification | Pinned root counter, exact full product src/SQL snapshots and per-path member classification; no counter change |

Current original owner ruling rows P-1–P-7 are byte-identical to input. Original
correction/manifest/historical receipts and product/config/lock trees are preserved.

## Source and primary-document evidence

- fuser `core/vendor/fuser-0.18.0/src/session.rs`: from_fd line196, run line291,
  receive/dispatch line523; `reply.rs` send_ll_mut line129, automatic drop EIO
  line150, public void-return ReplyEmpty line178; `read_buf.rs` line6 and
  session.rs MAX_WRITE_SIZE line52. These are input-source observations, not
  native measurements. All first-party reply paths must consume explicitly.
- S6 `core/crates/layerfs-overlay/src/lifetime/lookup.rs` line13 supplies
  independent retained lookup/processing semantics. Native aggregate/group
  custody and callback mapping are new scope, never counter-only FORGET.
- Linux v6.12 [propagated mount busy](https://github.com/torvalds/linux/blob/v6.12/fs/pnode.c#L386-L419)
  and [propagated unmount](https://github.com/torvalds/linux/blob/v6.12/fs/pnode.c#L541-L604)
  traverse parent propagation; [shared-subtree semantics](https://docs.kernel.org/filesystems/sharedsubtree.html#detailed-semantics)
  explains child unmount under a shared parent. Actual supported topology needs
  its oracle. [fusectl abort](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/control.c#L31-L42)
  remains independent of detach.
- Existing `daemon/store/commit.rs` is the R5 owner; captured regular-file
  `workspace/construction/captured/owner.rs`, neutral operation records and
  Content `update_filesystem_streamed_backed`/`FilesystemObjects::new_with_accepted`
  are reused. Source review found growing validation vectors/maps and whole-base
  alias/cycle walks, native directory-rename ancestry input and fresh-child release
  refusal; all remain implementation/correctness work, not approved shortcuts.

Last subagent messages used snapshots while root edits continued. Their remaining
reference/dispatch/wording findings are addressed in the final scoped documents;
no stale review assertion is promoted into a product verification claim.
