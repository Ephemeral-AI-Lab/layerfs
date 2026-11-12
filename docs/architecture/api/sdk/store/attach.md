# store.attach

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [store](README.md) › attach

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Lets a machine run Sandboxes of a PostgreSQL Store, with the Store's
connection or one of its own, after checking from that machine that it
reaches the Store's database and that its role may write there. On a
machine already attached it checks the new connection and replaces the
old one; with no options it checks the current one again. Supersedes
#536 C4 (PostgreSQL Stores usable from every machine) and takes up H9 (a
URL per machine).

| | |
| --- | --- |
| Family | [store](README.md) |
| Library entry | [`store/attach.rs`](../../../../../api/crates/layerfs-api/src/store/attach.rs) |
| Implemented in | `layerfs_control::store::attach` ([`store.rs`](../../../../../control/crates/layerfs-control/src/store.rs)) over `layerfs_store::postgresql::attach` ([`postgresql/attach.rs`](../../../../../control/crates/layerfs-store/src/postgresql/attach.rs)) |
| Runs in | the caller's process; the check runs where the machine's Sandbox calls run (this process, or the machine's worker as `store.reach`) |

## Signature

```js
// JavaScript
await layerfs.store.attach(name, machine, { url, password });
```

```python
# Python
layerfs.store.attach(name, machine, url=None, password=None)
```

```bash
# CLI
layerfs store attach <name> <machine> [--url <url>] [--password <ref>]
```

## Arguments and options

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a PostgreSQL Store of this home; a sqlite Store is `InvalidInput` naming `store push` | [`postgresql/attach.rs`](../../../../../control/crates/layerfs-store/src/postgresql/attach.rs) `postgresql_record` |
| `machine` | Name | a machine whose platform runs PostgreSQL Sandboxes on this host (the support table, word for word, as `sandbox.create`) | [`store.rs`](../../../../../control/crates/layerfs-control/src/store.rs) `runs_postgresql_sandboxes` |
| `url` | Text, optional | the URL the machine dials instead of the Store's; the same rules as `store.connect`'s (no password in it) | [`postgresql/connect.rs`](../../../../../control/crates/layerfs-store/src/postgresql/connect.rs) `refuse_password`, `target` |
| `password` | Text, optional | the password's reference (`env:NAME` or `file:/path`) instead of the Store's; it must resolve now | [`secrets.rs`](../../../../../control/crates/layerfs-home/src/secrets.rs) `resolve` |

Either option may be given alone: the other is the Store's.

## Returns

The Store as [store.get](get.md) shows it. Its `machines` holds each
attached machine, `{ url?, password?, checked? }`; `checked` is the last
check that passed, `{ at, server_version, latency_ms }`.

## Where it runs

```text
 caller process                                 the machine's side
 --------------                                 ------------------
 refusals: Store kind, machine, platform,
           url, password reference
 the URL the machine dials: its url or the
   Store's, the password resolved, a loopback
   host rewritten for the platform (reach)
 Executor::Local ----------------------------->  in this process
 Executor::Worker -- store.reach ------------->  in the machine's worker
                                                 one connection: version,
                                                 role, schema, privileges;
                                                 then the schema's binding
 home lock, Store lock:
   the record's binding unchanged since, else Conflict
   write machines.<machine> = { url?, password?, checked }
```

The check reads only: `SHOW server_version`, the role, whether the
`layerfs` schema exists, `USAGE` on it, `TEMP` on the database (a merge
stages in temporary tables), and `SELECT`, `INSERT`, `UPDATE`, `DELETE` on
each of its tables and `USAGE` on each sequence
([`access.rs`](../../../../../engine/crates/layerfs-persistence/src/backend/postgresql/access.rs)).
A failed check writes nothing. LayerFS never creates or grants roles.

## Errors

| Code | When | Source |
| --- | --- | --- |
| `NotFound` | no Store or no machine of that name | [`record.rs`](../../../../../control/crates/layerfs-store/src/record.rs) |
| `InvalidInput` | a sqlite Store; a URL holding a password | [`postgresql/attach.rs`](../../../../../control/crates/layerfs-store/src/postgresql/attach.rs), [`postgresql/connect.rs`](../../../../../control/crates/layerfs-store/src/postgresql/connect.rs) |
| `Unsupported` | the platform runs no PostgreSQL Sandboxes here; an ssh loopback URL (reach); a build without the `postgresql` engine; a worker from a bundle without `store.reach` (names `machine stop` and `machine start`) | [`store.rs`](../../../../../control/crates/layerfs-control/src/store.rs) |
| `Unavailable` | the password reference does not resolve; the machine is stopped (names `machine start`); the server not reached from the machine, or its database holds another Store or none; names the machine and the host, never the URL | same |
| `Denied` | the server refused the login, or the role lacks a privilege (names the first and how many) | [`postgresql/attach.rs`](../../../../../control/crates/layerfs-store/src/postgresql/attach.rs) `check` |
| `Conflict` | the Store was disconnected, or connected again to another database, while the machine was checked | `record` |

## Related

- [store.connect](connect.md) attaches `native`; [store.detach](detach.md);
  [sandbox.create](../sandbox/create.md) refuses a machine the Store is not
  attached to; [machine.list](../machine/list.md) names each machine's
  attached PostgreSQL Stores (`postgresql`).

---

← [store.connect](connect.md) · [Up: store](README.md) · [store.detach](detach.md) →
