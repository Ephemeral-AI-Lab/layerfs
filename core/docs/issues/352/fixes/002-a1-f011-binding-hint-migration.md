# F-011 (range 002 a1) · Stores written before `binding_hint` open and gain the table

| | |
| --- | --- |
| Trigger | REBUILD.md §5, BUGS.md F-011: 002.a1's `binding_hint` ([record](002-a1-diff-commits-change-walk.md), `49499abac`) was handled as kind 3. Every daemon since `511dceed` refuses an older Store (`Store open: … Malformed`), and ranges 0–1 were rebuilt. `diff_commits` already falls back to the full walk for roots without hints, so it should have been kind 2. No measured attempt and no `finding-NNN.aK.json` |
| Kind | **2, additive migration** (REBUILD.md) |
| Old Stores | open and migrate. A writable open adds the `binding_hint` table and records the current schema source and definition in `history_meta`. A read-only open of an unmigrated Store is still refused, because it cannot add the table; the daemon opens its writer first, so its readers see a migrated Store |
| Old data | read through a fallback. Trees saved before the migration have no hints, so a difference involving their changed inodes takes `ChangeWalk::full`, with the same answer and the pre-002.a1 cost. Commits saved after the migration record their hints |
| Compatibility test | p1-pilot's range-1 snapshot, the old format written by `0ef24121`, is read by the new daemon on fork `diag-f011`. REBUILD.md §4 for the whole batch runs on a range-4 fork written by `511dceed`. Both are below |
| Playbook step | D0 (refusal at open) |
| Binaries | daemon `cbfa5e7f…` (F-005) → `71001f655acb9821e76783a9a7e17117ff84fdb776dea1cd9a2c5fdaf7990430`, kept in `core/target/issue352/daemon-71001f65/`. This is the batch's final daemon. SDK addon unchanged: `3da0f665…` |
| Commits | the commit that adds this file. Regression test: `core/crates/layerfs-persistence/tests/old_format_open.rs`. Compatibility tool: `dsh/tools/history/compat.mjs` |
| Recovery | nothing is rebuilt. The rebuilt `p1-pilot-b` line stays the fork point. The abandoned `p1-pilot` line's snapshots (ranges 0–2, `0ef24121`) can be forked again |

## Old design

`layerfs-persistence/src/backend/sqlite/schema.rs` `check` and
`Session::validate` (at `48ad603b1`) require two things:

- The exact table list, which includes `binding_hint`.
- A `history_meta` whose recorded `schema_source` and `schema_definition`
  equal the current source text and the live `sqlite_schema` dump.

`49499abac` added the table to `history.sql` without changing `user_version`.
A Store written earlier therefore failed every one of these checks and was
refused as `Integrity`. The daemon reports this as `Malformed`.

## New design

`Session::add_binding_hints` (`schema.rs`) runs in `Handles::open` before
validation, for writable opens only. It is one write job of one attempt.

1. If the table exists, it does nothing.
2. Otherwise it migrates only when both of these hold:
   - The recorded `schema_source` is exactly the current source with the
     `binding_hint` statement removed: the text every earlier binary recorded.
   - The recorded `schema_definition` equals the live schema dump.
3. It creates the table from `history.sql`'s own statement, then records the
   current source and the new definition.
4. Any other Store is left unchanged, and `validate` refuses it as before.

A migrated Store is byte-for-byte the schema a fresh Store has: the definition
dump orders by type and name, so creation order does not matter. No row is
rewritten. The daemon opens its writer before its readers, so readers validate
the migrated schema.

## Proof

`old_format_open.rs` (host, 0.08 s) turns a fresh Store into the earlier
format: it drops the table and records the earlier source and definition.

1. A read-only open is refused. A writable open succeeds, records and reads a
   hint, and leaves the fresh Store's `schema_source`. The Store then reopens
   read-only and writable.
2. An old-format Store with any other schema difference (a dropped index) is
   still refused.

At `48ad603b1`, case 1 fails with `Malformed`. All `layerfs-persistence`
targets pass, and Clippy `-D warnings` is clean.

## Compatibility

REBUILD.md §4 needs a Store written by an old binary to open and read
correctly. Each run below is one sample on a fresh diag fork, cleaned up by
literal name. Each plays the range's proof checkpoint with the whole tree,
compared with git: changed paths, then every file's blob id and mode.

| Store | Written by | Daemon | Outcome |
| --- | --- | --- | --- |
| `p1-pilot` range 1, old format | `0ef24121` | `cbfa5e7f` (before this fix) | refused: Sandbox create stopped at `Store open: physical Store I/O: pack persistence: Malformed` |
| same | same | `71001f65` (this fix) | ok in 3.9 s: 210 changed paths (git 207, 3 metadata-only), 177 blob ids, whole tree 307 files, nothing missing, extra or different |
| same, after migration | — | `71001f65` | two `fs.write` Commits both `Committed`; the diff between them lists exactly `/f011-a.txt` |
| `p1-pilot-b` range 4 (§4 for the batch: F-008, F-009, F-010, F-001, F-005, F-011) | `511dceed` | `71001f65` | ok in 6.4 s: 285 changed paths (git 262), 283 blob ids, whole tree 530 files, nothing missing, extra or different |

To reproduce, run `node dsh/tools/history/run.mjs fork --run diag-… --from-run
<run> --range k --label dev`, then `node dsh/tools/history/compat.mjs
~/lfs-history/runs/diag-… <daemon> k <head k−1> <head k>`.

## Performance, old vs new

Opening a Store whose table exists costs one extra `sqlite_schema` lookup in
one write job. Opening an unmigrated old Store runs the migration once. No
other path changes, and storage is unchanged.

## Cost of the fix

- A writable open now takes one short write job, so it needs the Store's
  writer turn. The admission window is the existing 2 s.
- On a migrated Store, differences over pre-migration roots take the full walk
  until those roots age out of the diffed range.

## Left open

- A read-only open of a Store that no writable open has migrated is still
  refused. No current path opens read-only first.
