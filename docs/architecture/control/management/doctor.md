# Doctor

[Architecture](../../README.md) › [Control](../README.md) › [Management](README.md) › Doctor

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Health checks over the home.

| | |
| --- | --- |
| Component | [Control](README.md) |
| Source | [`control/crates/layerfs-control/src/doctor`](../../../../control/crates/layerfs-control/src/doctor) |

`layerfs doctor` (CLI) and `layerfs.doctor()` (Node, Python) are outside
the call table (#536 H7). It reads the home and reports; it takes no lock,
writes nothing (no lock file, no stale-lease removal) and starts no
machine. A home that changes meanwhile may be reported part way
([`doctor/mod.rs`](../../../../control/crates/layerfs-control/src/doctor/mod.rs)).

```text
 doctor(home)                                   report.rs
   detect(home)      (layout read only)
     unreadable  -> error on ("home", path)
     Empty       -> ok, no findings, home stays absent
     Old (1.1.0) -> warning: not converted; `layerfs import-env`
     Previous(2) -> convert::check    blockers as errors + warning
     Current(3)  -> scan::scan        records, "v", modes
                    checks::check     references, intents, leases,
                                      partial copies
                    workers::check    each recorded worker's hello
   -> { ok, findings: [{severity, entity: {kind, name},
                        message, fix}] }
```

## Checks

From [`checks.rs`](../../../../control/crates/layerfs-control/src/doctor/checks.rs) and [`convert.rs`](../../../../control/crates/layerfs-control/src/doctor/convert.rs):

| Entity | Finding | Severity | Fix it names |
| --- | --- | --- | --- |
| machine | recorded `daemon` file missing | error | `layerfs machine update <m> --daemon <path>` |
| store | record cannot be read | error | repair the record by hand |
| store | sqlite Store on a machine with no record | error | make the machine again, or delete the folder by hand |
| store | 1.1.0 Store whose machine is unknown | warning | `layerfs import-env` |
| store | stale lease of an ended process (read, never removed) | warning | the next call on the Store removes it |
| store | `store.sqlite.incoming` left by an interrupted or certain-failed transfer, on `native` or on a push's target machine: from the push's intent, or, once a certain failure dropped it, from the record's `incoming_on` (a machine's is read through its worker's `stat`) | warning | a push's target: `layerfs store push <n> <machine>` replaces it, or `layerfs store remove <n>` deletes it; on `native`: `layerfs store pull <n>` when the Store is off `native`, else delete by hand |
| store | a copy a push or pull did not retire (from `last_transfer`) | warning | the next `store push` or `store pull` deletes it |
| sandbox | record names no project, or its Store, project or machine is gone | error | `layerfs sandbox close <n>` |
| machine, store, sandbox | gone owner's uncertain `op` | error | show: `machine status`, `store status`, `sandbox get`; clear: the entity's clearing call |
| machine, store, sandbox | gone owner's certain `op` | warning | nothing: the next operation replaces it |
| exec | a command record (`workspace/<id>/exec/<exec id>.json`) whose owner process is gone: stale (layerfs-project#113, R12) | warning | `layerfs workspace exec-cancel <sb>:<ws> <exec id>`, or the Workspace's unmount or the Sandbox's close, ends what it still runs and removes it |
| exec | a file under `exec/` that is not a command record | error | repair or remove it by hand |
| workspace | a `workspace/<id>/` folder with no Workspace record `<id>.json` beside it | warning | nothing to run: the next cancel, unmount or close of that Workspace settles its records |
| home (layout 2) | each conversion blocker (live lease, Sandbox, create intent, move or removal) | error | end what blocks it |
| home (layout 2) | conversion still to come | warning | the next call converts |

A live owner's `op` is not a finding: the operation is still running.

## Scan

[`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) reads every record of a layout 3 home once:

| Read | Error when | Source |
| --- | --- | --- |
| each machine, Store, project, Sandbox record | it does not parse, or an entity record's `v` is not 1 | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `record` |
| record file mode | not 0600 (group or others may read) | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `file_mode` |
| folder mode | others may list or change it, or its group may use it; 0701 is allowed | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `folder` |
| Store folder | it has projects and no Store record | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `store` |
| Sandbox folder | its record, `control.json` and Workspace records | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `sandbox` |
| Workspace folder | each `workspace/<id>/` folder's mode, and each command record under its `exec/`; the doctor ends no command and removes no record | [`scan.rs`](../../../../control/crates/layerfs-control/src/doctor/scan.rs) `execs` |

The scan returns `Scanned` (the parsed machines, Stores, projects and
Sandboxes, and the names with a record file) for the later checks.
Built-in `native` and `docker` count as present without a record.

## Report

```text
 { "ok": false,
   "findings": [
     { "severity": "error" | "warning",
       "entity":   { "kind": "home|machine|store|project|sandbox",
                     "name": "<name>" },
       "message":  "<what is wrong>",
       "fix":      "<the call or step that fixes it>" } ] }
```

| Field | Meaning | Source |
| --- | --- | --- |
| `ok` | false when any finding is an `error` | [`report.rs`](../../../../control/crates/layerfs-control/src/doctor/report.rs) |
| `severity` | `error`: a call on the entity is refused or does the wrong thing; `warning`: left over or out of date | [`report.rs`](../../../../control/crates/layerfs-control/src/doctor/report.rs) |
| order | findings in the order the checks made them | [`report.rs`](../../../../control/crates/layerfs-control/src/doctor/report.rs) |

`doctor_json` wraps the report for the Node and Python bindings as
`{ value }` or `{ refused: {code, message, detail} }`; a refusal there
means the home path itself could not be resolved. The CLI
(`layerfs doctor [--text]`,
[`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs)) prints the
report; see [CLI](../../api/front-ends/cli.md).

## Worker checks

[`workers.rs`](../../../../control/crates/layerfs-control/src/doctor/workers.rs) dials each machine whose `worker.json` is
recorded, for its hello alone:

```text
 caller                         worker (recorded address)
   |-- { secret } ------------->|
   |<- { ready, protocol, bundle }  (or Denied, or no answer)
   x  connection closed: no request is ever sent
```

| Answer | Finding |
| --- | --- |
| `worker.json` without an address | error, fix: `layerfs machine stop <m>`, then `start` |
| hello of another protocol or bundle (`Hello::expected`) | warning: calls to it are refused; stop and start the machine, rebuilding the Linux bundle first if it was sealed before this build |
| no answer, or a refused secret | warning: `layerfs machine start <m>` starts it again, `layerfs machine stop <m>` records it stopped |

## Related

- [Home and records](home-and-records.md): the records, modes, intents and leases it reads
- [Workers](../routing/workers.md): the hello frame
- [Errors](../../api/call-table/errors.md#uncertain-outcomes): `Unresolved` and `Unknown`
- [Front ends](../../api/front-ends/README.md)

---

← [Management: Families](families.md) · [Up: Management](README.md) · [Control: Routing and workers](../routing/README.md) →
