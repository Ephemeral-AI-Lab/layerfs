# branch.get_commit

Returns full details for one saved Commit or main publication Layer admitted by
its branch/project context. `commit` is the saved version's complete immutable
ID, not an abbreviation. Existing `commit.get` prefix behavior is unchanged.

```js
await layerfs.branch.get_commit("shop/api", "main", fullLayerId);
await layerfs.branch.get_commit("shop/api", "feature", fullCommitId);
```

```python
layerfs.branch.get_commit("shop/api", "main", full_layer_id)
```

```sh
layerfs branch get-commit shop/api main FULL_LAYER_ID
```

The result preserves recorded Commit/Layer metadata and adds typed
`ref`, `parent_ref`, `second_parent_ref`, `base_layer`, `source_commit_ref`,
`source_time`, `time_status`. References are `{id,kind:"Commit"|"Layer",branch,root}`;
unknown originating branch is null. Relationships which do not apply are null.

A publication's `parent`/`parent_ref` identify the previous publication Layer;
`source_commit`/`source_commit_ref` identify what was published. Publication does
not rewrite file-line ownership. Layers persist no publication timestamp:
`time:null`, `time_status:"unavailable"`; `source_time` is separate source metadata.
A Commit keeps its null first Commit parent distinct from its non-null base Layer.
Merge second-parent and displacement facts remain separate.

Main admits its publication Layer lineage. Side branches admit their persisted
first-parent/base lineage and their own recorded Commits, including displaced
ones, so pinned own-branch reads remain possible after a head overwrite. An
unrelated branch/project ID is refused; shared ancestry is not denied merely
because the Commit originated on another branch. Context lookup is bounded to
10,000 visited history records and refuses `Capacity` rather than returning an
ambiguous or incomplete match. Folder scopes are refused.

This is a read-only Store call. No machine is started and no write lane is
acquired. Implementation: API `branch/get_commit.rs` → control
`calls::branch::get_commit` → Store method `branch.get_commit` → `history/saved.rs`.
See [branch.commits](commits.md) and [shared contract](../saved-queries.md).
