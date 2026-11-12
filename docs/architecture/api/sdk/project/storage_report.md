# project.storage_report

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`layerfs project storage-report <store>/<project> [--max-objects <n>] [--max-canonical-bytes <size>] [--include-encoding] [--from <file>]`
(SDK `project.storage_report(project, snapshots, {max_objects?, max_canonical_bytes?, include_encoding?})`)
is [store.storage_report](../store/storage_report.md) restricted to one project.
The CLI reads `snapshots` as JSON from stdin or `--from`.

```js
const page = await layerfs.branch.commits("shop/api", "main", {page_size: 20});
const report = await layerfs.project.storage_report(
  "shop/api", page.items.map((item) => ({branch: "main", commit: item.id})));
```

Each snapshot is `{branch, commit}`: the call supplies the project, and a
snapshot that names one is refused. Inputs, budgets, completeness and the
optional `payload_encoding` are the Store call's. The result is the Store
call's with `scope: "project"` and `project` in place of `name`; the same pins
give the same `population_id` and totals through either call.

A typical use compares a Branch's recent saved revisions: referenced over
unique bytes is how much those revisions share. `branch.commits` with
`include_storage` gives each revision's own Save allocation, which is a
different measure.

Implementation: `api/crates/layerfs-api/src/project/storage_report.rs` →
control `calls::project::storage_report`, which runs the Store call.
