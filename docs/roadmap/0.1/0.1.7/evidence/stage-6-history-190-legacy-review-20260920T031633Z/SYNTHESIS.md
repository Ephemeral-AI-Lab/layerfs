# #190 legacy-code comparison: next experiment recommendation

> Status: Research; informative and not a product contract. Source-only review; no new performance measurement or product change.

**Do not stop yet: run one isolated group-compression experiment next.** Compare current group level 19 with the legacy level 1 while holding payload level, workspace, limits, workers and harness fixed. This is the lowest-complexity treatment with existing evidence near the owner's one-second threshold. The archival evidence measures codec CPU on old groups, not current operation savings; the proposed live pair must decide whether the tradeoff is worthwhile.

A second concrete source finding is that legacy extracts a demanded group range from SQLite while the replacement pooled reader copies the full pack BLOB. The retained lane records 79,784 pooled pack acquisitions and 7,378,994,999 returned bytes. This is an algorithmic follow-up worth screening, but preserving current full-directory validation requires more than a slice query or moving a cache check.

Keep the successful changes from PRs #194, #195 and #196. Keep the rejected zero-count experiment in PR #197 rejected. The owner's rules remain: one second is worthwhile, and a small allocated-storage overage is acceptable when accompanied by good time reduction. No default changes follow from this review. The proposed diagnostic varies only group level in an isolated candidate; it must not enlarge a cache, weaken validation or add a helper worker.

## What the three reviews establish

Read [TREE.md](TREE.md), [STORAGE.md](STORAGE.md) and [BOUNDARIES.md](BOUNDARIES.md) for exact source references, identity checks and counter-evidence. The current review pin is `4391f66d8f39fc4d179caf557176021fc6f2c700`; historical measured product/harness source is applicable at `7fab1027a0061e8b932345d4fcd6ac22a089b155`, with raw compiled identity `ac729dfeb4ee923b0a42106a53d1c7de4cd4cfcb`. Current reference production code matches that historical source; five later example/test additions do not alter the implementation. The release tag is not substituted for the timed identity.

| Direction | Grounded finding | Timing evidence | Decision |
|---|---|---|---|
| Selective pooled pack reads | Legacy reads the requested BLOB range; core copies the full pack on this path. | 7,378,994,999 bytes / 79,784 acquisitions; containing provider interval 8,572,820,583 ns. Removable subpath time NOT_MEASURED. | **Second direction.** First measure fetch/parse cost; preserve full-directory validation and all bounds before implementing selective reads. |
| Lower group compression level | Legacy uses level 1; current group codec uses level 19. | Two separate archival offline recompression logs report 1,011,835,000 and 1,425,492,000 ns more codec CPU at level 19 for 262,222 fewer stored bytes. ARCHIVAL_UNMATCHED; not current lane time or allocated bytes. | **First experiment.** Lowest implementation complexity; test current live time/space tradeoff. No default change from archival evidence. |
| Reuse authenticated absence during validation | Existing record memo does not retain absence; some New IDs can be looked up again. | Validation interval 2,794,811,126 ns; negative-lookup subpath NOT_MEASURED. High IDs can stop near root; redundant logical calls need not be expensive. | Defer. Screen removable work first; do not infer a second from the whole validation interval. |
| Subtree summaries / avoiding full-directory validation scan | Current API supports checks needed for standalone final-state updates; legacy consumes a validated mutable workspace frontier. | Retained stride10 validation reads **0 directory pages**; 359 entries examined belong to the initial build. | No evidence this is the current lane's bottleneck. Do not implement summaries for this result. |
| Producer/admission streaming | Legacy overlaps construction/admission; current harness stages C1 output then admits it. | No isolated current streaming effect; file-content construction totals only 1,618,536,750 ns. | Defer: concurrency and lifetime/ownership changes have greater complexity and constraints. |

Neither the pack byte count nor sampled profile residence is disk I/O time. It is bytes acquired into application buffers and a source path to inspect. Native profiling previously observed 550 provider samples in pack execution/copy/other and 43 in pack preparation, supporting investigation of data acquisition rather than another preparation-only micro-change. These were samples from the earlier pinned executable, not exact seconds or the latest profile.

## The larger algorithmic follow-up

```
v0.1.6 group read                 Current core pooled group read
----------------                ------------------------------
locate demanded group           locate demanded group
          |                               |
read its SQLite BLOB range      SELECT whole pack BLOB into Vec
          |                               |
validate / decode group         validate complete directory / use group cache
          |                               |
return demanded object          extract / decode required group and object

Possible follow-up: replace full-pack acquisition with a bounded group read;
keep object identity, full-group authentication, limits and error checks.
```

This is a work-reduction hypothesis, not a request for another long-lived cache. Current catalogue entries do not contain byte offsets/lengths; the route must obtain the BLOB length and parse the complete bounded header/directory. Existing rusqlite already enables the BLOB API. Preserving one parser and its full rejection behavior is the implementation cost that puts this behind the codec experiment. Existing decoded-group reuse remains bounded at its current capacity. The treatment must explicitly preserve validation on both cache hits and misses; moving a cache check earlier is a separate change and must not silently remove pack bounds or missing-pack checks.

## The available time budget

These are **existing** measurements from the phase-only baseline retained in [PR #197's results](../stage-6-history-190-screen-20260920T024251Z/results.json), not a new run or an improved result. [measurement-budget.json](measurement-budget.json) records the source hash and exact arithmetic. Nested provider time is subtracted from filesystem time before summation.

| Disjoint part of the measured operation | ns |
|---|---:|
| C1 file-content construction | 1,618,536,750 |
| Filesystem provider reads | 8,572,820,583 |
| Filesystem work outside provider | 963,765,457 |
| Store begin + accept + finish | 11,120,818,665 |
| Remaining child work, including caller bookkeeping and Store creation | 339,236,795 |
| **Operation sum** | **22,615,178,250** |

Arithmetic residual is zero; causal attribution of all intervals is not complete. A one-second provider improvement requires removing 1,000,000,000 / 8,572,820,583 of that interval. Entire filesystem work outside provider is below one second in this receipt, so a change confined there cannot save a second in that observation. None of these fractions is a forecast.

## Why legacy's headline still cannot be subtracted into a fix

Legacy Commit is 11,370,679,212 ns in the retained history. This recent core receipt is 22,615,178,250 ns: a descriptive difference of **11,244,499,038 ns**, not a matched effect. Different state preparation, cache state, runtime, semantics and unrecorded effective legacy worker counts remain. Same corpus does not mean identical canonical objects or identical timed work.

A newly emphasized timer trap: legacy's **342,355,542 ns namespace total excludes dirty-directory processing charged to its content interval**. The legacy content interval also contains concurrent content construction and admission. Comparing that small namespace field with the whole current filesystem interval would invent an algorithmic gap. The existing [historical reconstruction](../stage-6-history-190-20260919T225614Z/squad-s4/V016-COMMIT-COMPOSITION.md) and this round's boundary review explain the overlap. No arithmetic can recover unrecorded exclusive legacy intervals.

## Smallest decisive next experiment

1. Freeze one treatment: `GROUP_LEVEL = 19` versus `1`. Keep payload level 3, encode workspace, integrity flags, window cap, read/write limits, workers, candidate policy and harness unchanged. Record both exact source and binary identities.
2. Prove both representations decode through the unchanged reader and preserve canonical identities, group bounds, corruption rejection and object counts. Complete Store file bytes are expected to differ. Compare every saved state root and perform the existing separate identity-matched verifier across the selection; disclose its sampled, non-exhaustive path coverage.
3. Collect one current-source stride10 sample per arm under existing shared locks, quiet preflight and declared cache state, using fresh append-only outputs. Record operation, save interval, encoder elapsed/CPU where actually available, encoded/pack bytes, apparent/allocated Store size and memory. Missing exclusive codec timing stays NOT_MEASURED; archival CPU is not substituted. No legacy rerun, stride1 tuning or ceiling change.
4. Judge the actual time/space tradeoff. One second is worthwhile; a small allocation overage alone is not a rejection reason. Do not assume old 262,222 encoded bytes equal the new allocation delta. If stride10 justifies retention, confirm once on stride3 and run required core checks. Otherwise archive the rejected treatment and keep level 19.

## Review disagreement and final ordering

The coordinator initially preferred selective pack reads because they remove measured repeated copying. STORAGE recommended the group-level pair first because it is a one-constant experiment. BOUNDARIES recorded the disagreement and cautioned that neither direction demonstrates a one-second gain. On reading the complete-directory validation and catalogue constraints, the coordinator adopts STORAGE's ordering: **codec pair first; pack fetch/parse screen second**. This changes prioritization, not a timing conclusion. TREE's absent-record memo remains deferred pending attribution of that specific path.

A source review does not establish a speedup, so the proposed experiment remains **NOT_RUN**. No performance, verification, build or full-suite commands ran in this round. Product LOC is unchanged; all previous admission gaps remain. This round identifies a cheap time/space experiment and a substantive read-path follow-up, rather than asserting that the historical 11,244,499,038 ns descriptive difference is recoverable.
