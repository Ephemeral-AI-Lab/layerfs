# Credentials

[Architecture](../../README.md) › [Control](../README.md) › [Network](README.md) › Credentials

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Where a PostgreSQL password is resolved and how it travels.

| | |
| --- | --- |
| Component | [Network](README.md) |
| Source | [`postgres_url.rs`](../../../../control/crates/layerfs-sandbox-backend/src/postgres_url.rs), [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs), `reached` and `in_worker` in [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) |

The rule (control guide): a PostgreSQL password is resolved only in the
caller, travels only in one worker request and the Sandbox's private daemon
setup file, and is never written into a record, log, trace or `Debug`
output. Bucket keys follow a similar reference rule; see
[Backups: Buckets](../backups/buckets.md#credentials).

## Where a password is resolved

```text
 Store record (home)                       caller process, each call
+-----------------------------+          +-------------------------------+
| url      postgres://u@h/db  |--------->| secrets::url(record)          |
| password env:NAME           |          |   resolve(reference)          |
|      or  file:/abs/path     |          |     env:NAME -> $NAME         |
|  (a reference, no secret)   |          |     file:/p  -> text, trailing|
+-----------------------------+          |                 \n \r trimmed |
                                         |   u:<percent-encoded pw>@h/db |
                                         +---------------|---------------+
                                                         v
                                         layerfs_platform::reach(spec, url)
                                                         v
                                         `postgresql` argument of ONE call
```

| Step | What | Error | Source |
| --- | --- | --- | --- |
| Reference form | `env:NAME` or `file:/absolute/path`; anything else | `InvalidInput` | `resolve` in [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) |
| Unset or empty variable, unreadable file | — | `Unavailable` | same |
| URL without a user but with a password reference | — | `InvalidInput` | `with_password` in [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) |
| Encoding | Every byte outside `A-Z a-z 0-9 - . _ ~` is `%XX`. | — | `percent` in [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) |
| Rewrite for the machine | Loopback host to the machine's alias, or refused. | see [Reach](reach.md) | `reached` in [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) |

The reference is resolved again on each call; the full URL is never
written down.

## How it travels

```text
 caller --(1 worker request: sandbox.host {..., postgresql})--> worker
   |                                                              |
   | native: same process                                        | in_worker:
   v                                                              | store_record
 DaemonStore::Postgres { url, schema, ... }  <--------------------+ minus url,
   |                                                                password
   v
 daemon setup bytes (LaunchRequest.daemon_config, at most 8192 bytes)
   |
   +-- docker ------> tar entry layerfs-local/config/daemon.setup, 0600,
   |                  dirs 0700, PUT /containers/<id>/archive
   +-- native ------> <sandbox>/config/daemon.setup, 0600, in a 0700 dir,
   +-- namespaces --> created O_NOFOLLOW | O_CLOEXEC, create_new
   v
 layerfs-daemon --config <that file>   dials PostgreSQL
```

| Hop | Carrier | Source |
| --- | --- | --- |
| 1 | The `postgresql` field of one `sandbox.host` request to the machine's worker; not on `native`, where the call runs in the caller. | `reached`, `on_machine` path in [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) |
| 2 | The worker drops `url` and `password` from the Store record it was sent and keeps only a thin record. | `in_worker` in [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) |
| 3 | `DaemonStore::Postgres { url, … }` in the daemon setup. | `sandbox_store` in [`store/postgres.rs`](../../../../control/crates/layerfs-store/src/postgresql/open.rs), [`store.rs`](../../../../engine/crates/layerfs-bridge/src/store.rs) |
| 4 | The setup file: 0600 in a 0700 directory, written once. | `prepare` in [`local/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/create.rs) and [`backend/namespaces/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/create.rs); `upload` in [`backend/docker/upload.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/docker/upload.rs) |

The worker's reach check, before the launch, reads the schema's binding and
finds the project on one session to the server with the rewritten URL (the
process's held host session, #65, opened if none is held); a failure names
the machine and the host it tried, never the URL (`reach_check` in
[`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs),
[`postgresql/reader.rs`](../../../../control/crates/layerfs-store/src/postgresql/reader.rs)).

## Where it never appears

| Place | Why it is absent | Source |
| --- | --- | --- |
| Store record | Holds the URL without a password and a `password` reference. | [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) |
| Worker's own records | `in_worker` strips `url` and `password`. | [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) |
| `Debug` of the daemon setup | `DaemonStore` prints `url` as `[redacted]`; `LaunchRequest` prints only `config_bytes`. | [`store.rs`](../../../../engine/crates/layerfs-bridge/src/store.rs), [`types/options.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/options.rs) |
| `Debug` of a parsed URL | `PostgresUrl` has no `Debug`. | [`postgres_url.rs`](../../../../control/crates/layerfs-sandbox-backend/src/postgres_url.rs) |
| Reach and forward errors | Messages name the platform, machine and reason only. | [`reach.rs`](../../../../control/crates/layerfs-platform/src/reach.rs) |
| Worker trace | A connection trace records the worker's address, the operation and timing. | `send` in [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |
| Container command line | The daemon runs `--config /layerfs-local/config/daemon.setup`; the setup is not in `Env` or `Cmd`. | `encode` in [`backend/docker/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/docker/create.rs) |
| Namespaces init spec | `HelperSpec` names only paths, the cgroup and binds; the helper never reads the setup file. | [`helper_spec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/helper_spec.rs) |

The setup file lives as long as the Sandbox: it is inside the Sandbox's
directory or container and goes with `delete`.

## Related

- [Reach](reach.md): the rewrite applied to the resolved URL.
- [Workers](workers.md): the one request that carries it.
- [Sandboxes: Native and namespaces](../sandboxes/native-and-namespaces.md) and [Sandboxes: Docker](../sandboxes/docker.md): where the setup file is written.
- [Engine: Store, PostgreSQL](../../engine/store/postgresql.md): what the daemon opens.

---

← [Network: Forwards](forwards.md) · [Up: Network](README.md) · [Control: Sandboxes](../sandboxes/README.md) →
