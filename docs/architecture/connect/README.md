# Connect

[Architecture](../README.md) › Connect

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

**The question this layer answers:** Where can a Sandbox run, how does it run there, how is it reached, and how is a Store backed up?

Connect holds everything that reaches a machine or the network on behalf of the engine. Nothing in the engine or core depends on it.

## How the components relate

```text
 Machines ---> Support matrix   (every refusal comes from this table)
    |
    +--------> Network          (workers, reach, forwards)
    |
    +--------> Sandboxes - - - runs - - -> engine: Daemon

 Backups ----> engine: Store, Store calls
```

Dotted edges are runtime relations (a process started), not crate dependencies.

## Components

| Component | What it covers | Topics |
| --- | --- | --- |
| [Support matrix](support-matrix/README.md) | One table decides every platform × host × Store-kind combination, and `SUPPORT.json` is generated from it. | [Table](support-matrix/table.md), [Generated JSON](support-matrix/generated-json.md) |
| [Machines](machines/README.md) | Where Sandboxes run: one adapter per platform that probes, creates, starts, stops and removes a machine. | [Platforms and profiles](machines/platforms-and-profiles.md), [Adapter](machines/adapter.md), [Native](machines/platforms/native.md), [Docker](machines/platforms/docker.md), [Apple container](machines/platforms/apple-container.md), [microsandbox](machines/platforms/microsandbox.md), [Lima](machines/platforms/lima.md), [Firecracker](machines/platforms/firecracker.md), [SSH](machines/platforms/ssh.md), [Adding a platform](machines/adding-a-platform.md) |
| [Network](network/README.md) | How callers, workers and daemons reach each other: worker setup, PostgreSQL reach and port forwards. | [Workers](network/workers.md), [Reach](network/reach.md), [Forwards](network/forwards.md), [Credentials](network/credentials.md) |
| [Sandboxes](sandboxes/README.md) | How a Sandbox runs on a machine's kernel: the docker, native and namespaces backends, commands and streams. | [Backends](sandboxes/backends.md), [Docker](sandboxes/docker.md), [Native and namespaces](sandboxes/native-and-namespaces.md), [Commands and streams](sandboxes/commands-and-streams.md), [Adding a backend](sandboxes/adding-a-backend.md) |
| [Backups](backups/README.md) | Whole-Store backup to a bucket and restore, over bucket drivers that know nothing about Stores. | [Buckets](backups/buckets.md), [Pieces and manifest](backups/pieces-and-manifest.md), [Backup and restore](backups/backup-and-restore.md) |

## Rules for this layer

The [connect guide](../../../connect/AGENTS.md) holds the binding
rules for code in this layer. These pages explain the design; they do not
replace those rules.

---

← [Daemon: Operator](../engine/daemon/operator.md) · [Up: Architecture](../README.md) · [Connect: Support matrix](support-matrix/README.md) →
