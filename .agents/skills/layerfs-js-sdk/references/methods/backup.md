# backup calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. Only `backup list` on a home with no bucket was run
> (`{"items":[]}`). Everything else comes from the table, `layerfs help backup`,
> S §3.9 and the decisions log (O23); no bucket was used in this pass. An agent
> working on a project never needs this family.

```js
backup.connect(name, url, { key, region, part_size })
backup.disconnect(name)
backup.list(bucket?)
backup.create(store, bucket, file?, { on_progress })
backup.restore(store, bucket, file?, { on_progress })
```

A backup is a copy of a sqlite Store in an r2 or MinIO bucket. Nothing runs
from a backup. A PostgreSQL Store is `InvalidInput`. A Store has one link per
bucket, so it can be backed up to several buckets.

| Call | What it does |
| --- | --- |
| `connect(name, url, { key, region, part_size })` | Records a bucket: `url` is endpoint, bucket and prefix. A host `<account>[.<jurisdiction>].r2.cloudflarestorage.com` is R2; any other is MinIO. `key` is a reference: `env:LAYERFS_<NAME>` (reads `…_ACCESS_KEY` and `…_SECRET_KEY`) or `file:/path` (lines `access_key=…`, `secret_key=…`, readable only by its owner) |
| `disconnect(name)` | Forgets the bucket and this home's links to it; its objects stay |
| `list(bucket?)` | Every Store file on the connected buckets, checked live |
| `create(store, bucket, file?)` | Uploads a snapshot of the Store and links the Store to it. `file` defaults to the Store's name |
| `restore(store, bucket, file?)` | Downloads a file and installs it as the sqlite Store `store` on `native`: a new Store, or replacing an idle one on `native`. `Conflict` when that Store changed since its last backup or restore |

`create` and `restore` report `{ phase }` updates (`snapshot`, `upload`,
`download`, …) on `on_progress` / `--progress`. LayerFS never deletes bucket
objects. Secrets are read at each call and never written to the home.
