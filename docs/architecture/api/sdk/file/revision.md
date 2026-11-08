# file.revision

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [file](README.md) › revision

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Saved changes to one literal path, including deletions and metadata/identity-only
changes. Reads persisted trees; it does not inspect a live Workspace or infer
rename/copy ancestry.

```js
await layerfs.file.revision(project, branch, path, {
  at_commit, traversal: 'provenance', page: 1, page_size: 50
});
```

```python
layerfs.file.revision(project, branch, path,
                      at_commit=None, traversal="provenance", page=1, page_size=50)
```

```bash
layerfs file revision <store>/<project> <branch> <path> \
  --traversal provenance --page 1 --page-size 50
```

`path` uses the usual absolute-path validation and can be absent at the starting
version. A never-existing path returns an exhausted empty list. `at_commit` pins a
full Commit/Layer identity within the branch/project context. Reuse the first
answer's `at.id`, traversal, path and page size on later pages. A project-wide
history query is refused under a folder scope because record messages and branch
relationships are project metadata.

| Option | Meaning |
| --- | --- |
| `traversal: "provenance"` | Default. Commit first parent or base Layer; Layer source Commit or parent Layer, matching blame provenance |
| `traversal: "branch"` | Side-branch saved first parents only, or main publication Layers in publication order |
| `page`, `page_size` | One-based matching-result pages; default 1 and 50; size 1..1000 |

The first side-branch Commit is compared with its base Layer, even though that
Layer is not listed as a branch-owned revision. Main's publication order and a
publication's source Commit are distinct. Provenance can traverse inherited
commits; it does not expand second-parent merge DAGs.

```text
{ kind: "ok", at: { id, kind: "Commit" | "Layer", branch, root }, path, traversal,
  items: [{ candidate_index, ref, compared_to, kind: "Added" | "Removed" | "Modified",
            before: facts | null, after: facts | null,
            changes: { content, metadata, identity, attribution: null },
            message, recorded_at, time_status }],
  page, page_size, has_more: boolean, end_of_query,
  scan: { complete: true, candidates_examined, reasons: [] } }
```

Facts contain `type`, `inode`, `size`, `mode`; IDs/roots are complete. Byte equality
is checked independently of content-root equality. Pairwise flags are false for
absent sides, where Added/Removed and null facts explain the change. Attribution
discovery is not performed implicitly for every historical revision; use
`file.diff(include_attribution=true)` at selected endpoints.

Layer records have no stored publication timestamp: `recorded_at` is null and
`time_status` is `unavailable`. Never replace it with the source Commit time.
Commit times use the recorded Commit time. Known origin branch names come from
history; missing names remain null.

Successful pages contain matching changes, not sparse candidate windows.
Per-request traversal admission is 10,000 visited records, 64 MiB potential logical
content-comparison payload, and a 48 KiB response. Different-root, equal-length
regular-file pairs reserve their combined length before exact byte comparison;
equal roots and unequal lengths need no payload replay. The payload admission
is cumulative across the revision query and is not a file-size limit.
Budget exhaustion refuses with `Capacity`; it cannot return false exhaustion or
advance beyond an unreturned revision. A bounded cache of immutable page
boundaries avoids repeated prefix traversal for sequential pages. It retains four
queries, 128 boundaries each, with a Weak Store lifetime guard; cache eviction can
change query cost or cause Capacity, never change pinned results. There is no
public cursor, promised total page count or history-size limit.

Implementation: [`file/revision.rs`](../../../../../api/crates/layerfs-api/src/file/revision.rs),
[`read/revision.rs`](../../../../../engine/crates/layerfs-store-service/src/read/revision.rs),
[`revision_pages.rs`](../../../../../engine/crates/layerfs-store-service/src/read/revision_pages.rs).
See [saved query contract](../saved-queries.md) and [file.diff](diff.md).
