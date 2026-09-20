# S2 — tree-update path

> Status: Research; informative and not a product contract.

Source-only investigation for #190. Source pin: `9f35c49ad62956f131dc2676787f99d69659686e`. All file:line references below address this exact Git snapshot, not the later S1-instrumented working tree. Product source was clean against HEAD when inspected. No product edits, benchmark invocations, builds or commits were made by S2. S2 owns this document only.

## Finding and its limit

There is a definite whole-current-tree scan **in harness input assembly**, and there are several distinct product base-tree reads. Source does **not** establish that every update traverses every inode or that a full walk explains the measured slowdown. The sorted update reads all children of an entered branch before deciding whether each child is unchanged, but an untouched child does not recursively visit its descendants. Validation contains conditional whole-tree and subtree walks with explicit refusal bounds. A counter or a span is needed to establish which ran and its cost.

Consequently H1's proposed mechanism is plausible but is not proved by the base-size correlation. The per-state tree operation must first be split into validation, directory merge, reference reduction, inode merge and residual. The public timed APIs already permit this without a product change.

## Call graph, timer placement and named work

Paths prefixed `C/` mean `core/crates/layerfs-content/src/`; `S/` mean `core/crates/layerfs-storage/src/`; `H/` mean `core/benchmark/fs-bench-pro-storage-content/src/`. Each reference can be reproduced with `git show 9f35c49ad62956f131dc2676787f99d69659686e:<expanded-path> | nl -ba`.

| Boundary | Source and exact entry | Work and implications |
| --- | --- | --- |
| Corpus transition | `H/ops/history.rs:1515` | Reads transition and blob inputs between state children, inside root, outside operation sum. |
| State child | `H/ops/history.rs:1550` | Contains content, advisory preparation, input assembly, filesystem build/update, save, bookkeeping, and child-local destruction. |
| Content | `H/ops/history.rs:1567`, construction calls at `1631` and `1639` | Iterates changed entries; construction emits to harness `TreeStore`. Predecessor cursor gets a separate StoreProvider (`1583`). |
| Full current-tree input scan | `H/ops/history.rs:580`, loop `618` | `filesystem_input` scans every `transition.tree` key, enumerates its ancestors and builds `directories_now`. This is harness work inside the state timer, not a persisted-base read. Then changed paths become bindings/inodes (`649`), followed by sorts (`702`, `708`). |
| Filesystem entry | `H/ops/history.rs:1884`; `C/filesystem/update.rs:67`, `92` | First state calls build, later states update, with a fresh StoreProvider per state and FileBacking available. Both currently choose disabled product phases. |
| Validation | `C/filesystem/update.rs:160` → `C/filesystem/validate.rs:121` | Input checks, root load, allocation checks, grouped base demands, alias/cycle checks. |
| Directory merge | `C/filesystem/update.rs:179`, `239` | Only supplied directory updates; existing parents need inode lookup (`214`). Sorted engine reads/authenticates pages and emits updated pages. |
| Reference setup and release | `C/filesystem/update.rs:170`, `282`, `310` | Bounded reducer, base lookups for changed directory values, touched-serial zero-count determination (`455`), then zero-count subtree release. These are not all enclosed by the existing `references` product span. |
| Reference finalization | `C/filesystem/update.rs:337`; `C/filesystem/references/reduce.rs:270` | Consolidates ordering runs, transfers pending rows and constructs a lazy FinalRows stream. |
| Inode merge | `C/filesystem/update.rs:346`, `354` → `C/filesystem/inode/update.rs:50` → `C/filesystem/sorted/finish.rs:165` | Consumes the lazy final rows while updating the base inode table. Base-record reads from `FinalRows` therefore occur during this span as well as inode page work. |
| Cleanup/root | `C/filesystem/update.rs:369`, `374` | Releases ordering backing, encodes/emits filesystem root. |
| C2 accept loop | `H/ops/history.rs:1901`; `S/cas/store.rs:383` | Clones each emitted object and attaches predecessors, then `accept` can flush a bounded batch and do real membership, encode, placement and storage work. `storage.finish` alone is not save cost. |
| Finish | `S/cas/store.rs:424` | Drains remaining batch and finishes mutation owner. |
| O4 read-back | `H/ops/history.rs:1111`, `1126` | Separate Verify invocation. It is not in the performance state children. “Read-back” inside filesystem update means persisted-base demand, not O4 verification. |

## The actual base walks and their bounds

1. **Validation's grouped initial demand is change-derived.** `C/filesystem/validate.rs:142` collects supplied directory parents and bound children. `ValidationState::prefetch` sorts/deduplicates them (`462`) and `inode::lookup_many` groups requests by child per level (`C/filesystem/inode/read.rs:85`, `107`, `129`). A shared ancestor is read once within a grouped call; a later call starts again at the inode root. This is neither an inode-table scan nor an operation-wide page cache.

2. **Parent-alias validation can walk the whole base namespace, but only when needed.** Only existing non-regular-file bindings populate `by_parent` (`C/filesystem/validate.rs:203`, `220`). An empty candidate set returns immediately (`286`). Otherwise `check_parent_aliases` starts at the root (`308`), lists each encountered directory in pages of at most 64 entries and 8,192 bytes (`319`), looks up children to identify directories (`362`), and refuses after more than 4,096 examined entries (`332`; limit alias at `55`). A successful large history state therefore cannot simply be assumed to have executed an unrestricted whole-namespace alias scan. It may have no triggering candidates. The actual counts must be retained.

3. **Cycle validation is per bound directory, not one global scan per state.** `check_effective_cycles` iterates the supplied bindings (`C/filesystem/validate.rs:609`), excludes non-directories (`625`), and walks each effective directory subtree (`634`). Its `seen` and `visited` sets reset for each starting binding (`635`), so overlapping subtrees can be revisited. Existing entries are paged at `766`; the 4,096 ceiling applies per walk (`791`), not to the whole update. New-directory entries use the supplied changes (`756`). The maximum of the per-walk bounds is not an operation-wide cap. Logical inode values memoized in ValidationState reduce repeat storage reads (`498`); absent values are not memoized (`508`).

4. **The sorted merge has read amplification before its untouched-subtree decision.** `C/filesystem/sorted/finish.rs:27` returns immediately when there are no changes. Otherwise it reads the root (`35`) and enters `Engine::edit`. An entered branch loops over all of its child entries (`C/filesystem/sorted/merge.rs:239`), batch-fetches them (`247`), decodes and checks each (`259`, `266`), then recursively calls `edit` (`278`). Only at the callee does `!changes.in_range(bound)` reuse the subtree without descending (`180`). Thus an update touching one leaf can read siblings at every entered branch. With a level-1 inode root, all its leaves are read. At greater height it need not read the entire table. A prefix of children before the next changed key can also be entered because `in_range` tests only the upper bound (`merge.rs:32`); this can visit more than the precise search path. Worst case is all pages; source alone does not quantify this history's case. The batch is capped at 256 children (`sorted/page.rs:37`) and shrinks to available scratch (`238`). Pages are at most 8,192 bytes, inode leaves have at most 100 rows, branches at most 127 children (`filesystem/limits.rs:10`, `28`, `32`); the operation scratch ceiling is 4 MiB (`41`).

5. **Reference reduction is based on touched serials and released subtrees.** The reducer holds at most 4,096 pending rows and spills when a newly needed row meets that bound (`references/reduce.rs:25`, `248`). Default base batches are 32 (`27`); merge buffer is 16 KiB and ordering capacity 64 MiB (`references/runs.rs:37`, `39`). `zero_count_serials` obtains the touched set once and calls grouped lookup in chunks (`filesystem/update.rs:464`, `477`). `release_zero_count` starts only from zero-count serials (`references/release.rs:51`), follows directory descendants, and stops propagation when a child remains bound (`173`). It is proportional to touched/released data, not unconditionally the entire namespace. FinalRows resolves its effect rows lazily (`references/reduce.rs:389`, `455`) while the inode engine consumes them.

6. **Stored-base acquisition is additional C2 work beneath C1.** `S/cas/provider.rs:109` lazily opens one ReadSession, reusing connection and decode workspace for this provider. Each wave rereads publication ceiling (`S/cas/read.rs:180`), obtains locators (`61`), and reconstructs/authenticates requested objects and dependencies (`87`). Pack bodies are retained per wave (`77`), whereas decoded ordinary groups are retained across waves within the operation (`143`). A fresh provider per history state does not preserve this group cache across states. These are product cache semantics; source does not prove OS-cache coldness.

## Complexity statement

Let N be current input paths, A total enumerated ancestor components, D changed bindings, T touched inode serials, P visited tree pages and R released descendant entries. The explicit harness scan costs at least Θ(N + A) path processing with ordered-set insertion costs. Product base work is workload-sensitive: grouped inode lookup costs the pages in its demand-induced paths; sorted merging can visit Θ(P) pages up to the entire relevant tree; reference reduction is over T plus R with bounded spill/merge buffers; conditional validation walks add their examined entries and repeated per-binding subtrees. Byte decoding, dependency chains and database work multiply the cost per read. No source-derived “320.9 µs per entry” causal model is justified.

## Instrumentation available without product edits

| Observable | Public existing source | What it can establish / limitation |
| --- | --- | --- |
| Coarse product phases | `C/filesystem/update.rs:79`, `104`; `C/filesystem/objects.rs:133` | Harness can call timed APIs with FilesystemPhases::new. Sum explicit spans and preserve residual. |
| Validation counters | `C/filesystem/validate.rs:30` | `inode_demands`, `inode_pages_read`, `directory_pages_read`, `read_waves`, `entries_examined`. Counters do not separate alias vs cycle traversal. `objects_read` also counts memo-served logical demand (`501`), so it is not physical read count. |
| Sorted counters | `C/filesystem/sorted/page.rs:48`; update result `C/filesystem/update.rs:32` | Pages read/created/reused, changed keys, untouched subtrees and peak scratch separately for directories/inodes. An untouched subtree can already have had its root read. |
| Reference/release work | `C/filesystem/references/reduce.rs:31`; `release.rs:22` | Rows touched/spilled, base records/waves, final values/removals, serials scanned, merge work; release entries/pages. |
| Provider reads | `S/cas/provider.rs:94`, `99`, `109` | Connection opens, ordinary group decodes; public read_wave returns StoreReadCounters including packs, dependency edges and canonical bytes. A harness wrapper can count calls/time without changing provider semantics. |
| Filesystem object counters | `C/filesystem/objects.rs:24`, `66` | `objects.work()` counts calls through this boundary, not all direct reads made through `objects.reader()`; do not treat it as total base I/O. |
| Save work | `S/cas/store.rs:383`, `424`; `S/cas/save.rs:22` | Time accept separately, since accept triggers full batch work. SaveOutcome gives counters, not CPU split between codec/index/SQL. |
| Harness copying | `H/workload/providers.rs:92` | cloned_object records handoff time but executes inside child; publishing it separately does not subtract it from the state's telemetry duration. |

Ordering-spill work can occur during directory mutation and lazy inode consumption, not only in the named `references` scope. Provider read time is nested within tree/content/save work: report it as a subdivision, never add it again to top-level operation totals.

## Hypothesis disposition

- H1, “an unconditional full base-tree walk explains the gap”: **not established**. The whole-input harness scan is established; conditional product traversal and sorted sibling acquisition are established; their time contributions require S1 evidence. There is no counter-backed falsification from S2 alone.
- “storage.finish measures all codec/index/save cost”: **contradicted by control flow**, because accept drains full batches through `save::flush_batch` before finish. This is a boundary fact, not a measured effect.
- “the timed children include O4 whole-tree read-back”: **contradicted by phase dispatch**; O4 is a separate invocation.
- Any claim assigning the 21.195067669 s historical excess to one mechanism remains **unmeasured** until the historical timer boundaries and the current per-phase artifact are reconciled. Historical/current source identities may differ; this source pin must not silently stand in for the older run.

S2 supplies no independent timing sample and claims no gate result. S1's retained decomposition is the authority for durations; S4's pinned reconstruction is the authority for the v0.1.6 boundary.
