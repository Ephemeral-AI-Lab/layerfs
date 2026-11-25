# The doctors

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](README.md). Sources: [the calls](calls.md#the-doctors),
the frozen `doctor` key of `core/docs/issues/call-surface-2-0/calls.json`,
DECISIONS.md V22, and the per-check catalogue in
[doctor findings](../../architecture/control/doctor/findings.md).

A doctor checks something and tells you what it found. It never fixes
anything: it takes no lock, removes no lease and starts no machine, and it
writes no state you own. Two exceptions: `backup doctor --full` writes and
deletes one test object in the bucket, and a PostgreSQL Store's check records
what it observed of the connection. When it knows a command that would repair a problem, it prints it as
`fix`, and you decide whether to run it.

## The six doctors

| Call | Checks |
| --- | --- |
| `layerfs home doctor` | The home folder, then each machine, PostgreSQL Store and bucket, shallowly |
| `layerfs machine doctor <machine>` | The machine and its worker, then each Store and Sandbox on it |
| `layerfs store doctor <store> [--machine M]` | The Store (record, connection, identity, schema, backups), then each of its projects |
| `layerfs project doctor <project> [--full]` | The project's record against its Store's own list; `--full` also reads all its data |
| `layerfs sandbox doctor <sandbox>` | The Sandbox's record, runtime and daemon, then each of its Workspaces |
| `layerfs backup doctor <bucket> [--full]` | The bucket's record, credentials and reach; `--full` also writes, reads and deletes one test object |

A Workspace has no doctor of its own; `sandbox doctor` reports on it.

## Read the answer

On a terminal you get a readable report. Findings come worst first: errors,
then warnings, then notices. In a pipe, a file or a program you get one JSON
line. `--text` forces the readable form and `--json` forces the JSON line;
giving both is a wrong command line (exit 2, nothing runs).

A healthy home looks like this:

```json
{"ok":true}
```

Otherwise the answer holds `findings` and `not_checked`, each only when it has
entries. A finding names the thing, the `check`, a `severity` and a `message`,
and has a `fix` when there is one:

```json
{"findings":[{"store":"shop","check":"store.backups","severity":"notice",
  "message":"its backup in b1 is behind","fix":"layerfs backup create shop b1"}]}
```

| Severity | Meaning |
| --- | --- |
| `error` | Something is broken. |
| `warning` | Something is off and may become a problem. |
| `notice` | Worth knowing; nothing is broken. |

A `not_checked` entry means a check gave no verdict, either because the thing
it depends on is stopped or failed (`blocked_by`) or because the answer did not
arrive (`reason`: `timeout`, `busy` or `lost`). It is not a pass.

Add `--detail` to see why. It adds `evidence` to each finding (paths, ids,
state words; never a secret), the worst `status`, `counts`, and a `tree` with
every check and its outcome. `--progress` sends one event per finished thing on
a long run.

## The fix

`fix` is one runnable shell line. When two commands are needed they are joined
with `&&`, for example `layerfs machine stop docker && layerfs machine start
docker`. A word in angle brackets, such as `<url>` or `<bucket>`, is something
only you know: fill it in.

Some findings have no `fix`. That means no single command repairs the cause, or
the obvious command could lose data. Read the `message`: it says what to do by
hand. Three cases to know:

- A damaged Store (`store.identity`, `store.schema`, `project.data`) is fixed by
  restoring a backup, `layerfs backup restore <bucket> <file> <store>`, and
  never by removing the Store.
- A machine record that is not valid JSON, or names an unknown platform, cannot
  be removed with `machine remove` either; the doctor gives no fix, and the file
  has to be repaired or deleted by hand.
- A Sandbox whose runtime has ended is fixed by `layerfs sandbox remove <x>`.
  The message says how many recorded Workspaces' uncommitted changes are lost.
  The call refuses a Sandbox that is in use.

Check the safety of a fix before you run it. Fixes that remove a Sandbox, a
project record or a Workspace's changes cannot be undone.

## Exit codes

A doctor exits 0 once it has made its report, whether or not it found
anything. Read the report, not the exit code, to learn the result. Exit 125
means the call was refused (for example, a name that is not a machine), and
the refusal says why.

## Run the full check

`project doctor --full` reads everything the main Branch and every live Branch
reach. `backup doctor --full` writes one object to the bucket and reads and
deletes it, which needs a bucket with versioning on. Both write nothing to your
data. `store doctor` never runs the data check.
