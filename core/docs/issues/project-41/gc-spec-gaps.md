# GC spec gaps: implementation decisions (T13b)

> **Status:** implementation note for layerfs-project#41. `LayerFS-GC-spec.md` is **frozen** (2.0.0,
> owner-approved on #42): nothing here changes contract text. Where the spec is silent these are
> implementation decisions — the least-surprising option consistent with it. A "correction" entry is a
> proposed #42 amendment, not a change made here.

**Baseline:** `main` at T13b; companion docs `gc-api-contract.md` (T11), `gc-acceptance-matrix.md` (T12), `gc-runbook.md`.

## G1. Detail shapes missing for five codes (§10)
**Spec.** §10's table lists `StaleStore`, `Capacity`, `Unsupported`, `Conflict`, `ReservedName`, but states a
`detail` only for `MountsActive`, `Busy`, `Unresolved` (§10), `Removed` (§4.3, §10) and `NameTaken` (§4.4);
otherwise §10 requires only snake_case details.
**Decision.** All five carry an **empty detail** (`{}`). No field is invented: the spec names none, so no client may
depend on one, and `code` plus message are the surface. `ReservedName` (today only `branch.remove` of `main`, §4)
does not echo the Branch; `Conflict` (sealed tuple, §7/§10; missing `project_epoch`, §5) does not echo the failed
check; `Capacity`/`Unsupported` (§10) do not echo backend or catalog method; `StaleStore` (§10) does not echo
`(dev, ino)` or the generation.
**Impact.** T11: one empty-detail helper, explicit `HistoryError` arms before `_ => "Failed"`, never a `Failed`
mapping. T3/T4/T5: propagate `{}` unchanged, no `branch`/`project` field added at the site. T12: error-matrix rows
assert code, message and `detail == {}`, never an unnamed field.
**Correction.** None; additive fields later would need a #42 amendment.

## G2. `Unknown` absent from §10's code table (§4, §4.2, §10)
**Spec.** §4: "a lost reply is `Unknown`, keeps the sidecar"; §4.2: `project.list(store)` propagates
`NoSuchStore`/`Busy`/`Unknown`. §10's table omits `Unknown`.
**Decision.** `Unknown` is a **service/control-plane outcome**, not a `HistoryError`/`lookup.rs` code: it exists for
the caller of a lost reply and for a Store listing that cannot be determined. It reuses the `Unresolved` detail
shape `{entity: {kind, name}, phase, op}` — same uncertainty, different observer (owning caller vs non-owning
call) — with `phase` and `op` `null` when no durable op is known. It joins neither `HistoryError` nor §10's table.
**Impact.** T4/T11: the `project.delete`/`store.compact` worker path keeps the sidecar and returns `Unknown`; T12
"Worker/lost reply" asserts it and no cleanup; `project.list(store)` propagates it untouched. T3/T5 have no worker path.
**Correction.** Yes: a §10 row (or one §4 sentence) calling `Unknown` a service code with the `Unresolved` shape.

## G3. §11 camelCase names vs §10 snake_case rendering (§10, §11)
**Spec.** §11 names `deadBytesBound`, `deadBytesRefined`, `logicalDiscarded`, `backendReusable`, `physicalShrink`,
`keptWaste.deadInRetainedPacksBytes`, `keptWaste.deltaBases`, `inodeBlocksBytes`, `peakTempBytes`, `zeroReason`;
§10 requires snake_case results/details and says "§11 names are logical, not rendered".
**Decision.** §11 names are Rust/logical only; every wire, CLI and SDK key is snake_case, mechanically:
`dead_bytes_bound`, `dead_bytes_refined`, `logical_discarded`, `backend_reusable`, `physical_shrink`,
`peak_temp_bytes`, `zero_reason`, `kept_waste {dead_in_retained_packs_bytes, delta_bases {bytes, references},
inode_blocks_bytes}`. `kind`, `path`, `generation`, `before`, `after`, `residue`, `bytes`, `artifacts`, `reason`,
`value`, `cap`, `estimate`, `references` are single words. One serializer does it; no per-call map.
**Impact.** T10 owns the report struct (serde `rename_all = "snake_case"`); T11 renders verbatim; T12 asserts the
snake keys on both paths. T3/T4/T5 results are already snake_case (`removed_ms`, `deleted_ms`, `home_entry_removed`,
`frontier_serial`).
**Correction.** None needed; §11 already defers to §10. A one-line "rendered snake_case" note would remove the ambiguity.

## G4. Merge `Removed` detail vs the canonical shape (§4.3, §10)
**Spec.** §4.3: "A merge that cannot materialize the whiteout is refused `Removed` with `detail {paths: [...]}`";
the same section: "Every refusal carries one detail shape: `{paths: [{path, frontier_serial}]}`"; §10 repeats it.
**Decision.** One shape everywhere: `{paths: [{path, frontier_serial}]}`. The merge refusal lists every path it
cannot materialize as a full element (not a bare string), each with the governing maximum frontier over the path and
its `/`-boundary ancestors (§4.3); elements are sorted by normalized path bytes and deduplicated. read/stat emit one
element: the request path with its governing frontier. A directory element names the directory; descendants are
covered by the same frontier rule.
**Impact.** T5: the whiteout walk produces this list; T11: one detail serializer shared by read/stat/merge; T12:
"`remove_file` and derived nodes" and "Removed path in a merge" assert full elements and deterministic order. T3/T4
never emit `Removed`.
**Correction.** No behavior change; §4.3's `[...]` is shorthand for §10's shape. A clarification amendment could say
"full §10 elements"; not made here.

## G5. Idempotent retry only for `project.delete` (§1, §4, §4.4)
**Spec.** §4 gives `project.delete` a durable sidecar, marker-as-commit-point and an idempotent retry reusing
`deleted_ms`; §4.4 makes a repeat `branch.remove` `NoSuchBranch`; §1 says a lost reply stays uncertain and nothing
retries itself.
**Decision.** Keep the asymmetry. Only `project.delete` is retry-idempotent (sidecar `.../delete/<project>.json`,
states `pending|marked|done`; the retry resolves retired/live/absent). `branch.remove` has no sidecar in 2.0: a lost
reply is `Unknown` (G2) and a repeat after success is `NoSuchBranch`; the API never turns that into `Removed`.
`remove_file` is re-callable but not replayable: a repeat is a new call that writes the current high-water as the
new frontier (§4.3), so it may hide newer content; it is `UpToDate` only when no project node is above this path's
marker frontier. No client, CLI, SDK or server auto-retries any of the three.
**Impact.** T3: repeat `NoSuchBranch`, no `deleted_ms` echo; T4: sole sidecar/retry path, plus `deleted_ms` reuse;
T5: repeat = new frontier/`UpToDate`, no stored reply; T11: no retry option/parameter, `Unknown` documented; T12:
retry case only for `project.delete`, the other repeats as above.
**Correction.** None; the asymmetry is intended (§4 is the single exception).

## G6. `project.delete` `home_entry_removed:false` residue (§4, §4.2)
**Spec.** §4: the marker is the commit point; a home-removal failure returns success with `home_entry_removed:false`
and the stored `deleted_ms`; a different home's stale entry is not rewritten and is hidden by the §4.2 intersection.
**Decision.** Return exactly the §10 shape and add **no** residue field (that would be a contract change):
`{kind: "Removed", project, deleted_ms, home_entry_removed}`. The residue is another home's entry; it is named
nowhere in the result. The sidecar stays `marked` (durable evidence for a same-home retry); `project.list` omits the
entry and `project.get` answers `NoSuchProject`. A same-home retry finishes the entry removal and returns `true` with
the reused `deleted_ms`. Operator visibility is the sidecar and the call log, not the result.
**Impact.** T4: sidecar state machine and same-home retry; T11: no residue field; T12: "project.delete retry" and
"Home listing" cases (hidden, entry present, retry true). T3/T5 unaffected.
**Correction.** None; §4 already states this. A residue field in the result would be a #42 amendment.

## Cross-cutting

- Every refusal keeps one code and one stable message (§12 error matrix); these decisions add no code and no field.
- Details are built snake_case at the site or snaked once in the dispatcher (§10); an empty detail stays `{}` and is
  never dropped or synthesized.
- These decisions bind T3, T4, T5, T11, T12 and the T10 report; the frozen spec remains the authority and wins on
  disagreement.
