# branch.commits

Lists saved side-branch Commits or main publications newest first, at an immutable
start. Main rows are Layers, not renamed source Commits. Saved branch commits
are checkpoints; no separate checkpoint API is needed.

```js
const first = await layerfs.branch.commits("shop/api", "main", {page: 1, page_size: 50});
const second = await layerfs.branch.commits("shop/api", "main", {
  page: 2, page_size: 50, at_commit: first.at_commit.id
});
```

```python
layerfs.branch.commits("shop/api", "main", page=1, page_size=50)
```

```sh
layerfs branch commits shop/api main --page 1 --page-size 50
```

Options are `page` (one-based, default 1), `page_size` (1..1000, default 50),
`at_commit` (full immutable starting ID, absent resolves the branch head once).
The response contains `branch`, typed `at_commit`, `items`, `page`, `page_size`,
boolean `has_more`, and `scan:{complete,examined,reason}`. Each item has a full
`ref:{id,kind,branch,root}` plus compact recorded metadata: `id`, `parent`,
`second_parent`, `base`, `source_commit`, `source_branch`, `time`, `source_time`,
`time_status`, `via`, `logical` where applicable. Messages are bounded to 128
UTF-8 bytes with `message_truncated`; `details_omitted:true` directs clients to
[branch.get_commit](get_commit.md) for full relationships and reports.

Side branches follow the persisted first-parent Commit chain. Merge second
parents are linked metadata, not additional ordered rows. An empty side branch
returns no Commit rows and pins its base Layer. Main follows publication Layer
parents, including Init. Source Commits are linked without inserting them into
publication order.

Layer records currently persist no publication timestamp. Their `time` is null
and `time_status` is `unavailable`; `source_time` is the source Commit's genuine
time when present. Never present source time as publication time. This API
introduces no persistence-format migration.

Pages count matching records and pin their start; new writes do not move a pinned
page. Queries have a 10,000-visit per-request history/seek bound and a bounded
reply. A page that cannot fit is refused with `Capacity`; reduce page size or use
a more recent anchor. No incomplete successful page, cursor or total count is
returned. See [shared saved-query contract](../saved-queries.md).

Calls run read-only through the ordinary Store route and refuse folder scopes.
IDs must be complete and admitted by the branch/project context. Existing
`commit.list/get/diff` retain their behavior and legacy paging options.

Implementation: `api/crates/layerfs-api/src/branch/commits.rs` → control
`calls::branch::commits` → Store method `branch.commits`, using `history/saved.rs`.

Sequential page boundaries are retained in a disposable, weak-Store-keyed cache
of at most 128 entries. The key includes the immutable start, branch, page size
and requested page. Cache eviction or process restart changes replay cost only;
a random/deep request which cannot seek within its visit budget refuses Capacity.
A large page number is not itself an error when exhaustion can be proved cheaply.
