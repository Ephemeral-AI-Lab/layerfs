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

## Explicit host diagnosis

`layerfs doctor host` selects only the local installation and home. Unlike
legacy no-argument doctor, this target never dials workers or inspects remote
transfer files. It reports the calling build and native prerequisites, measures
local filesystem capacity, and observes home layout, local records, modes,
references, operation owners, leases, command records and local transfer
remnants. Recorded daemon executability and applicable Linux bundle manifests
and required files are inspected without deployment. Bundle content hashes are
not recomputed and that coverage is reported explicitly. Optional unused
providers are not probed.

An absent home is reported as uninitialized and stays absent. Older layouts
are surveyed without conversion. Metadata inventories before and after the
record survey detect concurrent changes; an unstable or unreadable inventory
produces an incomplete result rather than a stable consistency verdict. This
observation is not a transaction over the home, and cannot exclude a change
that is reversed between observations. Local filesystem calls themselves
remain synchronous; the budget is checked between observations. The capacity
subprocess has its own bounded deadline and is killed/reaped when exhausted.
No machine-specific cleanup thresholds are used as product health limits.

## Explicit target commands

The legacy no-argument entry above remains unchanged. Explicit diagnosis stays
outside the ordinary call dispatcher, which can initialize or convert a home.

```bash
layerfs doctor host
layerfs doctor machine lima1
layerfs doctor sandbox api-dev
layerfs doctor store integrity app
layerfs doctor store integrity app --quick --timeout 10m
layerfs doctor store connection team-db
layerfs doctor backup connection production
layerfs doctor backup connection production --write-test
```

All explicit commands accept `--timeout <duration>` and `--text`; JSON is the
default. Durations are positive whole `ms`, `s`, `m` or `h` values. Host, Machine
and Sandbox default to 30 seconds. PostgreSQL and backup connection default to
10 seconds, or 30 seconds for the explicit backup write test. Store integrity
has no arbitrary total default cutoff: it accepts an explicit overall deadline,
while individual external probes must still be bounded. These are diagnostic
budgets, not runtime limits for ordinary commands or Commits.

Sandbox diagnosis always includes its daemon. `store connection` accepts only a
registered PostgreSQL Store; `store integrity` selects SQLite or PostgreSQL from
the Store record. Backup connection takes a registered backup alias, including
its configured prefix and credential references. No raw passwords, access keys,
or signed URLs belong in command options or reports.

### Outcomes and coverage

Explicit reports retain legacy `ok` and `findings` and add `target`, `operation`,
`mode`, `home`, `checks`, `counts`, `complete`, `truncated` and observation timing.
Each check has stable `id`/`code`, separate outcome and severity, bounded redacted
evidence, `blocked_by`, and timing. A per-check duration is null when not measured.

| Outcome | Meaning |
| --- | --- |
| pass | Expected behavior was observed |
| fail | A concrete fault was observed; its cause is not necessarily corruption |
| blocked | A failed or unobserved prerequisite prevented this check |
| skipped | Not applicable or intentionally outside the selected mode |
| unknown | Requested coverage could not be established |

`ok` means no retained or counted error finding, not proof of complete health.
Always inspect `complete`, mode and coverage. A confirmed failure stays a failure
when later work times out, and the report also remains incomplete. Retained check
entries and evidence are bounded; truncation retains counts and prevents a
complete result. CLI reports still exit 0 once produced, including findings;
usage and refusal exits remain 2 and 125. `--text` shows all retained check
outcomes, including blocked and unknown results, rather than an empty findings
list being rendered as healthy.

### SDK selection

```js
await layerfs.doctor({ home, target: "host" });
await layerfs.doctor({ home, target: "machine", name: "lima1", timeout: "10s" });
await layerfs.doctor({ home, target: "store.integrity", name: "app", quick: true });
await layerfs.doctor({ home, target: "backup.connection", name: "production", write_test: true });
```

```python
layerfs.doctor(home, target="sandbox", name="api-dev", timeout="10s")
layerfs.doctor(home, target="store.connection", name="team-db")
```

Empty options still select legacy doctor. Unknown or misplaced options are
refused before diagnosis. `quick` belongs only to Store integrity and
`write_test` only to backup connection. The shared implementation owns deadlines;
a wrapper Promise timeout alone does not stop a synchronous database query.
