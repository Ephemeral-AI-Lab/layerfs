# store.storage_report

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`layerfs store storage-report <name> [--max-objects <n>] [--max-canonical-bytes <size>] [--include-encoding] [--from <file>]`
(SDK `store.storage_report(name, snapshots, {max_objects?, max_canonical_bytes?, include_encoding?})`)
measures how much canonical content an explicit set of saved snapshots shares.
The CLI reads `snapshots` as JSON from stdin or `--from`.

```js
const report = await layerfs.store.storage_report("shop", [
  {project: "shop/api", branch: "main", commit: apiHead},
  {project: "shop/web", branch: "main", commit: webHead},
], {include_encoding: true});
```

```sh
echo '[{"project":"shop/api","branch":"main","commit":"<66-hex id>"}]' | layerfs store storage-report shop
```

## Input

`snapshots` is a list of 1..=64 `{project, branch, commit}` records, no other
field. `project` is `<store>/<project>` in this Store. `commit` is a full saved
Commit or Layer id that the `branch` context admits, exactly as
[branch.get_commit](../branch/get_commit.md) admits it: a main Layer is admitted
by any branch whose history holds it. The same project and saved id may not
appear twice. Nothing resolves a head or lists history: the caller chooses the
population. Refusals are `InvalidInput` (shape, foreign Store, abbreviated id,
another project's record, duplicates, budgets), `NoSuchBranch`, `NoSuchCommit`
and `NotFound` for the Store.

| Option | Range | Default | Meaning |
| --- | --- | --- | --- |
| `max_objects` | 1..=10,000 | 10,000 | object visits over the whole report |
| `max_canonical_bytes` | 1..=64 MiB | 64 MiB | canonical bytes read; not physical I/O |
| `include_encoding` | boolean | false | also observe the selected payload records |

## Result

`{definition_version: 1, population_id, population: "explicit_saved_snapshots",
snapshots: [{project, branch, at:{id,kind,branch,root}}], complete,
examined_objects, canonical_bytes_read, referenced_canonical_bytes,
unique_canonical_bytes, projects: [{project, referenced_canonical_bytes,
exclusive_within_population_canonical_bytes,
shared_within_population_canonical_bytes}], physical_bytes: null,
reclaimable_bytes: null, physical_reason, reason?, payload_encoding?, scope:
"store", name}`.

- **Referenced** sums, over snapshots, the canonical bytes of the distinct
  objects each snapshot's root reaches. **Unique** counts each object once
  across the population. Their ratio is the population's canonical reuse.
- Per project, referenced sums its snapshots; its distinct objects split into
  **exclusive** (no other listed project reaches them) and **shared**.
- `population_id` is an order-independent identity of the listed project and
  saved-id pairs. Projects are listed in name order, snapshots in input order.
- Physical and reclaimable bytes are null: canonical reachability does not
  attribute packs, delta bases, database overhead or free pages.

With `include_encoding`, `payload_encoding` is `{definition_version: 1,
population: "selected_payload_object_records", available, referenced_canonical_bytes,
unique_canonical_bytes, encoded_record_bytes, external_direct_base_count,
dependency_closure_included: false, shared_framing_included: false, reason?}`
over the selected WholeFile and Chunk objects only. `encoded_record_bytes` sums
each distinct payload object's own stored record; group directories, pack
headers and delta bases are not included. `external_direct_base_count` counts
records whose direct base is outside the selected payload objects. A record in
a compressed group has no width of its own, and the observation is then
unavailable with a reason.

## Budgets and completeness

Each snapshot is walked breadth first from its filesystem root. Each visit
locates the object, charges its canonical length, reads and authenticates it
and decodes its references. Objects are deduplicated within a snapshot; a
later snapshot visits a shared object again. Before more objects are queued,
the queue must fit in the visits left. When it does not (`object frontier
exceeds remaining visit budget`), or the next object would pass the byte budget
(`canonical byte read budget exhausted`), the walk stops. The report is then
`complete: false` with that `reason`. `examined_objects` and
`canonical_bytes_read` describe the work done, and every total (per project and
encoding included) is null. A failed object read is a refusal, never a partial
success.

The call reads only immutable saved objects through the ordinary Store route,
in process or through the Store's worker, with one attempt; folder scopes refuse
it. Implementation: `api/crates/layerfs-api/src/store/storage_report.rs` →
control `storage_report::storage_report` → Store method `store.storage_report`
(`engine/crates/layerfs-store-service/src/methods/storage_report.rs`,
`src/store/storage_report.rs`).
