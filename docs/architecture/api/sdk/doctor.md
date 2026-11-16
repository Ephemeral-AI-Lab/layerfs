# doctor family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `doctor` family's one call, `doctor.fix`, removes one leftover that `layerfs doctor` listed, within the [SDK](README.md). The report itself, `layerfs doctor` and `doctor()`, is outside the call table: see [Doctor](../../control/doctor/README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/doctor/fix.rs`](../../../../api/crates/layerfs-api/src/doctor/fix.rs), [`layerfs-control/src/doctor/leftover_fix.rs`](../../../../control/crates/layerfs-control/src/doctor/leftover_fix.rs), [`leftovers.rs`](../../../../control/crates/layerfs-control/src/doctor/leftovers.rs) |
| Runs by default | on the machine the id names: this process on `native`, else that machine's worker, one request |
| Design | [Checks: leftovers](../../control/doctor/checks.md#leftovers) (the kinds, their guards and the report's fields) |

## Calls

```text
 layerfs doctor                         a warning per leftover, with
   findings[].leftover { machine, kind, path, bytes, age,
                         id, fixable[, not_before] }
   leftovers[] { machine, complete, image_locks, unmount_markers }
        |
        v  id = <machine>:<kind>:<name>
 doctor.fix(id)  -> the machine: find the entry again, check its guard,
                    remove it, once
                 -> { id, machine, kind, path, removed: true, bytes }
```

| Call | Returns |
| --- | --- |
| [`doctor.fix`](#doctorfix) | `{ id, machine, kind, path, removed: true, bytes }` |

The package's `doctor` is the report function and carries the family:
`layerfs.doctor()` reports and `layerfs.doctor.fix(id)` removes. A client
made by `client()` has the family alone (`lfs.doctor.fix(id)`), and
`layerfs.aio.doctor.fix` is the coroutine. The MCP server lists no doctor
tool in either profile ([MCP](../front-ends/mcp.md#profiles)): like the
report, the repair is a person's. A folder scope refuses the call.

### doctor.fix

Removes the one leftover `id` names, under that kind's guard.

```text
JS      await layerfs.doctor.fix(id)
Python  layerfs.doctor.fix(id)
CLI     layerfs doctor fix <id>
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `id` | Text | a leftover's `id` from `layerfs doctor`: `<machine>:<kind>:<name>`. It never holds a path |

Returns `{ id, machine, kind, path, removed: true, bytes }`; `bytes` is what
the removed files held, `null` for a folder copy and an image temporary. One
attempt: nothing is retried or sent twice, and nothing but the one entry is
removed. The kinds are `run-folder`, `empty-root`, `init-copy`,
`backup-staging`, `image-tmp` and `image-part`.

| Error | When |
| --- | --- |
| `NotFound` | no leftover has that id now: an id no report gives (a path, `..`, an unknown kind or machine), an entry already removed, or a link in its place |
| `InUse` | the kind's guard does not hold: a Sandbox record names the run folder, its `owner` marker is missing or newer than a minute, a daemon of it runs, something is mounted there, a `backup create` of the Store or an unpack of the image is running, the image part was written within the hour, or a folder copy's Init may run until an hour after its deadline. Nothing was removed |
| `InvalidInput` | a kind that is only reported (`image-lock`, `unmount-marker`, a folder copy no line of `log/uncertain.jsonl` names); a missing `id`; a folder scope |
| `Unavailable` | the machine is stopped; `layerfs machine start <machine>` starts it |
| `Unknown` | the worker's answer was lost; run `layerfs doctor` to see whether the entry is still there |
| `Failed` | the mount table cannot be read, or a removal stopped at an I/O error (the message names where) |

## Related

- [Checks: leftovers](../../control/doctor/checks.md#leftovers).
- [Doctor](../../control/doctor/README.md).
