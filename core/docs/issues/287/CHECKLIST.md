# Issue287 current implementation checklist

> **Status: Current planning checklist; no release candidate exists.**
> Current published checkpoint: `06fe8363d5c317c49876d5189d374c1a34cc6010`
> (R1b-cache PARTIAL; R1 remains open).

Assignment: #287 implementation only. Benchmark qualification is delegated/unrun
under #288. [Frozen interfaces](R0-FROZEN-INTERFACES.md) supplement the research
packet; [IMPLEMENTATION-LOG](IMPLEMENTATION-LOG.md) is append-only.

Current milestone: **R1c checkpoint publication**; next is
**R1d-run-seek exact point/high-water lookup** ([freeze](R1D-RUN-SEEK-FREEZE.md)).
R0 is published and linked in #287 comment5911332695. Native/engine/physical
protection and concurrent enablement remain gated on their owning proofs.

- [x] Inspect actual primary/published/research source, ancestry, origin and artifacts.
- [x] Create clean owned managed worktree and `codex/issue287-implementation` branch.
- [x] Copy only owned research/scenario/report/AGENTS/handoff/captures; preserve output.
- [x] Capture current #287/#288 and #248/#256/#249/#219/#259/#276 requirements/comments.
- [x] Read packet in requested order and preserve historical verdicts.
- [x] Freeze shared private/stream/READY/resource/migration/wire ownership choices.
- [x] Review independent canonical v1/v2 vectors and reproducible method.
- [x] Review resource/runtime audits and simultaneous window arithmetic.
- [x] Check owned links/whitespace and exact staged/parent production LOC.
- [x] Commit, confirm counted committed tree, push and confirm publication.
- [x] Link exact source packet/contracts/scenarios in #287; mark R0 on passing exits.

R1a current gates:

- [x] Typed per-Store C2Save and per-actual-catalog C5Catalog ownership.
- [ ] Release completed content admission before short composite publication (requires versioned result custody; legacy v1 remains unchanged until R3).
- [x] Real two-active-Save StageChanges barrier and ordinary authorized range refill (direct Service; Workspace lease exhaustion remains later).
- [x] Third C2/no-effect refusal, exact catalog conflict/grant/identity/custody checks (real-provider Unknown induction remains unrun).
- [x] Freeze/review source; run covering locked tests/examples/fmt/Clippy and boundary checks.
- [x] Exact LOC comparison, checkpoint commit/push/#287 update (comment5912196829; partial disposition preserved).

R1b-cache current gates:

- [x] Current batches own every hit/fetched page before retained-cache eviction.
- [x] Insertion enforces requested count, mapping validity and actual Vec capacity.
- [x] Default64/limits1,31,32,64; grouped32 demands and mixed-hit eviction preserve independent bytes.
- [x] Local edit route uses the same checked retention; independent wide old/new bytes and separate sealed v1 roots/partitions pass.
- [x] Real StoreProvider and requested-allocation/table/release evidence; global/native admission gaps explicit.
- [x] Freeze/review and run scoped owning locked tests/examples/fmt/Clippy/boundary (exact failure/correction/reuse retained).
- [x] Exact LOC comparison, committed recount, push/confirmation and #287 checkpoint (comment5913195371).

R1c current gates ([concrete freeze](R1C-FROZEN-STATE.md)):

- [x] Issued compact selection/full owner association; actual typed63-byte records (C1 exits pass).
- [x] Genuine common C1 DirectoryRoots indexed integration; independent v1 roots unchanged (Server composition check pending).
- [x] C2 metadata-only indexed scratch, dual128/64KiB bounds/seals/EOF/refusal.
- [x] Actual native allocation/identity, single cleanup and bounded retained failures/Unknown (Darwin scope; stronger/global gaps explicit).
- [x] Server pre-Save admission, exact typed failure and cleanup-before-publication composition (actual successful/failed known-Save cleanup under scratch Unknown).
- [x] SQL LOC classification repair/tests and unchanged parent classification.
- [x] Same revised counter on exact first-parent/final staged snapshots; committed recount follows publication work.
- [x] Independent roots/bytes/provider/resource checks and scoped owning locked checks; platform/global gaps recorded.
- [ ] Checkpoint exact LOC, commit/recount/push/publication and #287 update.

Named submilestones and dependencies:

| Milestone | Smallest complete responsibility | Exit proof |
| --- | --- | --- |
| R1a catalog admission | Typed C2Save/C5Catalog/read Service domains; phase-scoped publication | Real C2 active_slot rows=2, C5 refill before held bodies release, exact refusal/authorization/stage roots; native saturation gap explicit |
| R1b admitted working owners | Cache insertion and canonical/read/encoded/decoded/clone/wave admission | Actual overlap arithmetic, pre-effect refusal, exact transfer/release and cache crossing |
| R1c paged construction state | Typed C1 cursor/index/run ports and C2 metadata-only scratch | Monotone indexed progress, count+byte pre-effect refusal, real SQL plans/schema/cleanup |
| R1d C1 state integration | Complete paged draft/frontier/reference construction paths using R1c ports | Independent v1 partitions/roots, exact graph/effects and bounded actual allocations/work |
| R1e engine/control envelope | SQLite guard/readback, phase engine shapes and protected native dispatch | Two-Save/read/scratch occupancy with catalog progress; physical gaps explicit |
| R2a interval authority | V3 pages/cursor/split/join and paged candidate custody | Exact interval oracle, removed-subtree carry, bounded paths and invisible refused candidate |
| R2b live catalog | Current publisher/conflict leases/sources/locations/owners/retirement/handles/cookies | Ordinary syscalls, unrelated inode progress, capture crossing, no resident population/WRITE chain |
| R3a streamed construction | FileSet/binding/results, fresh direct/inherited replay, paged C1 state | Independent roots, exact EOF/seals and Unknown custody |
| R3b canonical transition | Immediate-parent policy/certified parents/schema11/import | Separate v1/v2 roots, correspondence/cycles, quiescence and unchanged packs/locators |
| R4a complete mutation | Mutation→capture→FileSet→READY→Branch→current-G2 install→cleanup | Old/new/pinned roots/bytes, known/Unknown and no proportional post-publication admission |
| R4b supported profile | Successive heads, all-phase writes/pins, #248/#256 boundaries | Whole-operation resources/work; platform/benchmark gaps explicit |
| R5 native runtime | Owned I/O/commands/domains, actor/FUSE interrupt/quota/death/discard | UntilOwnedExit>30s, cancel/disconnect/descendant/reap/pipe and external mounted mutations |
| R6 concurrency | Registry/incarnations/pools/count1/2/3/independent ownership | After R4/R5: same/cross-W progress, one submission/W, Branch conflict/capacity/refund |
| R7 cutover | Imports, old-owner drain, retirement, final docs/checks | Independent final proofs, exact profile, full Core checks and #288 handoff |

Each delivery gets exact LOC commit and #287 checkpoint. Incomplete gates stay
unchecked; dependent enablement stays disabled. No #288 campaign/issue edit.
