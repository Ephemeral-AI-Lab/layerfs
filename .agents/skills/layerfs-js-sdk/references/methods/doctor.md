# The six doctors

> **Status:** Current general guide for the LayerFS 2.0 call surface. Part of
> the `layerfs-js-sdk` skill. There is no `doctor` family and no
> `layerfs.doctor()` function any more (no alias for `layerfs doctor` or
> `doctor.fix`): each thing that can be checked has a `doctor` call in its own
> family. Outputs marked "seen" were printed by the built `layerfs` command on
> a throwaway home on 2026-11-18; the rest is from the spec rows and the
> manual's [doctors section](../../../../../docs/versioned/2.0.0/calls.md#the-doctors).

```js
home.doctor({ on_progress })                     // the home, then each machine, PostgreSQL Store and bucket
machine.doctor(machine, { on_progress })         // the machine, then each Store and Sandbox on it
store.doctor(store, { machine, on_progress })    // the Store, then each of its projects
project.doctor(project, { full, on_progress })   // the record against the Store's own list; full reads its data
sandbox.doctor(sandbox, { on_progress })         // record, runtime, daemon; then each of its Workspaces
backup.doctor(bucket, { full })                  // the bucket's record and reach; full writes and reads one object
```

A doctor checks one thing and changes nothing: it takes no lock, writes no
record, removes no stale lease and starts no machine. It checks its own
thing, then each thing on it, shallowly. A Workspace has no doctor of its
own: `sandbox.doctor` gives each one a node.

## The report

The short form is `{"ok":true}` alone when every check was made and nothing
was found; otherwise `findings` and `not_checked`, each present only with
entries.

- A finding names its thing by its own word (`home`, `machine`, `store`,
  `project`, `sandbox`, `workspace`, `bucket`), then `check`, `severity`
  (`error`, `warning` or `notice`) and `message`; `fix` (one shell line) and
  `from` (the machine a check was made from) when there is one.
- A `not_checked` entry names a thing, or one of its checks, that gave no
  verdict: `blocked_by` (the thing at the root of the block and its state) or
  `reason` (`timeout`, `busy`, `lost`), never both.
- The long form (`detail: true`) adds `evidence` on each finding (e.g.
  `{ step: "handshake" | "authentication" | "transport" }`, retained backup
  keys, or reclaimable image bytes), plus `status` (the worst of `error`,
  `warning`, `not_checked`, `notice`, `ok`), `counts`, `observed_at` and
  `tree`: the doctor's own thing with `status`, `coverage` (`none`,
  `shallow` or `full`), its `checks` (`{ check, outcome, from }`),
  `blocked_by`, `reason` and one list per kind of thing on it.
- `on_progress` (`--progress`) receives one event per finished thing:
  `{ phase, done, total, node }`, `phase` `self` then the kind of thing
  (`machines`, `stores`, `projects`, `sandboxes`, `workspaces`, `buckets`).
  `project.doctor` with `full` also sends `{ phase: "data", done, total }`
  while it reads.
- The shell exits 0 once the report is made, whatever it finds; 125 with
  `NotFound` when the thing is not in the home.

```bash
$ layerfs machine doctor native                                                # seen
{"ok":true}
$ layerfs store doctor shop --detail                                           # seen
{"ok":true,"findings":[],"not_checked":[],"status":"ok","counts":{"ok":2,"notice":0,"warning":0,"error":0,"not_checked":0},"observed_at":"2026-11-18T22:05:32Z","tree":{"store":"shop","status":"ok","coverage":"shallow","checks":[{"check":"store.record","outcome":"pass","from":null},{"check":"store.operation","outcome":"pass","from":null},{"check":"store.connection","outcome":"pass","from":null},{"check":"store.identity","outcome":"pass","from":null},{"check":"store.schema","outcome":"pass","from":null}],"blocked_by":null,"reason":null,"projects":[{"project":"shop/api","status":"ok","coverage":"shallow","checks":[{"check":"project.record","outcome":"pass","from":null},{"check":"project.listed","outcome":"pass","from":null}],"blocked_by":null,"reason":null}]}}
$ layerfs backup doctor nope                                                   # seen, exit 125
{"error":"NotFound","message":"no bucket named nope"}
```

The shape of a finding, from the manual (not run):

```json
{"findings":[{"store":"bad","check":"home.records","severity":"error","message":"…"}],
 "not_checked":[{"sandbox":"sb","blocked_by":{"machine":"docker","reason":"stopped"}}]}
```

```js
const report = await layerfs.home.doctor({ detail: true });
for (const f of report.findings) console.log(f.severity, f.check, f.message, f.fix ?? "");
for (const n of report.not_checked) console.log("not checked:", JSON.stringify(n));
```

## What each one checks

- **`project.doctor`**: `project.record`, `project.listed` (a record its
  Store does not list is a `notice` whose `fix` is
  `layerfs project delete <project>`) and, with `full` only,
  `project.data`: everything `main`'s head and each Branch's head reach is
  read and checked where the Store's calls run. It writes nothing to the
  Store.
- **`store.doctor`**: `store.record` (for a PostgreSQL Store whose record
  lacks a saved login, names a missing login, or holds outdated pre-2.0
  fields, `store.record` fails with the `fix` from `store list`/`store get`
  while remaining checks still run), `store.operation` (an operation left
  with its outcome unknown is an error whose `fix` clears it),
  `store.connection` (for PostgreSQL, probes from this computer and from each
  machine with an own address, recorded reach, or Sandbox of the Store — plus
  `machine` when given — with a 5 s connect-and-login handshake limit so a
  silent server fails `store.connection` with `evidence: { step: "handshake" }`
  and records reach `failed` without a second dial), `store.identity`,
  `store.schema` and, for a sqlite Store, `store.backups` (a notice per
  bucket whose backup is behind, `fix` `layerfs backup create <store> <bucket>`).
  No Store data is scanned.
- **`sandbox.doctor`**: the Sandbox's record and references, its operation,
  runtime, network, route and daemon (`sandbox.record` … `sandbox.observation`), then one
  node per Workspace (`workspace.mounted`, `workspace.activity`); a failed or
  uncertain Workspace is an error fixed by
  `layerfs workspace unmount <workspace> --discard --force`.
- **`machine.doctor`**: `machine.record`, `machine.operation`,
  `machine.configuration`, `machine.provider`, `machine.state`, the worker
  checks (`machine.worker.endpoint`, `.auth`, `.compatibility`, `.ping`),
  `machine.capacity` and `machine.network` (its Sandboxes' network), each
  where it applies (`native` answered only
  `record`, `operation` and `state` in the run); then each Store and Sandbox
  on it. A stopped machine gets no worker checks.
- **`home.doctor`**: `home.build`, `home.prerequisites`, `home.layout`,
  `home.records`, `home.bundle`, `home.host_daemon`, `home.artifacts`,
  `home.capacity`; then
  each machine, PostgreSQL Store and bucket.
- **`backup.doctor`**: `bucket.configuration`, `bucket.credentials`,
  `bucket.transport`, `bucket.list`; `full` adds a write test
  (`bucket.write_capability`, `bucket.put`, `bucket.get`, `bucket.delete`,
  and `bucket.reconcile` after a put whose reply was lost).

Leftovers of interrupted operations are in no doctor's report: LayerFS
removes them itself ([home: leftovers](home.md#leftovers)).
