# S9 runtime and complete-root audit

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Milestone state: CHECKPOINT, incomplete. S9 remains unchecked.

The new [bound history handler](../../architecture/37-runtime-history-receipts.md)
is actual product code under SDK runtime/handlers, over the initialized real
macOS Store/catalog. It adds checked root serial binding, stage-after-SaveFinish,
exact-token conditional transition/discard and bounded retained receipts. It
preserves authenticated peer/Workspace/Branch/catalog/runtime authority and the
captured expectations. Unknowns retain custody; no Branch refresh, resend,
automatic conflict discard or guessed install is added.

| S9 requirement | Current implementation/evidence | Remaining |
| --- | --- | --- |
| Authenticated policy/object/serial/Save boundary | Existing native KK VerifiedPeer binding, owning semantic object admission, local policy/length/serial windows and interleaved Saves | Logical framing/codecs and actual authenticated runtime service delivery |
| History adapters | stage_saved/commit_saved/discard_saved plus history_receipts; exact outcome/token/error retention | Logical transport, bounded fair queued service and integrated history reads |
| Save/topology lifetime | Stack-borrowed initialized Storage owners, same-Save reads, no whole-Save provider checkout; finished Save required for stage | Cross-connection/peer disconnect/restart fences and aggregate flow/capacity proofs |
| Semantic/context authority | Root grammar/profile/scope and supplied authority checked; Store reference closure retained | Full contextual topology/provenance qualification and untrusted transport admission |
| Unknown history | Typed unknown retains slot, commit/discard cannot replay or release it | P10 completion-fenced resolver protocol remains explicit; no unknown fault proof added |
| Complete initial roots | Existing Content/base proofs include aliases/symlinks/full-state shape; this checkpoint adds no importer | P12 native Init still refuses symlinks, holds scan/job collections and retains4GiB file cap; faithful full ignored/dependency/cache/output/.git import incomplete |
| Sandbox/API-core/client assembly | Current package boundaries preserved; no excluded package activated from a scaffold | Actual owning Sandbox/API-core and SDK client integration still required |

Nine real-host runtime tests pass, including three new history bodies: UpToDate,
wrong-role/authority/cross-capability admission and exact explicit discard, and
same-Branch Committed winner versus deciding-transaction HeadMoved with retained
original stage/token. Both candidates are saved through real initialized owners;
loser release refuses until its exact known discard. There is no automatic
re-stage, operation retry or Branch refresh. Binding's root serial is authenticated
at bind. Test fixture native accept/read/write waits are bounded and panic-safe.
These small fixtures qualify handler behavior, not full-root readiness or a
load-bearing runtime.

[Covering host output](checks/s9-history/layerfs-sdk-covering.log), [no-run builds](checks/s9-history/)
and [failures](checks/s9-history/FAILURES.md) remain append-only. Rust1.85.1 locked,
repository ARM64 profile, explicit120-second test ceiling and one construction
worker apply. Linux compile succeeds but the owning provider tests are macOS cfg;
no Linux global provider/history execution is claimed. Four-package Clippy/fmt,
boundary568 and26 tool tests pass at the relevant product source. [Identity](checks/s9-history/identity.json)
records exact source and binary inputs; uncontrolled caches have no speed/RSS eligibility.

Production LOC uses unchanged tools/production_loc.py and exact first-parent/staged
archives; the commit/handoff record core/reference/combined totals and signed delta.
Reference65417 and excluded predecessors remain counted and preserved.

Next independent work is authenticated logical demand/Save/history framing and
fair delivery with exact disconnect/restart fences, then faithful backed initial
acquisition in Project/import and owning Sandbox/API-core integration. S7 complete
resource/cost acceptance and S8 native integration also remain required. S10
incremental Commit and P3/P6/P7/P13/P14 are outside this batch and explicitly open.
