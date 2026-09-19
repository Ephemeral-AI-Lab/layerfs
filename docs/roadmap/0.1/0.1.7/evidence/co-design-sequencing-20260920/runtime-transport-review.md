# Read-only sequencing review: projection/runtime and boundary/trust

> **Status:** Research; informative and not a product contract.

Scope: frozen issue bodies #179/#180/#181/#172/#165 from `/tmp/layerfs-design-order.l4IJqs`, local proposal documents, the Stage 7 API audit and source checks. No repo/GitHub edits, builds, tests or benchmarks. Recommendations below are not owner decisions.

## Verdict

Adjust the delivery sequence to dependency gates rather than pair numbers. Do not implement all of #179, then #180, then #181; do not implement an entire daemon first either. #179 defines the logical operations that #181 transports, while #181's trust and deployment decisions constrain #179's state/lifecycle. #180 must supply allocation and publication semantics early enough for create/rename/commit rather than arrive only after FUSE.

The existing issues already support this: #181 Acceptance requires tenant direction **before #180 freezes a schema**; `core/docs/architecture/proposal/01-projection-and-runtime.md:39–43` explicitly feeds #179's operations, bounds and acknowledgement decisions to the other pairs. Pair numbers identify responsibilities, not execution order. The current owner direction puts design after Stage 7; this review can identify dependencies now without treating implementation as authorized or Stage 7 as accepted.

## Minimum decisions, in order

1. Finish/reconcile the reviewed C1/C2 contract baseline with final Stage 6 source. Stage 7 identifies which facade, policies, formats and resource bounds integration may rely on and dispositions relevant findings. Avoid depending on low-level SQL/pack helpers.
2. Joint #179/#181 deployment and boundary decision: first supported topology (all in Linux vs Linux consumer/macOS owner), actual reachable stream endpoint if separated, single trust domain vs multiple, authorized roots/stores, client/owner lifecycle. Keep C1+C2 together by storage. The macOS-owner option is a scenario to choose, not a frozen requirement. Merely using a Linux VM does not demand a daemon.
3. Joint operation contract: resolve/stat/list/range read; complete final filesystem mutations and stable byte input; batch/stream maxima; pending-overlay limits and pressure policy; result/error taxonomy; cancellation/abort and disconnect semantics. #179 determines operation shape; #181 determines framing, flow control and error representation. Logical operation bodies call local C1/C2; never RPC each AuthenticatedObjects/FinalizedConsumer callback.
4. #180 early semantic subset: owner/scope of inode allocation; distinction between saving objects/root, staging, logical Commit and branch publication; visibility and acknowledgement points. Tenant/policy direction from step 2 precedes frozen history schema. Full history implementation may follow the first root-level save/read slice, but creation and Commit semantics cannot be invented by transport.
5. Freeze only the initial supported operation contract/topology and needed semantics; then split implementation into runtime/accumulator, operation owner + one transport, and history. Avoid implementing every deployment cell or plugin registry up front.

## Concrete parallel work and blockers

| Track | Can proceed independently after shared decision | Blocked by |
|---|---|---|
| #179 accumulator | Bounded pending edits, complete sorted directory inputs, overlay/base read composition and deterministic public-API tests | Operation shape, pressure/flush policy, allocation contract for fresh inodes; published vs same-save roots decision |
| #181 owner/transport | One real stream endpoint, bounded framed logical operations, original error classes, cleanup and disconnect handling; use the real owner handler directly for local tests | #179 logical request/response shapes; tenancy/auth direction; #180 acknowledgement/publication meaning for Commit |
| #180 history | Identity/parent/stage/discard design and implementation once placement is agreed | Tenant/policy direction before schema freeze; runtime flush/visibility requirements |
| Linux FUSE adapter | Kernel callbacks mapped to frozen runtime contract, mount lifecycle and real supported operation subset | Access to a mount-capable Linux environment; runtime API; actual link for selected remote-owner deployment |
| Root save/read smoke slice | Existing published base -> accumulated edit -> C1 update -> C2 finish -> explicit root readback | No complete history engine needed, but label it a root save/read result, not logical Commit |
| User-visible Commit | Full stage/head publication, caller success and connection-loss behavior | #180 agreed/implemented publication semantics plus #181 outcome contract |

Published-base update fits current C1/C2; the Stage 7 audit states that the missing same-save reader/sink bridge is conditional. #179 should first decide whether it really needs a second C1 operation against an unpublished earlier root. Do not modify C2 or introduce intermediate save publication just to bypass that design choice.

## First actual vertical slice

For the **candidate** Linux-consumer/macOS-owner topology:

Linux application -> real FUSE mount -> bounded Workspace accumulator -> actual cross-boundary byte stream -> macOS logical-operation handler -> C1 -> C2 -> SQLite.

Start with an existing published filesystem root containing a regular file, so fresh inode allocation and full history schema need not be invented merely for the first slice. Resolve/stat/read it; overwrite bytes; read your own pending write through the overlay; submit a stable accumulated update; finish C2; obtain the explicit new filesystem root; read that new root from a separate consumer operation. Exercise a bound breach and connection drop during request/result separately, verify unknown outcome is failed with no automatic resend and no guessed rollback. Clarify cleanup on definite failure vs unknown success before implementing this.

After #180's publication semantics are available, extend to create/rename using allocated inode identities and to the real stage/Commit/head operation. Do not describe the first saved-root slice as full Commit acceptance.

If the selected first topology instead co-locates all components in Linux, the first production path can call the same logical operation body directly. A later host/Linux split requires real stream qualification; embedded passing tests cannot stand in for it.

Embedded/direct checks prove request semantics and core composition only. They do not prove Linux kernel FUSE callbacks/cache/visibility, VM-to-host routing, stream partial I/O, disconnect/unknown outcome, transport backpressure, or cross-process authorization. Conversely, merely constructing C1 output over a socket without real FUSE is not projection acceptance. No performance claim follows from these correctness checks.

## Source-backed wording problems affecting order

1. **Stale local implementation ownership.** `core/docs/architecture/proposal/01-projection-and-runtime.md:78–81,92` still assigns runtime implementation to #172 and measurement to #171. Frozen #179/#172/#165 explicitly supersede that: Stage 7 reviews architecture; integration design/implementation/qualification is subsequent. `04-boundary-and-trust.md:107` also points measured acceptance to #171. Update the active local proposal routing when the sequence is adopted; historical receipts remain unchanged.
2. **Database traffic is overstated.** #179's settled table and proposal 01:53 say DB traffic occurs at only create/commit/merge/end and never on FUSE write path. Yet `FilesystemRead::new` reads a canonical root (source `filesystem/read.rs:82–86`), resolve reads directory/inode objects (105–133), and `StoreProvider::read_wave` opens a DB read session and reads it (`cas/provider.rs:109–126`). #179 also leaves callback-triggered flush open (proposal 01:58–61). The assertion should distinguish history-metadata writes, base/object reads needed by operations, and the chosen pressure-triggered flush. Current source does not establish whether a future write callback specifically needs such base reads; this is a false blanket guarantee/underspecification, not an observed runtime bug.
3. **Acknowledgement wording conflicts.** #179 says acknowledgement must survive connection loss; #181 says unknown acknowledgement is failure, no resend, and leaves fresh-overlay reflush unresolved. Require the same explicit known-success/definite-failure/unknown-outcome table across #179/#180/#181. Do not implement reliable delivery/retry/recovery as an interpretation of “survive.” Repository rules forbid automatically retrying an unknown operation.
4. **Boundary narrative ambiguity.** #179/#181 describe the owner as serving “store these objects,” while #181's settled transport table requires logical operations rather than objects. Rewrite endpoint examples as logical construct/apply/update/save operations with bounded byte input, keeping object handoff local between C1 and C2.

## Suggested precise issue amendments (proposal, not applied)

Shared #179/#180/#181 sequencing paragraph:

> Pair numbers name design responsibilities, not an implementation sequence. Following the Stage 7 contract review, #179 and #181 jointly define the first supported deployment and bounded logical-operation contract. #180 supplies allocation, save/stage/publication and acknowledgement semantics before operations depending on them are frozen; #181's tenancy direction precedes #180 schema freeze. Implementation may then proceed in parallel against that shared contract. The first integrated slice exercises one actual deployment end to end; broader operation coverage and history publication follow their agreed designs.

Add #179 readiness checkbox:

> [ ] With #181, enumerate the initial logical requests/responses and define the pending-overlay budget, pressure/flush policy, published-base visibility, same-save read requirement and callback completion/error behavior. Name base reads separately from metadata writes and avoid asserting zero database traffic without evidence.

Add #181 readiness checkboxes:

> [ ] Select and record the first supported deployment, reachable endpoint, trust/authorization boundary and one bounded transport implementation. Linux FUSE plus macOS storage ownership remains an option until selected.
> [ ] Implement one real logical-operation path with #179; define definite failure, known completion and unknown outcome with #180 before a Commit acknowledgement exists. Keep C1/C2 together and object-provider/sink callbacks local.

Add shared integrated acceptance checkbox:

> [ ] Demonstrate real FUSE read -> pending edit/read-your-write -> bounded submit -> completed save -> new-root readback on the selected topology, then stage/publication under #180. Embedded tests and core Stage 6 results do not substitute for mount/transport acceptance; performance qualification is separate.

Update #165/plan routing to display this dependency sequence without renumbering pairs. Keep #172 as review only; create implementation assignments only when contracts and scope are concrete.
