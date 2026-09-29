# Selective integration of active backing into Phase 4.5

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Prepared 2026-09-29 from documentation source
> `d1192b7363e11f19641b2e0dba0c5d64f2011826` and three read-only component audits.
> Proposed construction base: `05eb5c14849f1b874383fd1600ba9288f103ad93`.
> Side-lane product donor: `11a864fc133844cae7a4247b1f243d84d5763b10`.
> No implementation, branch integration, build or physical benchmark runs here.

**Build the integrated product on the corrected component-only Phase 4.5
source, porting selected side-lane mechanisms in coherent patches.** Do not
merge the whole side branch or replace Phase 4.5's namespace with its tree.
Keep ordinary public Mount/Exec/Commit, one live namespace authority, verified
private publication and exact G1/G2 ownership. Use selected benchmarks as
engineering evaluation points during construction, then verify the frozen
integration source separately.

The current side tree contains Phase 4.5 ancestry, but restores aggregate
resident paths, descendant checks and path-bearing selected views. An
ancestry check or conflict-free Git merge would preserve those regressions.
The integration work is a semantic adaptation across Node, View, mutation,
capture and lease owners.

This plan extends [the benchmark organization](BENCHMARKS.md), [the baseline
inventory](BASELINE-20260929.md), [architecture refinement](architecture_refinement.md),
[private backing/live Commit](private_backing_workflow.md), and [FUSE
workflow](fuse_workflow.md). Those records describe existing source and receipts;
the target architecture below is proposed work.

## 1. Freeze the right sources

| Role | Exact source | Meaning |
| --- | --- | --- |
| Published Phase 4.5 review head | `6eb7553671d3000160ad023a57b335e52dd81a26` | Draft [PR #269](https://github.com/Ephemeral-AI-Lab/layerfs/pull/269), branch `codex/issue264-phase45` |
| Proposed corrected construction/port-control base T0 | `05eb5c14849f1b874383fd1600ba9288f103ad93` | Component-only Phase 4.5 plus its one-seal repair; first parent is the published head |
| Side product donor P0 | `11a864fc133844cae7a4247b1f243d84d5763b10` | Current active backing, funded completion, generic Base reuse, internal v2 and scoped functional proofs |
| Frozen earlier algorithm control A0 | `48b51e874a41b3e1e6c6661e145316df8b408f07` | Pre-#273 control; its numeric campaign remains NOT_RUN |
| Documentation basis | `d1192b7363e11f19641b2e0dba0c5d64f2011826` | Workflow documents on the owned finalization branch |
| Remote main at this checkpoint | `f74dbe77da12fa533587be8a578375bce3f19373` | Does not contain the completed component-only Phase 4.5 implementation; not the construction base |

Remote heads and issue/PR state were read on 2026-09-29. T0 is an exact
existing corrected snapshot, not the remote PR #269 head. Draft [PR #274](https://github.com/Ephemeral-AI-Lab/layerfs/pull/274)
still points to older `29fc5747d70eadf01fd8794bf84a50cf57ef3660`; it is not P0.
PR #269 remains stacked on open [PR #263](https://github.com/Ephemeral-AI-Lab/layerfs/pull/263)
at `ef3a310480254774d6e6966004fb5e0a4fa3b94d`, itself on draft
[PR #260](https://github.com/Ephemeral-AI-Lab/layerfs/pull/260). Preserve those
owners' branches and review heads.

```text
common ancestor 6af2c5c59
        +--> Phase 4 ef3a31048 -> published Phase 4.5 6eb755367 -> T0 05eb5c148
        |
        +--> pre-#273 control A0 48b51e874 -> side evolution

existing donor P0 contains BOTH lines' ancestry; T0 and A0 are sibling histories

NEW construction:
  T0 -> separately owned integration branch
          selected compatibility patches from P0
          component-only active Workspace adaptation
          selected leases + live Commit/custody
          frozen benchmark/evidence checkpoint

No git merge of the donor branch.
No whole-file replacement of shared namespace/state owners.
```

T0 and A0 are not ancestors of each other. Their common ancestor is
`6af2c5c59a48d0b6c85d656e55aecc353e346728`; both occur in P0's history.
That ancestry establishes source availability, not preservation of T0's
component-only runtime in the donor.

Before construction, record an exact target/donor/path manifest and verify the
current remote review target. Work from an isolated owned checkout at T0; do
not reset the existing documentation or another owner's worktree. If the
review target advances, compare that new tree against the manifest before
continuing. A later PR targeting the published Phase 4.5 branch must explicitly
include the T0 repair in its diff or use a newly agreed corrected target.

The current working tree's two pending edits are excluded from the donor:
`readable.rs` has an unverified capability fixture using the wrong
`maximum_version` field, and the integration checkpoint has an uncommitted
follow-up. The actual response field is `version`. Adapt the external fixture
explicitly at the new test source; do not sweep those edits into a port or
declare their proof complete.

## 2. Target architecture and the integration seam

```text
ordinary WorkspaceApi.exec(command)
        |
Linux shell -> VFS -> real FUSE callbacks
        |
parent inode serial + ONE checked component / stable file handle
        |
+---------------- COMPONENT-ONLY WORKSPACE --------------------+
| NodeName + stable serial + parent + attached/retained ancestry |
| node_index; charged ancestor closure; main live_chain checks  |
|                                                            |
| selected View = immutable filesystem Base + ActiveSnapshot  |
| current mutation rechecks live ancestry                     |
| old selected reads use their OWN selected namespace         |
+---------------------------+--------------------------------+
                            |
              ONE private active authority
                 I/N/D/E/P/R/L index
                            |
              verified page/index publication
              shared tiny packs / owned payloads
              optional byte-proven Base origins
              selecting-pin retirement + precharged fund
                            |
           capture G1; live G2 publishes new versions
                            |
              authenticated internal SaveFile v2
                            |
          host Service -> C1 -> unchanged C2 + History
                            |
              checked known canonical outcome
                            |
          affected C5 patch, preserving G2/F0 coordinates
```

The resident namespace and private backing fit together naturally: active
`N|parent|name` and `I|serial` records already use components and serials.
The physical page, pack, extent and retirement formats do not need a new
namespace format. The work is adapting their callers and ownership into T0.

Three identity rules are essential:

1. `State.base` is a canonical filesystem root; `I.base` is the immutable
   file-content root used by that inode's Base extents. Advancing filesystem
   R0 to R1 does not authorize rebinding later G2 offsets from file F0 to F1.
2. The live directory graph authorizes new mutations. A frozen View resolves
   its old names against its selected active/canonical state, independently
   of a later live move, removal or parent edge.
3. Direct canonical LogicalPath/import and one Linux pathname syscall keep
   their own bounds. Removing an aggregate limit from component-relative
   mounted operations does not remove those separate interfaces' limits.

Keep the existing 128-handle, 32-public-lease, Budget, quota, physical-owner,
page/record and integer bounds. This plan removes the restored aggregate
namespace locator/refusal and associated path work; it does not declare
unlimited files, unlimited resident state or constant total memory.

### The directory move to preserve

```text
REJECTED WHOLE SIDE-TREE RESULT              PROPOSED TARGET
------------------------------              ---------------
derive old/new full paths                   resolve old/new parent + name
        |                                           |
depth grows? -> visit descendants           check replacement, walk d live
        |                                   resident ancestors for cycle
prepare resident replacement paths                  |
        |                                   N(old,name)=tombstone
update bindings and all reached paths       N(new,name)=same moved serial
        |                                   prepare affected I/D + moved edge
publish selected root                               |
                                            under final State gate:
                                            recheck stamp + attached ancestry
                                                    |
                                            publish one active candidate
                                                    |
                                            install prepared moved Node edge

Work depends on descendants/resident paths  no descendant path enumeration
                                            actual index/owner + ancestor work
```

No new canonical parent index belongs to this port. C1's full-base/effective
subtree alias/cycle work remains a separate Commit cost, deferred through
[#276](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276), historically #270.

## 3. Resolve semantic conflicts explicitly

| Shared owner | Target rule | Required port action |
| --- | --- | --- |
| `runtime/state.rs` | Keep NodeName, parent/attached/retained fields and indexed ancestry | Add active owner, selected context, memo, fund and lease fields individually; never copy P0's NodePath/extended-path state |
| `runtime/ancestry.rs` | Keep main live_chain and charged complete ancestor closure | Use population/cycle checks; do not restore a fixed 256-step namespace-depth refusal |
| `namespace.rs` / `namespace_view.rs` | One-component checks; View+parent+name identity resolution | Add ActiveSnapshot selection without PinnedDirectoryPath/full path construction |
| `active_view.rs` | Active binding/tombstone first, immutable same-Base fallback | Rewrite resolver to serial/component/cursor inputs; preserve selected revision rechecks |
| `active_create.rs` | Reserve one serial, atomically publish new identity/name/handle | Prepare a charged component owner, not a full child locator; retain consumed-serial custody |
| `active_remove.rs` | Selected effective emptiness and detached-parent refusal | Use serial-based listing; retain held/unbound identities and old views |
| `active_rename.rs` | One/two binding changes and one live moved-directory edge | Replace path vectors, descendant scans and prefix-based cycle tests with main live-chain rechecks |
| `original.rs` / `read.rs` | Selected active file root first; stale canonical identity read otherwise | Keep P0's old G2 Base coordinates and T0's InodeAttributes refresh; remove path scratch/fallback |
| `symlink.rs` | Selected target or InodeReadlink by serial | Keep main identity reads and target custody; do not derive a renamed path |
| `open.rs` / `directory.rs` | Ordinary regular FD follows live inode; directory FD selects names | Retain View, directory serial and current/last parent semantics without aggregate directory paths |
| `view_leases.rs` / `view_reads.rs` | Token-bound issued serial/kind/content facts and immutable View | Remove HeldEntry.path and child_path_active; preserve forged/stale/cross-lease refusal and response admission |
| `coherence.rs` / FUSE callers | Existing projected owner/reply/publication semantics | Adapt to target names/ancestry; preserve actual notifier choices, stopping and checked unmount |
| `overlay/snapshot.rs` | One selected G1 context, one Submission | Transfer same dirty-generation fund; clean capture without a fund explicitly reserves one |
| `active_reconcile.rs` | Only matching regular files collapse to saved Base | Preserve intervening nonzero F0, symlink target representation and directory edges; never overwrite NodeName/parent through file installation |
| `completion.rs` / `pages.rs` | One canonical command and exact same-fund physical custody | Port allocation/refund/finish order and scoped local resume together; no public Commit replay |
| External fixtures/proofs | Actual typed capability, actual precharged reserve | Rebase stale expectations on source; retain prior failures and rerun only affected proofs at the new identity |

For original file facts the target decision is:

```text
selected active inode exists?
        +-- yes -> I.base / I.metadata / selected extents
        |          old G2 source coordinates remain valid
        +-- no ---> stale canonical Node?
                     Inspect::InodeAttributes(filesystem Base, serial)

Never reconstruct a Node full path and inspect it at a newer filesystem root.
```

Directory handles and SDK held entries are part of this conversion. Removing
only `Node.path` would leave a hidden aggregate limit and repeated path memory
in those owners. Regular files can have several hard links; they must not
acquire a fictitious unique-parent invariant.
The main complete-ancestor guarantee also applies to ReadOnly component
traversal; do not restrict its collection/retention to active attachments.

Target source anchors: [NodeName](https://github.com/Ephemeral-AI-Lab/layerfs/blob/05eb5c14849f1b874383fd1600ba9288f103ad93/core/crates/layerfs-workspace/src/runtime/state.rs#L107),
[live ancestry](https://github.com/Ephemeral-AI-Lab/layerfs/blob/05eb5c14849f1b874383fd1600ba9288f103ad93/core/crates/layerfs-workspace/src/runtime/ancestry.rs#L9),
[path-free View resolution](https://github.com/Ephemeral-AI-Lab/layerfs/blob/05eb5c14849f1b874383fd1600ba9288f103ad93/core/crates/layerfs-workspace/src/filesystem/namespace_view.rs#L69),
[one-seal rename](https://github.com/Ephemeral-AI-Lab/layerfs/blob/05eb5c14849f1b874383fd1600ba9288f103ad93/core/crates/layerfs-workspace/src/filesystem/rename.rs#L517).
Donor seams: [file origin](../../../../crates/layerfs-workspace/src/filesystem/original.rs#L16),
[lease entries](../../../../crates/layerfs-workspace/src/runtime/view_leases.rs#L28),
[selected lease reads](../../../../crates/layerfs-workspace/src/filesystem/view_reads.rs#L117),
[C5 conditions](../../../../crates/layerfs-workspace/src/commit/active_reconcile.rs#L197).

## 4. Proposed file and folder structure

Keep the existing crate boundaries and focused implementation modules.
The tree below names responsibilities, not empty scaffolding to create now.
Reuse path-independent modules; adapt shared owners in place. Split a touched
responsibility only if real changes require it under the file ceilings.

```text
core/
  crates/
    layerfs-workspace/
      src/
        runtime/
          state.rs              main NodeName graph + adapted active state
          ancestry.rs           main live-chain and retained-ancestor checks
          host.rs               attach, exact capability, shared admissions
          attachment.rs         failed attachment custody
          lifecycle.rs          checked projection/close ownership
          coherence.rs          publication/reply/notifier ownership
          view_leases.rs        issued identities, no full namespace paths
          mod.rs                declarations/delegation only
        filesystem/
          namespace.rs          component grammar/live parent admission
          namespace_view.rs     immutable Base + selected ActiveSnapshot
          active_view.rs        serial/component effective resolution
          active_create.rs      fresh I/N/D and ready-handle publication
          active_remove.rs      tombstones, links, detach/unbound custody
          active_rename.rs      active patch + main component-edge move
          rename_preflight.rs   adapted main parent/replacement/chain facts
          active_names.rs       changed-name bookkeeping
          active_file.rs        verified content publication/Node update
          active_attributes.rs  portable attributes, indexed resident update
          original.rs           active file origins / canonical identity read
          read.rs, read_origin.rs, view_reads.rs
          open.rs, directory.rs, symlink.rs
          ...                   existing ordinary mutation entrypoints
        backing/
          active/
            page.rs, pages.rs, records.rs, keyed.rs, resolve.rs
            index.rs, splice.rs, hot_directory.rs, hot_cursor.rs, hot_path.rs
            pack.rs, reader.rs, extents.rs, reclaim.rs, compaction.rs
            generation.rs, generation/read_origin.rs
            retirement.rs, lifetime.rs
            mod.rs              declarations/delegation only
          metadata.rs, metadata/progress.rs
          payload.rs, budget.rs, directory.rs, segments.rs
          ...                   saved-observation/ledger pieces actually used
        overlay/
          snapshot.rs           active G1 capture + fund transfer
          ...                   required saved-result pieces
        commit/
          active.rs             captured content/namespace preparation
          active_source.rs      bounded grouped packed-source reads
          active_reconcile.rs   captured-set C5 patch
          completion.rs         known/unknown/local resume/fund ordering
          operation.rs, save.rs, stream.rs, directories.rs
          ...                   actual framing/observation implementation
      tests/                    adapt existing public external selections
      examples/                 actual SDK drivers/independent verifiers

    layerfs-bridge/
      src/contract/             additive capability/v2/lease contracts
      src/adapters/native/      codecs, client/connection deadline semantics
    layerfs-server/
      src/service/handler.rs
      src/service/save/content.rs
      src/service/save/file_stream.rs
      src/service/save/file_stream/origin_runs.rs
    layerfs-api/
      core/src/workspace_view.rs
      sdk/src/workspace_view.rs, workspace.rs, lib.rs
    layerfs-daemon/
      src/control_view.rs, control.rs, lib.rs
    layerfs-sandbox/
      src/session.rs            held-session deadline/custody
    layerfs-content/
      src/filesystem/           scoped ordering/charge repair if selected
      src/file/edit/            existing builder + scoped real diagnostics
    layerfs-storage/            keep target implementation/schema
    layerfs-history/            keep target implementation/schema

  benchmark/fs-bench-pro/
    families/
      init_namespace.py         existing sole Init implementation
      workspace_write.py        3x3 + original separate-write membership
      workspace_commit.py       clean/one-edit retained-state controls
      workspace_namespace.py    component/rename scaling evaluation
      workspace_mutations.py    mixed-operation workflow evaluation
      workspace_shell_package.py
      history_retention.py      adapter to existing C1/C2 Rust history driver
    shared/                     reuse setup/seals/invocation/receipt utilities
    registry/                   one canonical registry per implemented family
    writers/                    ordinary POSIX workload programs
    tests/                      focused registry/harness checks
    runner.py                   dispatch; no duplicated Init route

  docs/issues/273/phase45-merge/  this plan, workflows, baseline and evidence index
  docs/architecture/            same-commit source/algorithm descriptions
  target/<fresh-attempt>/        local-only append-only raw evidence
```

Do not port `rename_paths.rs`, NodePath or path-bearing View/HeldEntry storage
into this target. Physical backing directory/file paths and direct canonical
LogicalPath validation remain in their real owners.

Donor `generation.rs` is 968 physical lines, `pages.rs` 916 and `splice.rs`
936. Put namespace adaptation in its callers, not those nearly full storage
files. If actual growth needs a split, use a responsibility name and ordinary
functions; no adapter framework, numbered fragments or test feature. Product
files remain at most 999 physical lines, with 200-line declaration/delegation
caps for `lib.rs`/`mod.rs`.

### Compatibility is a coordinated patch, not a schema migration

| Unit | Files/responsibility | Target decision |
| --- | --- | --- |
| SaveFile contract | Bridge request/outcome/authorization, native metadata/response codecs | Preserve v1 opcode 20; add exact capability 28 and v2 save 29 from P0 |
| Service lowering | handler, content, file_stream and origin_runs | Fixed-record Service-local Base reads; no daemon-recursive RPC or result-sized Base spool |
| Native deadline | native client/connection and Sandbox session | Actual read timeout becomes Deadline; mutation/release loss keeps Unknown; no fresh post-deadline handshake |
| Public leases | API, Bridge view contract/codecs, daemon control_view | Existing additive token/entry wire; adapted native identity owner, checked release and response Budget |
| C1 file construction | existing file/edit builder | Keep canonical algorithm/profile; import scoped observation changes separately |
| C1 filesystem repair | independently selected ordering/membership/charge correction | Port on matching source inputs; no new root format or parent-index project |
| C2/History/Service prepared filesystem | target source already matches P0 in these production owners | Do not replace or migrate merely because they appear in the workflow |

At the named target, opcodes 21–27 for leases and 28–29 for v2 are unassigned.
Recheck the registry if the target moves. The internal v2 route and positive
attachment capability are already owner-approved; do not ask for that approval
again. Preserve exactly typed version 2 acceptance rather than assuming future
versions are compatible. See [the owner progress/ruling record](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5885112095).

Old v1 callers retain v1 semantics on the new Service. New LocalEdit clients
with an old/incompatible Service refuse before accepting writes that depend
on provenance. New lease callers against an old daemon fail explicitly;
they do not receive a fabricated lease or an internal test substitute.

The independent C1 repair is
`fbda0f0f1dbb6bb3ddd375694e3ea7edbab62ebd`. T0's `filesystem/update.rs`
and `validate.rs` match that repair's parent inputs. Port these exact
responsibilities together:

- `filesystem/update.rs`: retain unreachable-parent membership only for
  declared-new non-root parent candidates; a binding pass marks them bound.
  Unrelated positive child bindings need probes, not retained parent entries.
- `filesystem/validate.rs`: declared row/name/prefetch demand uses the actual
  `maximum_touched_serials()` allowance. Real Base-record memo/effective walk
  keeps the conservative `ordering_bytes / 1024` admission.
- `filesystem/validate/cycles.rs`: preserve cycle/reachability validation in
  its focused implementation file, with the same proofs and file ceiling.
- External `filesystem_ordering.rs` and architecture `04-filesystem.md`:
  carry parent-membership/rate boundary proofs and source description.

This repair charges the actual structures at their existing rates; it does
not enlarge Budget or erase alias/cycle work. By contrast, the canonical
file-content builder stays unchanged apart from scoped real observation.

Public lease entry results carry serial, kind, size, references, mode and
mtime. Requests use token and issued serial/component/range; canonical content
facts remain private selected-view authority, and the lease header carries the
filesystem Base. `HeldEntry.path` is private state. Removing it requires no
new public wire version: preserve opcodes 21–27 and existing authority/response
shapes without adding public content or metadata roots.

## 5. Implementation sequence with benchmark evaluation points

Use one construction owner. The three component agents can audit and review
namespace, backing/custody and compatibility/evidence concurrently; they do
not write competing versions of shared state or alter other PR heads.

The sequence below is dependency order, not a demand for a separate commit
for every row. In particular, the active authority, component adaptation,
capture and completion fund must form a coherent runnable patch before it
accepts LocalEdit writes. Combine dependent rows into one commit rather than
shipping partial authority or adding a temporary algorithm flag.

| Step | Concrete implementation deliverable | Evaluation before broad final validation |
| --- | --- | --- |
| P0: manifest | Freeze T0/P0 and path decisions; carry one-seal and selected canonical repair; establish isolated owned branch | Read-only diff/ancestry/format audit; no physical measurement |
| P1: compatibility | v1-preserving internal v2/Service unit; precise timeout semantics; selected independent C1 filesystem repair | One focused external v1/v2/capability/deadline check for changed boundary; no Workspace benchmark until coherent route exists |
| P2: active Workspace seam | Path-independent active backing plus component-only Node/View/lookup/mutation state; one authority; precharged completion; capture/C5 wired before accepting active writes | One real public Exec/Commit byte smoke, deep component access, then one prospectively selected small matrix diagnostic |
| P3: complete owners | Finish rename/remove/link/symlink/attrs, directory selections and identity-based public leases; G1/G2 origins and checked custody | Main 3-versus-67/unrelated-resident count diagnostic; selected dispersed/repeated benchmark cases; diagnose from counts |
| P4: full selected evaluation | Freeze functional source after shared-cause fixes; finalize family dispatch/registries without changing workloads | Full 3x3, original #248, native 8192, original lowering/headroom/live/failure proofs and selected history strides; no fastest-run selection |
| P5: frozen review | Independent verifiers/cleanup at matching identities, final required owning-workspace checks, source/LOC/compatibility report | Review functional outcome, work counters, retained raw times and explicit NOT_RUN/INELIGIBLE/deferred entries |
| P6: integration review | Publish coherent reviewed diff into the Phase 4.5 review line with selected-port provenance | No automated merge, PR approval, issue closure or release promotion |

P2/P3 are the semantic work: copying active files does not complete them.
Directory and lease paths must be removed before declaring component-only
integration complete. Ordinary overlapping, nonmatching and reordered writes
remain supported through the generic path. No shell command or case identity
chooses an optimization.

### The development loop

```text
ONE concrete source change / coherent seam
        |
reuse worktree-local locked build and compatible image layers
reuse validated prepared master; independent writable clone
        |
run the smallest selected evaluation that answers this change
  real public route + phase/call/byte/pack/page counters
  performance-only exploration may mark verifier SKIPPED
        |
inspect retained receipt and actual shared cause
        +-- correct mechanism -> next planned slice
        +-- defect -> retain attempt; fix shared cause at new source
                     run only its covering selection once

stable final source
        |
complete registered cohort + separate exact independent proof/cleanup
owning Core checks once -> reviewable integration report
```

Benchmarks evaluate the change during implementation; they are not held until
every negative case and whole-workspace unit suite has run. Do not add unit
tests that mirror each private helper or repeatedly run unaffected suites.
Prefer existing public external tests, exact byte oracles, ownership proofs
and count diagnostics where a semantic seam needs verification.

No unchanged performance arm is rerun for a better number. A documentation-only
revision or a new output path is not a new optimization. A meaningful changed
mechanism can have a prospectively declared next evaluation at its new source;
retain the old run, its failure and the reason for that next selection.

## 6. Baseline versus expectation

### Three baseline roles, with no best-run selection

| Role | Selection method | Use/limitation |
| --- | --- | --- |
| Port before/after | Unmodified T0 and new integrated target, one common prospective harness/profile | Relevant comparison for this adaptation; both complete sources must be sealed before their selected runs |
| Earlier active algorithm control | Frozen A0 at 48b51e874 | Distinct original question; campaign NOT_RUN; do not rename it as T0 |
| Donor engineering reference | Entire retained P0 cohort at test source a8528b9757de993eb8ee73b6d183ea8ee5badefa | Functional/context reference; raw numeric INELIGIBLE, not a qualified speed denominator |

Choose the complete source/method/cohort before execution. Do not choose the
best row from one attempt and another row from another source. Both arms use
the same actual public operation, fixture, oracle, worker limit, host/profile,
cache declaration and timing boundary. A binary/harness change must be visible
in identities. Do not quietly patch the frozen product to support a new API.

T0/A0 lack the new public held-view surface. They cannot impersonate the
same-Workspace retained-generation Commit controls using a detached Store,
native helper or private test bridge. For unsupported controls, record that
fact. Evaluate candidate-only functional controls or prospectively declare a
common-capability comparison source; do not manufacture a matched arm.

### Retained public 3x3 reference and target expectations

The following are exact retained observations rounded to six decimals, from
[the full baseline inventory](BASELINE-20260929.md#2-complete-pre-integration-3x3-reference)
and [its machine-readable metadata](baseline-20260929.json). Every donor cell
has scoped byte/cleanup PASS and numeric INELIGIBLE.

| Pattern | Writes | Donor Exec s | Donor Commit s | Donor complete s | Registered complete limit s | Donor pack loads |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| append |100|0.098460|0.025168|1.524716|15|2|
| append |512|0.825870|0.050040|1.722133|15|7|
| append |4097|4.965218|0.254103|6.019977|25|52|
| dispersed |100|0.122175|0.033284|1.034881|15|2|
| dispersed |512|0.946037|0.070427|1.858093|15|7|
| dispersed |4097|9.202495|0.583623|10.673892|25|209|
| repeated |100|0.141458|0.022740|1.021502|15|1|
| repeated |512|0.667767|0.022440|1.532209|15|1|
| repeated |4097|5.564823|0.030433|6.516553|25|1|

The unchanged matrix uses a prepared 10 MiB file, 100/512/4097 ordinary writes
and one canonical construction worker. Target expectation is exact final bytes,
known Commit, complete cleanup, the original command limits and preserved
packing/grouping behavior. Compare work trends and explain differences in
loads/physical pages; the old integer counts are reference observations, not
new arbitrary hard limits.

Raw timing equality and an invented speedup percentage are not acceptance
contracts. Component names should remove path allocation/rewrite work; active
packing/hot publication should retain their mechanisms. The new source still
pays real namespace checks, Base reads, C1/C2 construction and custody.

### Other required or proposed evaluations

| Selection | Existing reference/status | Expected target result |
| --- | --- | --- |
| Original #248 separated-4097, 8194-byte fixture | Complete 5.917701 s; Exec 4.750473 s; Commit 0.350837 s; 52 loads; functional PASS/numeric INELIGIBLE | Same original fixture/schedule, known Commit and independent bytes/cleanup within original 25 s complete limit |
| Native 8192 distinct writes | Complete 23.649186 s; Commit diagnostic 0.595642 s; 8 source windows, 824 loads, 7368 hits; scoped PASS | Same 8194-byte fixture/schedule, default 8 MiB Budget, exact bytes and checked release; retain native/live-Service route and original 10 s operation/60 s functional-command profile |
| Public SDK 8192 promotion | NOT_RUN; not the native receipt | If selected, register unchanged schedule/fixture on public Mount/Exec/Commit before execution; prospective 25 s complete exception and separate <10 s verifier; miss stays visible |
| Default-Budget10240 boundary | Known canonical G1/local C5 Capacity; G2 continues | Record actual unchanged-workload target result. Smaller NodeName state may admit more; never waste memory to preserve the old refusal |
| Clean/one-edit retained-state Commit | Exact registered SDK controls NOT_RUN | Real same-Workspace lease and selected generation; no substitute in-process/native control |
| Original 64 MiB lowering | Complete 29.478575 s; 67,108,662 final bytes; 8 literal replacements; 180,224 actual allocated/charged bytes | Original three edits, 64 MiB quota, 10 s Stage/60 s functional command; exact saved C1 bytes and accounting; no changed timeout/quota |
| Occupied headroom | Scoped functional PASS at original 2 MiB Stage/4 MiB Commit | Same-owner precharge/allocation/refund, no published/old-pin release to create headroom |
| Namespace local rename | Standalone3vs67count proof; new matched public cohort NOT_RUN | No descendant enumeration or resident path rewriting; cost follows changed keys/reached ancestors; exact old/new/held namespace state |
| Mixed mutations | Existing external functional selections; new timed family NOT_RUN | Correct create/write/resize/attrs/link/rename/remove/symlink interactions and known Commit, with prospectively fixed membership |
| Shell/package scale | Existing four-case registry, broader cardinality NOT_PROVED | Separate public create/update/delete/list corpus with declared counts; no file-capacity claim from single-file8192 |

The 10,240 outcome is a capacity boundary observation, not a required failure
at that exact count forever. Deterministic physical known-C1/local-C5 failure,
unknown outcome, pin and refund proofs remain required independently of whether
memory headroom improves. Any revised pressure selection needs its own frozen
identity and oracle; historical receipts keep their original outcome.

### Stride 10, 3, 1 belongs to history_retention

Keep `history-stride10`, `history-stride3`, `history-stride1` under
`history_retention`, preserving their 17/53/157 selected states. These are
retained-history/C1/C2 questions, distinct from private Workspace WRITE/Commit.
The adapter must invoke the existing Rust history backend rather than rebuild
history in Python or relabel it SDK/FUSE.

The current matched Core history cohort is NOT_RUN. The complete historical
v0.1.6 cohort is context only:

| Stride | States | Historical Commit sum s | Historical performance-phase wall s | Store allocated bytes |
| --- | ---: | ---: | ---: | ---: |
|10|17|11.371|98.0|49,344,512|
|3|53|24.815|241.7|64,024,576|
|1|157|64.108|617.6|83,947,520|

These are source 7fab1027a0061e8b932345d4fcd6ac22a089b155 observations, with
a different Workspace envelope and admission_eligible=false. Commit sum,
performance-phase wall and complete invocation are different boundaries.
The separate historical Core stride-1 diagnostic records operation 97.729521s
and complete 141.117076 s with its own representation and missing proof; it is
not a faster/better interchangeable baseline. See [the retained stride
inventory](BASELINE-20260929.md#5-history-stride-reference-and-the-unavailable-current-baseline).

Target expectation is exact retained states/roots/policy, correct differences
between consecutive selected states, known construction outcomes and complete
lane-specific verification. This port leaves C2/History algorithms unchanged;
do not promise a new stride speedup from private packing. Select stride-1
explicitly. Do not copy Workspace15/25 s limits to history, shrink membership,
split a timed claim after observing it, or extend its registered profile.
If an allowed command/profile cannot fit the bounded execution envelope,
retain NOT_RUN and the reason rather than disguising a shortened sample.

Release SDK Init remains a separate family: default 100/1000, explicitly
selected 10,000/100,000, locked release driver/verifier, existing sampled-content
oracle and parallelism exception. It is not a renamed many-file Workspace
mutation benchmark. Reevaluate it only when the port changes its actual path
or its final review explicitly selects it.

## 7. Space, time and memory analysis

These are source-derived expectations, not measured target improvements.
Let P be resident Nodes, d reached ancestor depth, B_name retained component
bytes, N_sub moved inherited descendants, H private index height, E_f final
file extents, D captured dirty identities, K_C5 affected C5 keys, A retained
physical owners, G selecting revisions, S_payload shared payload owners and
J pressure-compaction candidates.
W is the number of writes in the selected sequence, L_tiny the retained
one-byte pack records in the ideal packing comparison, and N_refs the total
selected packed-source references across upload windows.

### Namespace and mutation work

| Component | Corrected Phase 4.5 / current donor | Proposed integrated target |
| --- | --- | --- |
| Resident namespace space | T0: O(P + B_name). P0 restores 4096 bytes per Node capacity plus extended aggregate paths | Preserve T0 component term; add actual active/frontier/owner/lease charges |
| Rename local time | T0 constant-key COW plusO(d log(P+1)); P0 can scan descendants and rewrite resident paths | Constant logical N/I/D changes plus reached active-page/owner work and same ancestor check; independent of N_sub path work |
| Old selected directory/lease space | P0 also retains/clones aggregate paths | Immutable selection plus issued identity/component facts; no duplicated prefix per issued entry |
| Collection/ancestor closure | T0 worst local boundO(P * d * log(P+1)) | Remains paid work; NodeName does not make collection constant |
| Identity lookup/readlink | Serial+component canonical lookup, immutable name memo where valid | Same, with selected I/N overlay first; no full canonical path reconstruction |
| General overlap WRITE | Input and every affected extent/inverse/key/page matter | Preserve affected-work cost; no universalO(1) write claim |
| Hot eligible sequences | P0 avoids unchanged ancestor replacement through HotRef | Preserve conditional O(W) structural/page work within fixed admissions; full CPU/I/O includes other terms |

Live-chain validation can add O(d log(P+1)) to one reached lookup. Repeating
that check at every component can accumulate depth-squared traversal work.
Removing a fixed depth refusal does not make arbitrarily deep traversal free.

For tiny one-byte records, the pack payload component changes from one
4096-byte segment per retained tiny payload to ideal
`4096 * ceil(L_tiny / 80)` pack bytes. Full128-byte records fit22 per pack.
Index, partially dead packs, candidates, old pins, failed owners, reservations
and records add separate terms. Repeated writes can already reclaim previous
payloads; do not claim every old path was quadratic or retained every write.

```text
WRITE time includes
  input/actual comparison bytes
  + changed-key ordering and reached private pages
  + affected E/R/L references
  + indexed Node navigation log(P+1)
  + physical owner navigation log(A+1)
  + selecting pin navigation log(G+1)
  + reached shared quota scan O(S_payload)
  + actual create/write/readback/auth/unlink/notification I/O
```

### Construction and reconciliation work

| Component | Before / donor behavior | Target expectation |
| --- | --- | --- |
| Capture | Shared selected root/context; public maintenance/declaration clone | Preserve immutable G1 and live G2; total capture is not universally O(1) |
| Per-file upload | Legacy streaming cursor had bounded-window behavior; donor materializes O(E_f) extents and 24E_f descriptors | Retain explicit O(E_f) tradeoff; geometric growth charges old/new overlap |
| Grouped source | <=1024 references and 32 KiB scatter output per window | Fixed 73,728-byte grouping capacity on 64-bit, plus decoded PackRecords and old/new/readback overlap; no one-pack-page-only memory claim |
| Pack reads | Sum of distinct selected packs in each window | <=reference count, worst Theta(N_refs); same pack can appear in later windows |
| SaveFile v2 | O(E+L+Q log E) input/origin navigation plus C1/C2 | Retain actual backward Base replacement reads R_b, which can be Theta(final bytes); copies still pay Omega(copied bytes) |
| Canonical file construction | Existing CDC/whole/chunked, comparisons/hash/CAS | No new canonical algorithm or free Base reconstruction claim |
| C5 memory | O(D+sum E_f+K_C5), plus changed-name/declaration bytes and retained owners | Preserve charged map transfer and matching-file collapse; G2/nonfile conditions remain explicit |
| C5 ordering/pressure | O(K_C5 log K_C5) ordering; can scan/sort J partly dead candidates | Real affected installation/compaction work remains, including under State gate |
| Pure-move canonical Commit | C1 Base/subtree alias/cycle/reference validation remains | Same separate scope; parent-index/path-local C1 project stays deferred |
| Final pin release | Actual selecting cohort, payload maintenance and checked unlinks | No unrelated cohort sweep for active pages; no constant-total-cleanup claim |

For v2, E is descriptor count, L includes Local **and Zero** input bytes, and
Q is actual origin/window navigation. Wire size is `1+24E+L`; derived spool
admission is `80E+32+L <=8 GiB`. Service Base reads use bounded 64 KiB logical
windows and do not put backward Base bytes into a result-sized byte spool.
These bounds do not remove actual reads, C1 comparison/chunking or C2 work.

### Resource domains and the memory deferral

```text
charged Workspace/Host RAM Budget
  Node/component names + memo/cookies + lease/response/operation owners
  + active/frontier/physical/payload/retirement records
  + selected/candidate windows + O(E)upload/C5 maps

private physical quota
  actual allocated st_blocks*512 + outstanding reserved completion/page credit

separate external domains
  allocator/RSS, kernel cache, Linux VM, Docker backend, device, macOS host
  + Service input spool, C1/C2 owners and SQLite residency
```

Removing the donor's inline 4096-byte Node path eliminates roughly a 1 MiB
path-array term for the first 256-capacity allocation, replacing it with actual
Node/component ownership. This is not a measured 1 MiB RSS reduction: layout,
capacity, names and other owners remain. Smaller internal state can admit
additional files/edits under the same Budget, but no tested total-file capacity
or new hard limit follows from that source observation.

Preserve default 8 MiB Budget, existing physical quotas and the 208*4096 = 851968
byte completion credit, with 256 bytes separately charged for the fund object.
Dirty capture transfers that same fund; clean capture can need explicit
admission. Failed preparation can retain precharged unused credit. Partial
allocation returns unused credit to the same fund. Finishing the fund releases
its remaining reserved credit. Allocated physical-owner refunds follow identity/
block-checked actual unlink; currently selected, pinned and unknown allocated
owners stay charged.

Hot 8 cursors/64 nodes/1 MiB reservation,128 handles, 32 leases, lower 32 captures/
160 selecting revisions, index level 7, 4 GiB logical file and per-message bounds
remain finite. No total 1 MiB memory promise is made.

The owner explicitly directs correctness,3x3 and 8192 first, with additional
memory qualification deferred to [#283](https://github.com/Ephemeral-AI-Lab/layerfs/issues/283).
Do not spend this integration loop trying to qualify host memory/cache again.
Keep actual Budget/quota/physical accounting/pin/refund safety proofs. Record
available observations without treating missing peaks as zero. The repaired
same-FD observer is capability evidence, not a LayerFS memory or symmetric
VM/backend/device/host-cache contract. Raw timing remains INELIGIBLE when the
numeric method is unproved; that status does not prevent functional evaluation.

## 8. Benchmark organization and execution discipline

Use [the existing family proposal](BENCHMARKS.md#1-proposed-family-ownership).
Move definitions into the owning family and reuse common lifecycle/seal/
invocation/receipt code. Keep historical IDs and archived entrypoints; thin
compatibility forwarding must not create a second registry or measured route.
Family organization changes the harness seal for new runs, not old receipts.

| Family | Naming/ownership |
| --- | --- |
| `workspace_write` | Pattern/write-count/file-size axes; 3x3 and original #248 remain distinct registered cases |
| `workspace_commit` | Clean, one-edit and retained-state controls; same live Workspace selectors |
| `workspace_namespace` | Width/depth/resident population/descendants and actual component operations |
| `workspace_mutations` | Interactions and exact final state, not a renamed correctness runner |
| `workspace_shell_package` | Existing package cases plus separately registered many-file corpus |
| `history_retention` | Stride 10/3/1 case dimension and existing C1/C2 backend |
| `init_namespace` | Existing sole release SDK runner and registry |

For new IDs use `<family>-<scenario>-<dimension>-<value>-v<version>`, with
explicit units. Example prospective public case:
`workspace-write-distinct-writes-8192-size-8194b-v1`. Its exact schedule must
be copied from the declared 8192 selection and frozen before execution. This
name is a proposal, not a currently callable selection or new PASS.

Preparation/build reuse stays outside timers: validated master once, closed
independent writable byte-copy clone per post-init sample, compatible locked
worktree-local build/image seals, fresh live ownership and fresh output path.
Clone is not a cold-cache claim. Do not reuse mutated samples or let setup,
earlier phases or another arm credit timed reads.

Record source/product/tree, compiler/flags/lock, harness/workload/oracle, image,
host/profile, cache declaration, arm/selection order, expected/observed public
calls, FUSE requests, input/replacement bytes, physical/index/pack work, C1
visits/Saves where observed, cleanup and unknown custody. Missing counters are
unavailable, not zero. Active `a-*` and legacy `m-*` page domains must stay
separate; do not compare a changed counter aggregation as the same metric.

A timer must name Exec, complete composite Commit, Stage, C1 construction or
complete command precisely. Full SDK Commit includes file/metadata Saves,
filesystem C2 finish, conditional History publication and local installation;
an upload-only timer is not interchangeable. Verifier wall stays separate.

For exploration, select only the cases answering the current change, use
performance-only mode and label diagnostic/SKIPPED evidence. At frozen source,
run the complete registered cohort once per arm and the exact separate
verification/cleanup. Reuse only identity-matched existing PASS proof where
the contract explicitly permits it. Keep every failed/partial/ineligible run
append-only. No best-of, extra workers, warmed arm, workload reduction or
deadline/quota/buffer relaxation.

## 9. Minimal validation and final functional proofs

The fast loop does not need a new unit-test framework, private test hooks or
an exhaustive unit suite after every file edit. Compile the touched owners,
run one covering public external selection where the seam changed, and use
the selected benchmark receipt to decide the next implementation step.
Diagnose failures from output/source rather than repeatedly probing a passing
selection. Fix the shared cause, then run its covering command once at the new
source.

At the frozen integration source, complete the affected proof set:

- Component-relative lookup/list/readlink; cached and never-resident moves,
  deep/long FD traversal, replacement/move-back, ancestor Forget, detached
  held directory refusal, cycle refusal and live/last `..` semantics.
- A prospectively declared public component traversal deeper than 256,
  using short relative names/directory FDs and an independent serial oracle.
  The existing 4,123-byte proof crosses aggregate bytes but alone does not
  prove removal of an aggregate component-count refusal. Do not issue one
  giant pathname or replace the command with a private test entrypoint.
- Untimed 3-versus-67 descendant and unrelated-resident diagnostics. Require no
  descendant path enumeration/rewrite; report actual active page work and C1
  Commit visits separately. Do not require the old four legacy page writes
  as an invariant of a different backing representation.
  Carry actual C1 filesystem visit and saved-file observations where required;
  a pure directory move should save zero file payloads, while Base/subtree
  validation still remains paid and separately reported.
- Original 3x3 and #248 public routes, native 8192 and actual 10,240 boundary result,
  independent old/new bytes, known successful Commit and complete cleanup.
- Exact original 64 MiB three-edit lowering and real public reordered/duplicated
  ordinary Base copy, followed by known Commit and old/new/G2/pinned oracles.
- G1held through save/construction, later G2ordinary writes, unchanged versus
  intervening regular revisions, fresh files, symlinks, directory bindings,
  hard links, open-unlinked files and exact Base-coordinate preservation.
- Original 2 MiB Stage/4 MiB Commit occupied-headroom cases, actual physical blocks,
  same-fund transfer/refund, unrelated owner/old-pin preservation, deterministic
  known-canonical/local-C5 failure, scoped local resume, lost result and cleanup.
- Authenticated capability/compatibility/version/count/EOF/root refusals;
  actual read deadline versus uncertain mutation/release; no automatic replay.
- Public32lease admission, forged/stale/cross-lease entries, exact selected
  old views, checked release, fixed 16 MiB response-Budget refusal and stopping/
  unmount custody. A historical source-specific threshold is not silently
  imposed after allocations change; any revised selection is prospective.
- Correct readable capability fixture and the two actual inherited-move
  `agent_route` SDK cases using the final immutable daemon image. Early return
  without `LAYERFS_TEST_IMAGE` is not live proof.

The existing SDK stopping receipt is already scoped functional PASS. Reuse it
as source context and rerun the affected route at the final port identity; do
not describe the older NOT_RUN handoff as current. The independent 32 KiB pinned
SDK-read Io observation remains unresolved. 16 KiB byte oracles do not prove 128 KiB
support or dispose that observation.

The broader #248 acceptance is intentionally outside a claim that 4097/8192
finish the issue. Its remaining objectives include more than 65,535 final runs
with count/sentinel boundaries, exact root/mapping compatibility, final-state
aggregation of adjacent/repeated/Zero/truncate/no-op edits, two heads with G2,
cursor/page visits, and write progress across lowering, descriptors, transfer,
remote construction and reconciliation. Current O(E_f) upload arrays are a
disclosed limitation for its streaming/resource objective. Preserve malformed/
EOF/unknown custody and its numeric method clauses; transfer unfinished scope
explicitly if the owner later narrows closure. Its functional allowance,
registered 15/25-second performance limits and 30-second product Exec request
are different bounds. Removing the product Exec ceiling belongs to the
separate runtime/control scope of #249, not this port.

Final required Core checks are performed once at the frozen source, using
worktree-local targets and repository-root ARMv8 build flags:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace
cargo +1.85.1 check --manifest-path core/Cargo.toml --locked --workspace --examples
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace --all-targets -- -D warnings
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
git diff --check
```

Use bounded owning-package/test-target components if a whole command would
exceed three minutes, retaining coverage rather than stretching its deadline.
Linux FUSE/ext4/public SDK routes require their actual external environment;
host ignores are not their proof. Build only needed binaries during iteration;
new SDK Init measurements always use locked release driver/verifier. Preserve
one construction worker outside Init's existing exception, no third-party
patches, no Workspace fsync and no test code/features in product `src/`.
This repository has no CI and no retired-preflight replacement.

## 10. Issue closure and what remains deferred

Issue state is a dated observation. Closing work is based on its full declared
acceptance or an explicit owner scope disposition, not ancestry, compilation,
an attractive raw number or a different finite stress case.

| Issue/PR | Current state | Action after this port |
| --- | --- | --- |
| [#264](https://github.com/Ephemeral-AI-Lab/layerfs/issues/264) | OPEN | Closure candidate only after integrated component-only/no aggregate refusal/no descendant or resident-path rewrite, charged ancestry, held-view/public deep-route and scoped count proofs; state remaining C1 Commit cost and obtain review of that scope |
| [#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248) | OPEN | Close only when its broader declared final-run/phase-liveness and real public operation/Commit objectives are proved or explicitly dispositioned;4097/8192 alone do not close the whole issue |
| [#245](https://github.com/Ephemeral-AI-Lab/layerfs/issues/245) | OPEN parent | Record Phase 4.5integration milestone; retain open package-scale/file-run/cardinality acceptance until its own proof/disposition |
| [#256](https://github.com/Ephemeral-AI-Lab/layerfs/issues/256) | OPEN | Broader many-file create/update/delete/list/capacity work stays NOT_PROVED; registered scalar-write success is not that evidence |
| [#273](https://github.com/Ephemeral-AI-Lab/layerfs/issues/273) | CLOSED, completed — scoped handoff | Do not close again; link new integration source and proof, preserving original scope/status |
| [#252](https://github.com/Ephemeral-AI-Lab/layerfs/issues/252) | CLOSED, completed | Preserve permanent removal of direct Workspace range API/ioctl and forbidden carriers |
| [#270](https://github.com/Ephemeral-AI-Lab/layerfs/issues/270) | CLOSED, not_planned/deferred | No implementation PASS; acceptance transferred to #276; no canonical parent-index work silently added here |
| [#276](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276) | OPEN deferred tracker | Update integration section with exact source/proofs; leave umbrella open while C1/scale/method items remain deferred or need disposition |
| [#283](https://github.com/Ephemeral-AI-Lab/layerfs/issues/283) | OPEN, memory observation | Keep owner-directed deferral; actual allocation/custody correctness remains required; no numeric/release promotion |
| [PR #269](https://github.com/Ephemeral-AI-Lab/layerfs/pull/269) | OPEN/DRAFT | Preserve head; review the new selective integration against Phase 4.5, including corrected foundation; no automatic merge |
| [PR #274](https://github.com/Ephemeral-AI-Lab/layerfs/pull/274) | OPEN/DRAFT, older donor | Reference source/provenance only; do not merge/retarget/update another owner's head as an implementation shortcut |

The [#270 disposition](https://github.com/Ephemeral-AI-Lab/layerfs/issues/270#issuecomment-5871949010)
transfers unimplemented acceptance to the common deferred tracker. Closed
status does not prove path-local C1 work. The latest [#276 owner/progress
record](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5885112095)
preserves approved v2 and correctness-first evaluation; it does not rewrite
old numeric receipts or authorize merging/releasing from this plan.

## 11. Review deliverables and stop conditions

The integration is ready for a concrete functional review when its new source
preserves both lanes' intended behavior:

```text
main component-only namespace and charged ancestry
        +
side verified packing/hot/generic active publication
        +
exact old/new bytes, live G1/G2 and identity-based selected owners
        +
precharged physical completion + checked pin/refund/failure custody
        +
new-source selected evaluations, exact separate proofs and required checks
```

The review bundle must include the port manifest (reuse/adapt/exclude and
source hashes), compatibility matrix, new product/test/harness/image seals,
baseline role for each selection, full receipt index with failures, expected/
observed work and bytes, cleanup custody, issue scope table and remaining
NOT_RUN/NOT_PROVED/INELIGIBLE observations. Local `core/target/` raw evidence
is gitignored; publish a committed source-pinned report/index without calling
those local paths GitHub raw-evidence URLs.

Each product algorithm/format/boundary change updates its architecture
description in the same commit. Each commit compares exact first-parent and
staged production snapshots with `python3 tools/production_loc.py --json
--root <snapshot>`, reporting reference/Core/combined before, after and signed
delta. Count the constructed target tree; current documentation-branch totals
are not an estimate of the future port. Source moves are relocation, not
algorithmic improvement. Preserve actual line caps and locked dependencies.

A new incompatibility, unproved acknowledged-write ownership, unavailable
required backend or unsolved target/side semantic contradiction is an explicit
blocker with a precise next executable step. Do independent engineering work
while seeking a genuinely new owner decision. Do not repeat the already
approved v2 question, block construction on deferred host-memory exploration,
or call a promising prototype complete. The plan and review bundle are not
permission to merge, approve a release or close incomplete issues.
