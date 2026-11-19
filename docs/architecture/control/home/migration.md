# Migration

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How the first call on a home creates it, and why an older LayerFS's home is
refused rather than converted, within [Home](README.md).

This build converts nothing. The conversion of 1.1.0 and layout 2 homes,
`layerfs import-env` and the clearing of an older bundle's leftovers in a
worker's root were removed (owner, 2026-11-19: "cut and cleanup old
incompatible aggressively, no lfs, and fallback to old 2.0 or 1.x"). The
page keeps its name so links to it hold.

| | |
| --- | --- |
| Source | [`home/layout.rs`](../../../../control/crates/layerfs-home/src/home/layout.rs), [`home/guest.rs`](../../../../control/crates/layerfs-home/src/home/guest.rs), [`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) |

## Opening a home (`layout.rs`)

```text
 open(home)
   worker root? ----------------------> answer the caller's home id; no write
   detect(home)  (reads only)
     Current(3) ----------------------> use the home
     Older ---------------------------> Unsupported, fix
                                        `mv <home> <home>.old && layerfs home init`;
                                        nothing written
     Empty
       take layerfs.lock, detect again
         Empty   -> create layerfs.json (layout 3, new id)
         Older   -> Unsupported, the same fix
     newer layout -> Unsupported;  unreadable or malformed -> Failed
```

| Layout | Recognised by | Source |
| --- | --- | --- |
| Empty | no `layerfs.json` and none of 1.1.0's records (a `layerfs.lock` alone is still empty), or no folder at all | [`home/layout.rs`](../../../../control/crates/layerfs-home/src/home/layout.rs) `detect` |
| Older | `layerfs.json` with `layout: 2`; or no `layerfs.json` and one of `.stores`, `stores.json`, `stores.v1.json`, `stores.json.lock`, `remotes`, `remote-state`, `.runtime`, `worker`, `traces`, or a `<p>/project.record` (a 1.1.0 home) | same, `OLD` |
| Current (3) | `layerfs.json` with `layout: 3` and a 32-hex `id` | same, `LAYOUT_VERSION` |

Every call whose entry opens the home runs `open` first
([`calls/dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs));
`home.init` runs it itself, to say what it did. A current home costs one
read of `layerfs.json`.

| Call | On an older home | Source |
| --- | --- | --- |
| any call that opens the home | refused `Unsupported`, "the home of an older LayerFS; this build does not convert it", with the fix `mv <home> <home>.old && layerfs home init`; nothing is written | [`home/layout.rs`](../../../../control/crates/layerfs-home/src/home/layout.rs) `older`, `older_fix` |
| `home.init` | the same refusal; on an empty home `outcome: "created"`, on a current one `"unchanged"` | [`home.rs`](../../../../control/crates/layerfs-control/src/home.rs) `init` |
| `home.get` | `state: "broken"`, `reason: "outdated"`, the same `fix`; it reads only | same, `get` |
| `home.doctor` | `home.layout` fails with the same fix; `home.records` is not run | [`doctor/host_target.rs`](../../../../control/crates/layerfs-control/src/doctor/host_target.rs) `layout` |

The fix keeps the old folder and makes a new, empty home at the path. Its
Stores, Sandboxes and machines are not carried over, and a 1.1.0
environment's settings are not imported. No conversion report or failure
log is written (`log/migration*.json`, `log/init-failures.jsonl`,
`log/import-env.jsonl`, `log/1.1.0/`).

## Worker roots (`home/guest.rs`)

A worker's root is not a home: `open` answers the home id its caller sent
and writes nothing, and `home.init` there is refused `InvalidInput`. The
root is never created, converted or cleared of an older bundle's files
([`home/guest.rs`](../../../../control/crates/layerfs-home/src/home/guest.rs)).

## Inventory (`inventory.rs`)

The relationships no record stores, read when a call needs them
([`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs)):

| Read or check | Answers |
| --- | --- |
| `machines`, `stores_on`, `sandboxes_on`, `hosted_on` | every machine record; the sqlite Stores and Sandboxes of a machine; old Firecracker records a Lima machine hosted |
| `sandboxes_of_store`, `sandboxes_of_project` | the Sandboxes of a Store or project |
| `can_remove_machine` | `machine.remove`: `InUse` while it holds sqlite Stores, Sandboxes or hosted Firecracker records (a PostgreSQL Store is on no machine); next fields `stores`, `sandboxes`, `machines`; held by Sandboxes alone, `fix` `layerfs sandbox remove <s> && … && layerfs machine remove <m>`, none when its record says it is stopped |
| `can_stop_machine` | `machine.stop`: `InUse` while Sandboxes are recorded on it; next field `sandboxes`; `fix` `layerfs sandbox remove <s> && … && layerfs machine stop <m>` (the message names `--force` as the other way) |
| `can_change_machine` | `machine.update`: `InUse` while Sandboxes are recorded on it; next field `sandboxes`; `fix` `layerfs sandbox remove <s> && …` |
| `store_idle` | `store.move`, `remove`, `disconnect`, `backup.restore`: `InUse` while another live process holds a lease or a project's Sandbox is recorded (removes stale leases); next field `sandboxes`; with no live holder, `fix` `layerfs sandbox remove <s> && …` |
| `can_remove_project` | `project.delete`: `Busy` while a Sandbox of the project is recorded |

Each `InUse` or `Busy` lists what holds the entity in `detail.related` as
`{kind, name}`; an `InUse` also names them in the next fields above, flat
beside `error` (ruling R-LF2-1), and a `fix` may join several commands
with `&&`.

Layout 3 keeps each Sandbox at `sandbox/<name>/`, so a Sandbox by name is
one read; only the relationships list `sandbox/*`.

## Demonstration

| Test | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`home_layout.rs`](../../../../control/crates/layerfs-control/tests/home_layout.rs) | `cargo +1.85.1 test --locked -p layerfs-control --test home_layout` | macOS, Linux | an empty or missing home gets a layout and id; a 1.1.0 or layout 2 home is `Older`, refused with the fix and never written; a newer or broken layout is refused |

## Related

- [Home](README.md): the layout and record rules a new home is made with.
- [Doctor](../doctor/README.md): `home.layout` on an older home.
- User guide: [an older home](../../../versioned/2.0.0/home.md#an-older-home), [migration](../../../versioned/2.0.0/migration.md).
