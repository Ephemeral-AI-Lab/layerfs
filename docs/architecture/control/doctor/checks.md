# Checks

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

This page covers the whole-home survey that `layerfs doctor` runs with no
target, and the two report shapes, within [Doctor](README.md).

| | |
| --- | --- |
| Source | [`layerfs-control/src/doctor`](../../../../control/crates/layerfs-control/src/doctor): `report.rs`, `scan.rs`, `checks.rs`, `convert.rs`, `workers.rs`, `capacity.rs`, `check_report.rs` |

## Report

```text
 doctor(home)
   detect(home)             layout read only
     unreadable    -> error on ("home", path)
     Empty         -> ok, no findings; the home stays absent
     Old (1.1.0)   -> warning: not converted; `layerfs import-env`
     Previous (2)  -> convert::check
     Current (3)   -> scan -> checks -> capacity -> workers
   -> { "ok": bool,
        "findings": [ { "severity": "error" | "warning",
                        "entity":   { "kind", "name" },
                        "message":  "<what is wrong>",
                        "fix":      "<the call or step that fixes it>" } ] }
```

| Field or case | Meaning | Source |
| --- | --- | --- |
| `ok` | false when any finding is an `error` | [`report.rs`](../../../../control/crates/layerfs-control/src/doctor/report.rs) |
| `severity` | `error`: a call on the entity is refused or does the wrong thing; `warning`: left over or out of date, nothing is refused for it | [`report.rs`](../../../../control/crates/layerfs-control/src/doctor/report.rs) |
| `entity.kind` | `home`, `machine`, `store`, `project`, `sandbox`, `workspace`, `exec` or `bucket` | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs), [`checks.rs`](../../../../control/crates/layerfs-control/src/doctor/checks.rs) |
| order | findings in the order the checks made them | [`report.rs`](../../../../control/crates/layerfs-control/src/doctor/report.rs) |
| refusal | a top folder that cannot be listed, or a lease folder that cannot be read, refuses the whole survey (`Failed`) | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs), [`checks.rs`](../../../../control/crates/layerfs-control/src/doctor/checks.rs) |

The survey takes no lock, so a home that changes meanwhile may be reported
part way. The bindings reach it through `layerfs_api::doctor_options_json`
with empty options, which answers `{ value }` or
`{ refused: { code, message, detail } }`; `doctor_json` is the same wrapper
without options.

## Scan

```text
 home/ layerfs.json   machine/ store/ sandbox/ backup/   folder modes
   machine/<m>/  record.json (v 1)   worker.json
   store/<s>/    record.json (v 1)   project/<p>/record.json (v 1)
   sandbox/<x>/  record.json (v 1)   control.json
                 workspace/<id>.json (v 1)
                 workspace/<id>/exec/<exec id>.json (v 1)
   backup/<b>/   record.json (v 1)
 -> Scanned { machines, machine_names, stores, store_names,
              projects, sandboxes }
```

| Read | Finding | Source |
| --- | --- | --- |
| each record | `error` when it does not parse ("repair or remove it by hand"), or when an entity record's `v` is not 1 ("use the LayerFS build that wrote it") | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `record` |
| record file mode | `error` when group or others may read it; fix `chmod 600 <path>` | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `file_mode` |
| folder mode | `error` when others may list or change it or its group may use it (0701 is allowed, for a namespaces Sandbox); fix `chmod g-rwx,o-rw <path>` | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `folder` |
| Store folder | `error` on each project in a Store folder with no Store record; fix: restore the record or delete the folder by hand | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `store` |
| Workspace folder | `warning` for `workspace/<id>/` with no `<id>.json` beside it; fix: its Workspace was unmounted, remove the folder by hand | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `execs` |
| command record | `warning` when its owner process is gone; fix `layerfs workspace exec-cancel <sb>:<ws> <exec id>`, or the Workspace's unmount or the Sandbox's close; `error` when a file under `exec/` does not read as one | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `execs` |

Hidden (dot) entries are skipped. The scan ends no command and removes no
record. Built-in `native` and `docker` count as present without a record.

## Record checks

```text
 Scanned -> machines   daemon file, op
         -> stores     op, placement, partial copies, leftover copy, leases
         -> sandboxes  op, project, Store, machine
```

| Entity | Finding | Severity | Fix it names |
| --- | --- | --- | --- |
| machine | recorded `daemon` file missing | error | `layerfs machine update <m> --daemon <path>` |
| store | record cannot be read | error | repair the record by hand |
| store | SQLite Store on a non-built-in machine with no record | error | restore the machine's record, or delete the Store folder by hand |
| store | 1.1.0 Store whose machine is unknown | warning | `layerfs import-env` |
| store | stale lease of an ended process (read, never removed) | warning | the next call on the Store removes it, or delete `store/<s>/open/<pid>` |
| store | local `store.sqlite.incoming` | warning | on `native`: delete it by hand; else `layerfs store pull <s>`, or delete it |
| store | a push target's `store.sqlite.incoming` with bytes, read by the worker's `stat` | warning | Store on `native`: `layerfs store push <s> <machine>` or `layerfs store remove <s>`; else `store pull` or `store remove` |
| store | a copy a push or pull did not retire (from the record) | warning | the next `layerfs store push <s> <machine>` (on `native`) or `store pull <s>` deletes it first |
| sandbox | record names no project, or its Store, project or machine is gone | error | `layerfs sandbox close <x>` |
| machine, store, sandbox | ended owner's uncertain `op` | error | show: `machine status`, `store status`, `sandbox get`; clear: `machine remove`, the Store's clearing call, `sandbox close` |
| machine, store, sandbox | ended owner's certain `op` | warning | nothing: the next operation replaces it |

A live owner's `op` is not a finding. A running Store transfer owns its
partial copy, so `incoming` is not read while one runs; an unresolved
intent's own finding names its clearing call, so no leftover copy is
reported then. A push's target-side copy is reported by `incoming`, not as
a leftover. Source: [`checks.rs`](../../../../control/crates/layerfs-control/src/doctor/checks.rs).

The host target runs the same checks through `check_local`, which skips the
remote `stat`.

## Conversion

```text
 layout 2 home -> layout_2_blockers(home)   the conversion's own survey
   each blocker: live lease or Store move/removal  -> store
                 live or uncertain Sandbox          -> sandbox
                 create intent of unknown outcome   -> project
                 machine removal                    -> machine
     -> error on that entity, with its own fix
   always -> warning: converted to layout 3 once nothing is live
```

| Case | Finding | Source |
| --- | --- | --- |
| survey fails | `error` on the home; the first call reports what refuses the conversion | [`convert.rs`](../../../../control/crates/layerfs-control/src/doctor/convert.rs) |
| blocker | `error` naming the state and the step that ends it (let the process finish, close the Sandbox with the build that started it, remove what an intent left) | [`convert.rs`](../../../../control/crates/layerfs-control/src/doctor/convert.rs) |
| conversion still to come | `warning`; "the first call on the home converts it", or "end what refuses the conversion" | [`convert.rs`](../../../../control/crates/layerfs-control/src/doctor/convert.rs) |

Stale leases stay; nothing is converted.

## Workers

```text
 caller                          worker (address in worker.json)
   |-- { secret } ------------->|      connect 10 s, hello 10 s
   |<- { ready, protocol, bundle }     (or a refusal, or no answer)
   x   closed: no request is sent, nothing is started
```

| Answer | Finding | Source |
| --- | --- | --- |
| `worker.json` without a parsable address | error; fix `layerfs machine stop <m>`, then `layerfs machine start <m>` | [`workers.rs`](../../../../control/crates/layerfs-control/src/doctor/workers.rs) |
| hello of another protocol or bundle (`Hello::expected`) | warning: calls to it are refused; fix stop then start, rebuilding the Linux bundle (`python3 tools/linux_bundle.py`) first if it was sealed before this build | [`workers.rs`](../../../../control/crates/layerfs-control/src/doctor/workers.rs), [`hello.rs`](../../../../control/crates/layerfs-machine/src/worker/hello.rs) |
| no answer, or a refused secret | warning; fix `layerfs machine start <m>` starts it again, `layerfs machine stop <m>` records it stopped | [`workers.rs`](../../../../control/crates/layerfs-control/src/doctor/workers.rs) |

A `worker.json` that does not parse is the scan's finding and is not
dialed. The hello is described in [Workers and reach](../machines/workers-and-reach.md).

## Capacity

```text
 each Sandbox with an id, state "running" and a known daemon endpoint
   -> one bounded query of backend.daemon.connection_failures (2 s)
        answer   -> retained failures?  exhausted pool?
        refusal  -> Capacity on daemon_control_slot, or no answer
```

| Case | Finding | Source |
| --- | --- | --- |
| retained connection failures (`count` of `limit`, `dropped`, TTL) | warning; `layerfs sandbox get <x>` shows `daemon_capacity` | [`capacity.rs`](../../../../control/crates/layerfs-control/src/doctor/capacity.rs) |
| no vacant pool control slot | warning: a call that needs one is refused `Capacity` on `daemon_control_slot` | [`capacity.rs`](../../../../control/crates/layerfs-control/src/doctor/capacity.rs) |
| query refused `Capacity` on `daemon_control_slot` | warning with `pool_vacant`, `reserved_vacant`, `retained` | [`capacity.rs`](../../../../control/crates/layerfs-control/src/doctor/capacity.rs) |
| no answer within 2 s | warning; `layerfs sandbox get <x>` shows whether it runs, `layerfs sandbox close <x>` closes one whose daemon is gone | [`capacity.rs`](../../../../control/crates/layerfs-control/src/doctor/capacity.rs), [`capacity.rs` (sandbox)](../../../../control/crates/layerfs-sandbox/src/daemon/capacity.rs) |

A stopped daemon is a normal state, so none of these is an error. A
Sandbox whose record names no endpoint is not queried. The query is the one
`sandbox.get` uses for its `daemon_capacity` field.

## Check report

The explicit targets report through `Checks` instead of `Findings`.

```text
 { "ok", "complete", "target": {kind, name}, "operation", "mode", "home",
   "checks": [ { id, code, outcome, severity, entity, message, evidence,
                 fix, blocked_by, observed_ms, duration_ms } ],
   "findings": [ { severity, entity, message, fix, check_id } ],
   "counts": { outcome: n }, "truncated", "observed_at_ms", "duration_ms" }
```

| Outcome | Severity | Meaning | Source |
| --- | --- | --- | --- |
| `pass` | none | expected behaviour was observed | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |
| `fail` | `error` or `warning` | a concrete problem was observed; not by itself corruption | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |
| `blocked` | none | a failed or unobserved prerequisite, named in `blocked_by`, prevented it | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |
| `skipped` | none | not applicable, or outside the selected mode | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |
| `unknown` | none | the requested coverage could not be established | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |

| Field | Rule | Source |
| --- | --- | --- |
| `ok` | no `error` counted, retained or not | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |
| `complete` | false after any `blocked` or `unknown`, any truncation, or an explicit `incomplete()` | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |
| `findings` | the checks with a severity, for readers of the survey's shape | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |
| retention | at most 1024 checks; messages cut at 4096 characters; evidence over 16 KiB replaced by `{ truncated: true }`; `counts` keep every outcome | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |
| evidence, fix, duration | attach to the latest retained check of that id; a dropped repeat cannot attach to an older row | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |
| `duration_ms` | per check, null when not measured | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |

`target`, `operation`, `mode` and `home` are added by the dispatcher
([`dispatch.rs`](../../../../control/crates/layerfs-control/src/doctor/dispatch.rs)).
Callers redact evidence before adding it.

## Related

- [Doctor](README.md): targets, invariants and limits
- [Home](../home/README.md): the records, modes, intents and leases the scan reads
- [Stores and projects](../stores-and-projects/README.md): placement, leases and transfers
- [Workers and reach](../machines/workers-and-reach.md): the hello frame
- [Sandboxes](../sandboxes/README.md): daemon capacity and `sandbox.get`
- [Front ends](../../api/front-ends/README.md): `layerfs doctor`, Node and Python `doctor()`
