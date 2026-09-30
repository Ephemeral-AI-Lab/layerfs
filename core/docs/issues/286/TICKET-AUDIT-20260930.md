# Ticket audit before Phase 5 — 2026-09-30

> **Status: Dated planning checkpoint; not release evidence or a product contract.**

The owner requests an additional closure audit after Phase B main integration.
The audit reads the 38 open issues and 10 open PRs at the start of this request,
their completion requirements and relevant comments, current source, recorded
proofs and exact PR ancestry. Audited main is
`7b158d729f82c2c2197942298810d0f2f5916561`, tree
`03087983c1bd0224882fc94cb4628632423f6d87`.
No product source, benchmark profile, receipt or historical verdict is changed.

## Four additional completed issue scopes

| Ticket | Closure basis | Qualification preserved |
| --- | --- | --- |
| [#243](https://github.com/Ephemeral-AI-Lab/layerfs/issues/243) | The documentation-only package-refresh plan was delivered at `6ebdf0e6e90a92ab5b45ad0503e333ceeed3e42e`, an ancestor of main. [Fixture/commands](../243/TEST_WORKSPACE_AND_COMMANDS.md), [rollout](../243/IMPLEMENTATION_PLAN.md) and [index](../243/README.md) are present; the later Family 7 package selection provides separate execution evidence. | This planning issue does not require the full parent #232 performance campaign. #232 remains open; the old failed mixed baseline and numeric INELIGIBLE rows remain historical evidence. |
| [#228](https://github.com/Ephemeral-AI-Lab/layerfs/issues/228) | [C2 record-width accounting](../../../crates/layerfs-storage/src/encoding/delta/read.rs) charges a Raw group's selected framed record, with no WholeFile group-width shortcut. The [r046](experiments/20260930-history-regression-r046.md)/[r047](experiments/20260930-history-stride1-regression-r047.md) proofs complete all 17/53/157 state chains, independent expected roots and declared content/tree checks. | The defect is closed; no chain bound is raised. Old abort/timing evidence remains unchanged. The historical report path in the issue body is absent from current main, so this closure cites the committed Phase B proofs and current source rather than claiming that old local report was retrieved. General history/read qualification stays in #190. |
| [#229](https://github.com/Ephemeral-AI-Lab/layerfs/issues/229) | The universal full-capacity reservation is gone. [Placement](../../../crates/layerfs-storage/src/pack/placement.rs) sets a newly closed pack's capacity to its used length; only pooled metadata retains an open tail. [C2 finish](../../../crates/layerfs-storage/src/cas/lifecycle.rs) closes the unused tail through [the checked writer](../../../crates/layerfs-storage/src/sqlite/write.rs) before publication. The closed-pack rule was introduced at `92492722aa9e47b7ee19a4ee5ea8cd20e4235fcb`, an ancestor of main. | Current stride1 C2 allocated bytes are 85,983,232; genuine C5 adds 196,608 for 86,179,840 total, with correctness/storage/cleanup PASS under the approved up-to-10% profile. This resolves the reported full-pack allocation mechanism; it is not a matched speed comparison against the older 212,779,008-byte Store or a claim that unused space never exists. |
| [#180](https://github.com/Ephemeral-AI-Lab/layerfs/issues/180) | The co-design deliverable is complete in the [history overview](../../architecture/proposal/03-history.md), [implementation specification](../../architecture/proposal/commit-history/implementation.md), [repaired boundary](../../architecture/proposal/commit-history/remediation-contract-20260921.md) and [C5 description](../../architecture/16-history.md). The specification supersedes the earlier initialized questions and operational sketch. | Close the design scope. #210's H04/H06/H08/H14 integration/substitution proofs remain open. This does not certify every descriptive architecture paper, implement GC/conflict resolution/durability, or supply a dirty-Workspace discard API. |

For #180, the acceptance maps to specific delivered material:

- Entity immutability, parent ordering and Commit-versus-Layer/stage identity:
  implementation specification sections 3–4 and the repaired boundary.
- Separate C2 finish, stage, Branch Commit and Layer publication acknowledgements,
  MEMORY/OFF/no-sync behavior and unknown outcome: section 6. No crash-atomic or
  power-loss guarantee is asserted.
- Exact stage discard deletes metadata only; orphan content remains without GC,
  and reserved serials are not refunded: sections 4, 6–7.
- C5 owns checked scope-wide allocation; forks share its authority, exposed
  numbers are consumed despite failure/discard, and supported continuity has an
  explicit envelope: section 7 and the current C5 description.
- Separate catalog schema 1, seven tables, constraints, open validation and no
  automatic repair/migration: section 5 and the repaired boundary. The proposal's
  historical C2 schema pin is not a current C2 version claim.
- Stale expected heads return HeadMoved with retained custody; no automatic
  merge/rebase or semantic conflict resolution: section 6; #164 remains deferred.
- This audit changes no worker, deadline, quota, cache or durability policy.

## Three already-integrated stacked reviews

These exact heads are ancestors of audited main. Close the old reviews as
integrated through [PR #285](https://github.com/Ephemeral-AI-Lab/layerfs/pull/285),
preserving their branches and historical results. GitHub marks these reviews
CLOSED; no separate merge into their old stacked bases is performed.

| PR | Exact included head | Remaining parent scope |
| --- | --- | --- |
| [#242](https://github.com/Ephemeral-AI-Lab/layerfs/pull/242) | `87b10ab1b2144f1d7fb54f1be9419095f891fea0` | #241's complete ordinary-shell insert-size gate remains open. |
| [#244](https://github.com/Ephemeral-AI-Lab/layerfs/pull/244) | `7f07b5dadd9bee33f431a243345ce40ed214bb1a` | #243's planning scope closes; #232's full eligible campaign remains open. |
| [#246](https://github.com/Ephemeral-AI-Lab/layerfs/pull/246) | `b2e9d728d8744695b106a9432ca8a30f7f0c4dc2` | The delivered range-COW plan is included; #245's broader implementation/qualification remains open. |

## The 34 remaining issues

One unresolved required scope suffices to retain a ticket. Finite family success
does not substitute for a different registered workload or proof. This table
records the remaining obligation rather than renewing obsolete run instructions.

| Issue | Reason to remain open |
| --- | --- |
| #245 | Broad range-COW/Commit rollout and remaining scale/concurrency/qualification scope. |
| #248 | Arbitrary final-delta streaming, >65535 runs, bounded resource proof and real full-Commit write-progress gate. |
| #249 | Overlapping independent Exec, multiple mounted Workspaces, command leases and per-Workspace serialized Commit. |
| #219 | Configured 1/2/3 simultaneous Workspaces per sandbox, exact admission/refund and isolation proof. |
| #256 | General namespace/handle/encoding/large-listing scale and bounded streaming beyond the finite 1025-file case. |
| #259 | FUSE callback interruption and crash-safe unmount; statfs change remains in unmerged PR #275. |
| #261 | Cache-eligible 4097-write performance gate and the separate diagnostic/optimization lane. |
| #241 | All four middle-insert sizes, including the capped 500 MiB case, through the declared ordinary SDK/FUSE route. |
| #232 | Complete 56-case ordinary-shell registry with eligible performance, full independent proof and cleanup. |
| #233 | Complete 21 tiny-file/snapshot cases, including 25000 one-byte files and three ordered Commits. |
| #230 | Full benchmark migration depends on its incomplete substrate and pilot qualification scopes. |
| #235 | Exact-identity real canary/telemetry acceptance; the recorded final canary dropped a Service record. Later family proofs do not establish that distinct acceptance. |
| #237 | Matched cold native Init research with full 100000-file oracle and bounded resources/compactness; current large Init diagnostics are narrower. |
| #218 | Scoped save amplification instrument, authorized treatment and matched mechanism evidence. |
| #208 | Three-row chunk-predecessor on/off time-versus-space attribution; codec experiments do not supply it. |
| #207 | Matched retained-history FUSE mount/first-read comparison with declared read fraction and v0.1.6 counterpart. |
| #190 | Remaining historical read-path attribution and eligible matched qualification; current compound Family 2 success has its own profile. |
| #276 | Owner-deferred 10240 frontier, deep270 C1/ReserveInodes cost, dirty-discard/close and other handed-off limitations. |
| #283 | Verified phase-local memory capability on a suitable host; successful ineffective lifetime-peak reset remains a recorded failure. |
| #210 | H04/H06/H08/H14 real overlap/fault/substitution obligations explicitly remain unverified. |
| #192 | Full transport/resource/large-load implementation acceptance exceeds the selected Workspace family proofs. |
| #193 | Complete direct/forward campaign with its own oracle/topology/resource/telemetry identities and gates. |
| #181 | Pair-3 parent retains #192/#193 implementation and qualification dependencies. |
| #191 | Independent review of every load-bearing claim in the complete architecture paper set is not supplied by this closure audit. |
| #174 | Stage-1/2 attribution/simplification and remaining provider/coverage obligations. |
| #173 | Radish study is initialized; current main has the study prompt, not the full requested study deliverable. |
| #160 | Whole-product decoupling and independent-boundary qualification extends beyond Phase B scope. |
| #155 | v0.1.7 migration/release tracking remains active. |
| #164 | Explicitly deferred v0.2 logical diff/conflict-resolution semantics. |
| #82 | Future SQLite/S3 placement, locator/export/remote-read and publication design. |
| #69 | Future durable Commit, sync and crash/recovery guarantees. |
| #52 | Future multi-machine cloud persistence/publication/recovery guarantees. |
| #70 | Separate legacy Git100/Git500 callback-cost campaign and its declared milestones. |
| #51 | Separate legacy fuser dependency/behavior restoration; root reference remains in the migration scope. |

The other seven PRs (#275/#274/#272/#262/#257/#37/#36) are not proven to be
included by exact ancestry. PR #275 is an unmerged external statfs change;
#274/#272 are donor branches with selective porting; #262 is the separate #261
lane; #257 is an uncontained research branch; #37/#36 are separate prototypes.
No blanket merge or completion claim is made for them.

## Evidence and verification

The [seven-family checkpoint](SEVEN-FAMILY-CHECKPOINT-20260930.md),
[explicit proof reuse](experiments/20260930-seven-family-reuse.json),
[owning checks](experiments/20260930-family7-checks.json) and
[main acceptance](MERGE-ACCEPTANCE-20260930.md) remain the implemented Phase B
evidence. This audit runs no benchmark, canary, product build or repeated unit
suite. It verifies source/PR ancestry, document presence, the new local links and
staged whitespace. Historical failures and numeric cache-INELIGIBLE verdicts
stay unchanged. Actual closure state and source-pinned evidence links are
published in each closing GitHub comment.

Production LOC is unchanged: reference 65417, Core 70279, combined 135696.
This documentation-only commit compares the exact first parent and final staged
tree with `tools/production_loc.py --json --root <snapshot>` using identical
production-only archives, inline-test handling and exclusions. Its signed delta
is +0; there is no product migration or scope change.
