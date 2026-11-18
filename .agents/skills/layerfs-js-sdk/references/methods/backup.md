# backup calls

> **Status:** Current general guide for the LayerFS 2.0 call surface. Part of
> the `layerfs-js-sdk` skill. Only `backup list` on a home with no bucket
> (`{"items":[]}`) and `backup doctor` of a missing bucket were run (built
> `layerfs` command, 2026-11-18); everything else comes from the table, the
> spec rows and the manual. 1.x `key`, `region` and `part_size` are gone,
> with no alias. An agent working on a project never needs this family.

```js
backup.connect(name, url, { access_key, secret_key })
backup.disconnect(bucket)
backup.list({ bucket })
backup.create(store, bucket, { file, on_progress })
backup.restore(bucket, file, store, { on_progress })
backup.doctor(bucket, { full })
```

A backup is a copy of a sqlite Store in an r2 bucket or an S3 endpoint.
Nothing runs from a backup. A Store has one link per bucket, so it can be
backed up to several buckets.

| Call | What it does |
| --- | --- |
| `connect(name, url, { access_key, secret_key })` | Records a bucket after one listing that checks it. Both keys are required (shell: `--access-key K --key-stdin`, the secret read from stdin only); the secret is kept in the home's secret folder and never answered or logged. A bucket that does not answer or refuses the key is `Unavailable`; a taken name is `Exists`. `{"ok":true}` |
| `disconnect(bucket)` | Forgets the bucket and its key; its files stay. Short `{"ok":true}`, with `unlinked` (the Stores whose links went) |
| `list({ bucket })` | One row per Store file in the connected buckets: `{ bucket, file }` with `size`, `store` (the linked Store), `behind` (the Store changed since its last backup), `changed` (the bucket's file is not the one the link last moved) and `missing`. A bucket that cannot be listed is named in `not_listed` |
| `create(store, bucket, { file })` | Uploads a snapshot of the Store (`file` defaults to the Store's name) and links the Store to it. Short `{ file, size }`. Progress `check`, `snapshot`, `send` (from a machine), `hash`, `upload` |
| `restore(bucket, file, store)` | Downloads a file and installs it as the sqlite Store `store` on this computer. Short `{"ok":true}` or `{ projects }`. Progress `check`, `download`, `verify`, `install`. `Conflict` when that Store changed since its last backup or restore |
| `doctor(bucket, { full })` | Checks the bucket's record and reach; `full` also writes and reads one object. See [doctors](doctor.md) |

`create` and `restore` record the Store's `op` (`backup` or `restore`), so
every client reads the Store `busy` meanwhile. A lost reply leaves the Store
`broken` `unresolved` until `store.get(store, { with: ["live"] })` settles
it.
