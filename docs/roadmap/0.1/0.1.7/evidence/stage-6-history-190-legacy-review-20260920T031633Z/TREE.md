# Tree and validation comparison

> **Status: Research; informative and not a product contract.** Read-only source review for #190. No new performance run, build, test or product edit. Owner considers one second worthwhile and accepts a small allocation miss for a worthwhile time saving.

## Disposition

**One narrow fallback candidate is worth attributing before implementation: retain authenticated absence inside validation, instead of repeating point reads for the same absent inode.** Its benefit is not measured; screen it only after the stronger provider/storage candidate from the companion review. The whole validation interval is 2,794,811,126 ns in the retained baseline, so there is enough phase budget to investigate, but no claim that this candidate saves one second.

**Do not pursue directory-topology summaries for this stride10 case now.** The retained trace reports zero directory pages read by validation and only 359 entries examined, all in the initial build. The potentially expensive whole-base alias walk is real code, but it was not the cause of this retained run's residual.

All successful PR194/195/196 changes remain applicable. The rejected `zero_count_serials` New-row filter stays rejected: its observed operation reduction was 156,679,583 ns and its target-phase reduction 53,181,962 ns. The candidate below concerns another phase and removes repeated individual reads; it does not repeat that experiment.

## Source identity and reading method

`L:path:line` denotes historical `7fab1027a0061e8b932345d4fcd6ac22a089b155`. `C:path:line` denotes `4391f66d8f39fc4d179caf557176021fc6f2c700`, the current successful product plus retained rejected experiment. Resolve each with `git show <revision>:<path> | nl -ba`.

`git diff 7fab1027a HEAD --` is empty for the legacy files cited here: `crates/layerfs-workspace/src/changes.rs`, `crates/layerfs-workspace-core/src/namespace.rs`, and `crates/layerfs-content/src/tree/batch.rs`. Current `crates/` therefore matches the historical text for those claims. The historical compiled revision differs from the report-label revision; the earlier [composition reconstruction](../stage-6-history-190-20260919T225614Z/squad-s4/V016-COMMIT-COMPOSITION.md) demonstrates product/harness equivalence and retains the actual receipt identity.

## Actual caller flows

```text
v0.1.6
  exec / mutable Workspace
    resolved names + mutation checks + dirty-node tracking
        |
        | frozen, already constrained Workspace changes
        v
  Commit -> CandidateInputs::build
    changed files -> dirty nodes / changed directory bindings
                 -> bounded FrontierInodes + reference journal
                 -> batched sorted inode-tree merge -> publish

v0.1.7
  corpus transition -> independent FilesystemInput
        |
        v
  update_filesystem / run_body
    validate arbitrary final-state input against authenticated base
      new-identity absence -> kind/parent checks -> topology checks
    batched directory-parent reads -> changed-binding merge
    reference reduction / release -> batched sorted inode-tree merge
        |
        v
  Store admission and finish
```

Legacy checks mutation preconditions before Commit: stale name/revision checks at L:`crates/layerfs-workspace-core/src/namespace.rs:644`; directory hard links are rejected at `:652`; directory rename descendant detection uses maintained paths at `:745`. Newly created directories record their parent at `:633`. Commit loops `self.live.dirty`, not the entire namespace, at L:`crates/layerfs-workspace/src/changes.rs:742`; changed directory bindings are applied at `:791`; reference application and final inode merge occur at `:854` and `:862`. The frontier first consults pending/spilled records and accepts a caller-supplied authenticated base record at `:2480`; references batch base demands at `:3093` and `:3155`.

Core accepts a standalone final-state batch. C:`core/crates/layerfs-content/src/filesystem/update.rs:162` runs `validate::check` before mutation. C:`core/crates/layerfs-content/src/filesystem/validate.rs:134` authenticates the new-ID absence claim, `:158` checks binding/parent kinds, `:228` checks aliases, and `:259` checks effective cycles. Those checks cannot be deleted because the legacy mutable Workspace already enforced related invariants at a different boundary. This is a semantic/boundary difference, not evidence that validation is unnecessary.

Both implementations already perform sorted incremental tree updates and reuse untouched subtrees. Legacy merge reads children in bounded batches at L:`crates/layerfs-content/src/tree/batch.rs:671`; core does the same at C:`core/crates/layerfs-content/src/filesystem/sorted/merge.rs:239`. Core's untouched-subtree exit is at `:180`. Both still authenticate immediate children needed for aggregate checks. It would be wrong to describe legacy as using summaries to skip all sibling reads while core alone scans them, or to describe core as rebuilding the entire inode tree for every change.

## Candidate 1 — authenticated absence reuse in validation

**Classification: worthwhile attribution screen; benefit NOT_MEASURED.** This nominates a subpath measurement, not a product experiment justified by the whole validation interval.

C:`core/crates/layerfs-content/src/filesystem/validate.rs:545` checks all new identities in batches of 64 and discards the absent results. `:142–156` prefetches parent/child demands. `ValidationState::prefetch` at `:469–485` stores only `Some(InodeValue)`, while `lookup_optional` at `:498–512` makes a single-key `lookup_many` call for every miss and again stores only `Some`. Both the initial binding classification at `:195` and cycle-kind classification at `:614` consult this path, even for newly allocated regular files whose absence was just authenticated. This is a concrete repeated-roundtrip mechanism within one immutable-base operation.

Legacy's allocator reserves a new serial range before admission at L:`crates/layerfs-workspace/src/changes.rs:607–634`; its dirty-node construction distinguishes new nodes through `canonical: None` at `:765–768` and does not perform core's generic new-identity validation passes. That difference explains why the standalone core needs the first check, not why it needs repeated checks.

The minimal hypothesis is to reuse a successful absence verdict for the same inode-table root within this validation call. It must preserve mandatory allocator checking, kind fallback to caller values, alias/cycle detection, malformed-base failures on first acquisition, and declared work accounting. Avoid a global cache or trust-me flag. A representation holding both found and absent verdicts must be bounded by this operation's demanded serial set; negative entries grow memory relative to the current positive-only memo, so account that growth explicitly. The current comment at `:441–445` deliberately promises unchanged absent-record accounting; a change must address that promise rather than silently changing counters.

The shape improves repeated single-key demands from approximately one tree descent per occurrence to one authenticated verdict per distinct serial (or one grouped descent for a batch), without changing the tree algorithm. It does **not** imply proportional savings: C:`core/crates/layerfs-content/src/filesystem/inode/read.rs:132–139` stops demands above a branch's maximum without fetching a leaf. Many newly allocated serials therefore pay only a root read. Reintroduced paths can reuse lower serials in the harness (C:`core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs:565` and `:674`), so not all absent IDs have that cheap shape.

Decisive next evidence: split validation's first absence proof, prefetch, and subsequent point misses by found/absent result; attribute provider pages/waves and elapsed time to those calls. A candidate must demonstrate fewer actual read waves and at least one second of operation reduction with corresponding validation/provider reduction, unchanged outputs and adversarial validation tests, then stride3 confirmation. If repeat-absence work totals less than one second, or changed memory/work bounds outweigh the benefit, stop. Do not credit a coincident storage-admission fluctuation as this mechanism's saving.

## Candidate 2 — topology summaries / changed-path validation

**Classification: unsupported as a stride10 optimization target; counter-evidence rejects the proposed attribution for this retained run.**

Core's alias check can walk the base namespace once at C:`core/crates/layerfs-content/src/filesystem/validate.rs:304–379`. The effective-cycle check starts a fresh subtree traversal for each rebound directory at `:609–666`. The first is proportional to reachable base entries when triggered; the latter can revisit overlapping subtrees, with a per-walk rather than aggregate operation ceiling (C:`core/crates/layerfs-content/src/filesystem/limits.rs:59`). Legacy instead relies on Workspace mutation validation and dirty frontier processing described above. This is an algorithmic distinction worth understanding for workloads that move existing directories.

However, the retained stride10 trace has **directory_pages_read = 0**, **entries_examined = 359**, with every post-build state reporting zero entries examined. No amount of optimizing these unexecuted base-directory walks can explain or remove a second of this run. Persisted reverse-parent or subtree summaries would add semantic, storage and validation obligations for a path with no demonstrated benefit here. Reopen only for a measured workload that actually exercises these walks; preserve rejection of cycles, multi-parent directories and false caller claims.

## Retained evidence, exact arithmetic and limits

All values below are re-derived from the existing phase-only baseline in [the previous screen](../stage-6-history-190-screen-20260920T024251Z/README.md), not new measurements. The baseline has temporary phase instrumentation and its original uncontrolled-cache diagnostic qualifications. Its operation is 22,615,178,250 ns. The validation subphase is 2,794,811,126 ns, within filesystem 9,536,586,040 ns; these nested intervals must not be added.

| Validation counter | Retained sum |
|---|---:|
| inode demands | 67,826 |
| inode pages read | 49,084 |
| read waves | 47,541 |
| directory pages read | 0 |
| entries examined | 359 |

These counters do not distinguish positive and negative lookups. They support measuring the repeated-absence hypothesis, not attributing all 47,541 waves to it. Current memoization of positive records, batched parent lookup, bounded parent reuse, pooled physical-group reuse, and cached group-location SQL are **implemented already** and must not be proposed as new discoveries.

Reproduction, from repository root (reads retained JSON only):

```python
import collections, json
from pathlib import Path
raw = Path('docs/roadmap/0.1/0.1.7/evidence/'
           'stage-6-history-190-screen-20260920T024251Z/'
           'runs/baseline-history-stride10/raw')
totals = collections.Counter()
def walk(node):
    totals[node['name']] += node['elapsed_ns']
    for child in node['children']:
        walk(child)
walk(json.loads((raw / 'timing.json').read_text()))
assert totals['validate'] == 2794811126
counters = collections.Counter()
for line in (raw / 'trace.jsonl').read_text().splitlines():
    row = json.loads(line)
    marker = '.filesystem.validation.'
    if marker in row['key']:
        suffix = row['key'].split(marker)[1]
        value = int(row['value'])
        counters[suffix] += value
        if suffix == 'directory_pages_read':
            assert value == 0
        if suffix == 'entries_examined':
            assert value == (359 if row['key'].startswith('history.state.1.') else 0)
assert counters['read_waves'] == 47541
assert counters['inode_pages_read'] == 49084
assert counters['directory_pages_read'] == 0
assert counters['entries_examined'] == 359
print(totals['validate'], dict(counters))
```

Historical Commit 11,370,679,212 ns and its namespace counter 342,355,542 ns are not matching timer scopes for the current 9,536,586,040 ns filesystem interval: legacy's directory/metadata work also appears in its content interval, and mutation validation happened in exec. The earlier reconstruction documents overlap in historical counters. No subtraction here assigns a causal legacy-to-core gap.
