## Historical issue-body draft: superseded by promoted implementation plan

The owner promoted #130 into the active migration. The current objective is
close performance on existing tiny-churn cases, with no quadratic scaling;
25k/two-second and million-file cases are deferred. Use the
[current implementation plan](../../overlay-minimal-overhead-implementation-plan.md)
and live issue body. The remaining draft is retained review history, including
its old prerequisite assumptions; it is not current scheduling authority.

## Objective

Reduce fresh Workspace-per-agent-tool-call latency, temporary storage, I/O and
actual resource demand. Retain a fresh identity, branch lease, root, replay and
FUSE/handle scope; reuse existing immutable data and physical services, never
reset/reuse a mutable Workspace to substitute for isolation.

The governing overlay rules and specification remain in force. The detailed
revision is `docs/roadmap/0.1/0.1.6/evidence/minimal-overhead-review/revision-draft.md`.
When applying this revision, publish/version the selected document and attach
its immutable link. The draft path alone is not evidence of published source.

## Selected first implementation

Recheck the delivered prerequisite source and skip work already completed.

1. **Narrow unchanged-sequence shortcut:** after valid capture, bypass construction
   only when captured logical sequence equals applied covered sequence for the
   same Workspace/canonical comparison context. Retain exact staging, expected
   head/base and authoritative V4 outcome resolution. Equal bytes/root alone
   are insufficient; dirty equal-output attempts still update correspondence.
2. **Defer genuinely unused backing and journals:** keep the root Index and
   existing ownership backend, ID/cookie/alias/change maps and repaired Payload.
   Reserve prospective work, reader/cache ownership and recovery/release headroom
   accurately. This does not promise zero-FD Begin or add a memory-only tier.
3. **Inline singleton canonical correspondence:** use the bounded 106-byte
   nonzero-singleton form in the existing parent row, with one Description API
   and the indexed fragmented form. Preserve C2/zero-anchor semantics and avoid
   raw-payload retention.
4. **Prepare a small range leaf once:** reuse bounded encoding/ownership
   transactions instead of three-to-five successive Index updates. This removes
   intermediate work, not the final dedicated range page.

Measure residual cost before selecting more mechanisms. A full pager, cross-file
range packing, lower-ID virtualization, sequence-summary replacement, alias
promotion and tiny/shared-source storage are conditional alternatives, not a
mandatory bundle. Denser pages/compact ranges are direct next candidates only
where actual retained page or I/O cost warrants them.

## Guards required before selecting optional mechanisms

- **Packed reader ownership:** a file-only reader of A must not pin unrelated B
  payload through their shared metadata page. Acquire target token/base ownership
  under a temporary page lease, then release the parent. Fragmented cases require
  bounded file-scoped cursors, not copying all pieces into RAM.
- **Pager bootstrap:** select ownership/location metadata before backing exists,
  admitted incremental eviction, error/rollback and charge transfer. No graph
  pin, unbounded location map, capture-time conversion or first-write full copy.
- **Resident fsync:** define resident-only flush/error handling, including a small
  acknowledged write followed by fsync before any eviction. Preserve the existing
  contract without inventing restart durability or per-write durable flushes.
- **Tiny/whole sources:** preserve ordinary versus SDK charging, Origin and append
  coalescing. Promote before a second independent token interval, including
  append/join or full-range duplication unless the same token is correctly reused.
- **Scan boundary:** O(L+K) sorted lower/upper merge describes a fixed captured
  view. Live FUSE keeps numeric-cookie/after-EOF semantics through an explicit
  adapter; count its revalidation, attribute and additional seek costs.
- **Representation revisions:** document/version selected internal-spec changes,
  including dual change/cookie/alias alternatives. Published-page in-place
  mutation is not selected; no semantic waiver is implicit.

Commit must continue to read its owned snapshot while ordinary operations use
live inodes. Publication advances exact coverage without clearing newer changes,
remounting, checkpoint reset or disabling operations for a retained stage. Retry
uses the same candidate/context and authoritative Created/UpToDate outcome.

## Verification and measurement

Compare against the delivered #124/#125 source. The [v0.1.5 benchmark closeout](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.5/release-notes/0.1.5/benchmark-closeout.md)
is additional historical context, not a new target or a matched current result.
For example, its clean 50-file/1-MiB public-call sum is 10.495 ms including Begin,
Commit, visibility and End; inner Commit alone is 1.501 ms. Preserve exact timer,
cache/source and sample identities, and original WARN/FAIL/waiver/reuse status.

- [ ] Verify prerequisite completion and recheck findings against delivered source.
- [ ] Freeze the selected minimal design and any private-format/spec changes.
- [ ] Implement selected first changes with exact ownership/publication guards.
- [ ] Check equal-byte/new-Origin -> UpToDate -> localized C2, conflicts/lost replies, fresh lifecycle, release/rollback and existing C1/C2/zero-anchor behavior.
- [ ] For selected optional mechanisms, complete their packed-neighbor, spill/fsync, append/promotion, identity/alias or live-cookie proofs before claiming compliance.
- [ ] Carry forward natural-overlap and million-changed-file qualification; rerun only evidence whose exercised source/fixture/environment/custody is invalidated. A 100k accounting example or paused-builder check cannot replace those proofs.
- [ ] Publish actual public-call time, foreground latency during active construction, immediate post-capture work, RSS/cache/reservations, physical metadata/payload/retained/scratch storage, I/O and cleanup separately.
- [ ] Complete applicable final-source regression/custody requirements with no required unresolved failure; retain failed attempts and show measured improvements rather than arithmetic-only completion.

Use existing benchmark entrypoints, repetitions, criteria and measurement lock.
Store/SDK/construction/spool remain on macOS; daemon/FUSE/workload remain in Linux
Docker. #122 scenarios remain excluded. No new numeric gate, replacement campaign,
unconditional full-suite rerun, release/tag or closure of #123 is authorized.

Preserve original analysis/audit/source receipts. The prior ~105–245-MiB layout
estimates are not accepted per-call allowances or final storage targets. Select
and evaluate additional complexity only for a demonstrated remaining cost.
