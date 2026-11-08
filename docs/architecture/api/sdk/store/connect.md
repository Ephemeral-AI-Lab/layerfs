# store.connect

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [store](README.md) › connect

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Attaches a PostgreSQL database as a Store of this home: an empty database
gets the `layerfs` schema, an existing one keeps its data and its projects
become this home's project records.

| | |
| --- | --- |
| Family | [store](README.md) |
| Library entry | [`store/connect.rs`](../../../../../api/crates/layerfs-api/src/store/connect.rs) |
| Implemented in | `layerfs_store::postgresql::connect::connect` ([`postgresql/connect.rs`](../../../../../control/crates/layerfs-store/src/postgresql/connect.rs)) |
| Runs in | the caller's process, which dials the server; needs a build with the `postgresql` feature |

## Signature

```js
// JavaScript
await layerfs.store.connect(name, url, { password, synchronous_commit });
await client.store.connect(name, url, { password });
```

```python
# Python
layerfs.store.connect(name, url, password=None, synchronous_commit=None)
client.store.connect(name, url, password="env:PGPASSWORD")
```

```bash
# CLI
layerfs store connect <name> <url> [--password <ref>] [--synchronous-commit <value>]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a new Store name (`StoreName::parse`) | [`postgresql/connect.rs`](../../../../../control/crates/layerfs-store/src/postgresql/connect.rs) `run` |
| `url` | Text | starts `postgresql://` or `postgres://`; holds no password (neither `user:pass@` nor any query parameter whose name contains `password`); names its user when `password` is given | [`postgresql/connect.rs`](../../../../../control/crates/layerfs-store/src/postgresql/connect.rs) `refuse_password`, [`secrets.rs`](../../../../../control/crates/layerfs-home/src/secrets.rs) `parts` |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `password` | Text | none | a reference, `env:NAME` (any variable name) or `file:/absolute/path` (trailing newline dropped); never the password itself; it must resolve now | [`secrets.rs`](../../../../../control/crates/layerfs-home/src/secrets.rs) `resolve` |
| `synchronous_commit` | Text | `off` | `off`, `local`, `remote_write`, `on` or `remote_apply`; recorded and applied to the Store's write transactions | [`postgresql/connect.rs`](../../../../../control/crates/layerfs-store/src/postgresql/connect.rs) |

The record keeps `url` and the reference only. The reference is resolved
in the caller's process on every later call, and the full URL is never
written to the home. Unlike a bucket key, a PostgreSQL `env:` reference
need not start with `LAYERFS_`.

## Returns

```text
 {
   name,
   kind: "postgresql",
   schema: "layerfs",
   synchronous_commit,
   projects: [name]      projects the schema already held
 }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `name` | text | the new Store | [`postgresql/connect.rs`](../../../../../control/crates/layerfs-store/src/postgresql/connect.rs) |
| `kind` | text | always `postgresql` | same |
| `schema` | text | always `layerfs` (`SCHEMA`), the one schema of a 2.0 PostgreSQL Store | same |
| `synchronous_commit` | text | as given, or `off` | same |
| `projects` | list of text | each project read from the schema, now a project record | [`identity.rs`](../../../../../control/crates/layerfs-store/src/identity.rs) `list_projects` |

[store.get](get.md) shows the full record afterwards (`url`, `created`,
`password` reference).

## Where it runs

```text
 caller process                                     PostgreSQL server
 --------------                                     -----------------
 checks: url has no password, synchronous_commit, password resolves
 unclaimed: name free; no PostgreSQL Store of this home has the same
            host:port/database
 create_schema(fresh binding) --------------------> schema "layerfs"
   made           --> binding = fresh
   "schema exists" --> read_binding --------------> existing binding
 list_projects (opens the Store service) ---------> read projects
 home lock:
   unclaimed again, now also by binding
   write store/<name>/record.json (url, schema, synchronous_commit,
     created, password ref, binding, own cursor key)
   write one project record per project found
```

| Build or platform | What happens | Source |
| --- | --- | --- |
| built with `postgresql` (Linux, macOS) | as above, in this process | [`store/postgres.rs`](../../../../../control/crates/layerfs-store/src/postgresql/open.rs) |
| built without `postgresql` (the Node, Python and CLI default) | `Unsupported` before anything is written | [`project/postgres_init.rs`](../../../../../control/crates/layerfs-store/src/project/postgres_init.rs) `create_schema` |

A PostgreSQL Store has no machine; every later Store call on it runs in the
caller's process. A sqlite Store is never converted into one. See the
[routing table](../../../control/routing/routing-table.md#by-store-kind).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad name; a URL that is not PostgreSQL, holds a password, or lacks a user while `password` is given; a bad `synchronous_commit`; a password that is not a reference; a folder-scoped client | no | [`postgresql/connect.rs`](../../../../../control/crates/layerfs-store/src/postgresql/connect.rs), [`secrets.rs`](../../../../../control/crates/layerfs-home/src/secrets.rs) |
| `Unavailable` | the password variable is not set or its file cannot be read; no connection was made, or the server refused the session (no such database, wrong user or password); a lock held for 120 s | no | [`secrets.rs`](../../../../../control/crates/layerfs-home/src/secrets.rs), [`store/postgres.rs`](../../../../../control/crates/layerfs-store/src/postgresql/open.rs) `no_session` |
| `Exists` | a Store of that name exists, or another PostgreSQL Store of this home connects the same database (by URL, or by binding once read) | no | [`postgresql/connect.rs`](../../../../../control/crates/layerfs-store/src/postgresql/connect.rs) `unclaimed` |
| `Unsupported` | a build without the `postgresql` feature | no | [`store/postgres.rs`](../../../../../control/crates/layerfs-store/src/postgresql/open.rs) |
| `Failed` | making or reading the schema, or opening it, failed after a session | no | [`project/postgres_init.rs`](../../../../../control/crates/layerfs-store/src/project/postgres_init.rs), [`store/postgres.rs`](../../../../../control/crates/layerfs-store/src/postgresql/open.rs) |

The schema is made before any record is written. A refusal after
`create_schema` leaves the new empty schema in the database and no record;
connecting again finds it and keeps its binding.

## Examples

Not run; written from the call table.

```js
await layerfs.store.connect("pg", "postgresql://app@db:5432/app",
  { password: "env:PGPASSWORD", synchronous_commit: "off" });
```

```python
layerfs.store.connect("pg", "postgresql://app@db:5432/app", password="env:PGPASSWORD")
```

```bash
layerfs store connect pg postgresql://app@db:5432/app --password env:PGPASSWORD
```

## Related

- After: [project.init](../project/init.md) into it, [store.status](status.md);
  later [store.disconnect](disconnect.md).
- [Engine: PostgreSQL](../../../engine/store/postgresql.md),
  [Engine: Store backends](../../../engine/store/backends.md),
  [Control: Network](../../../control/network/README.md) (how machines reach the server).

---

← [store.create](create.md) · [Up: store](README.md) · [store.push](push.md) →
