# Project Init

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`project.init`: reading a native directory and publishing the project's first Commit into a Store that already exists, as a live add.

## The path

```text
 caller process            machine worker            Store
                           (sqlite Store on           (SQLite file or
                            another kernel)            PostgreSQL schema)
 ------------------------  ------------------------  ------------------
 table check; split
 <store>/<project>
 record unblocked
 plan: where it runs
   |
   +-- local sqlite: lease
   |   add_project: kernel guard, entry in the home
   |   writable open ---------------------------------> live Store
   |     import: scan, construct, Save -------------> short writes
   |                                                 [each: admission
   |                                                  turn, writer lock]
   |     discard working rows; Save::finish ---------> writes
   |     genesis Layer -----------------------------> 1 write tx
   |     fork first Branch -------------------------> 1 write tx
   |   close session (no seal)
   |
   +-- worker: walk folder here; room check
   =====|== process boundary: 127.0.0.1:<port> ==|
       project.stage x N -->  write the copy
       project.init -------->  lease; add_project on
                               the copy -----------> (same writes)
                               remove the copy
       <---------------------  {stack, layer, root}
   |
   +-- postgresql: add_project ----------------------> (same writes on
                                                     the schema)
 write the project record
 answer: project summary + layer, root
```

Init runs where the Store's calls run. Every Store write is a short
transaction through the Store's one writer; Init does not hold the publish
lane across transactions, so daemons and SDK processes keep reading and
publishing between its writes. The Store is not sealed.

## Steps

| # | What happens | Owning topic |
| --- | --- | --- |
| 1 | The call table checks `project` and `folder`; the Store must be recorded and hold no blocking intent. | [SDK: `project.init`](../api/sdk/project.md#projectinit), [Home: the `op` intent](../control/home/README.md#the-op-intent) |
| 2 | Where Init runs is decided from the Store record: the caller's process (PostgreSQL, or sqlite on a local machine) or the Store machine's worker. | [Control: where each call runs](../control/README.md#where-each-call-runs), [Projects](../control/stores-and-projects/projects.md) |
| 3 | Worker route only: the caller walks the folder, refuses what Init would refuse, checks the machine has room, and sends a copy in `project.stage` parts. | [Projects](../control/stores-and-projects/projects.md), [Workers and reach](../control/machines/workers-and-reach.md) |
| 4 | `add_project` checks the Store's filesystem and file, creates the project's entry in the home and opens a writable session on the live Store. | [Project Init: live add](../engine/project-init/README.md#live-add), [Stores and projects](../control/stores-and-projects/README.md#what-it-does) |
| 5 | Import scans the complete folder without following links and keeps input-sized working state in the acquisition tables, not in memory. | [Project Init: import](../engine/project-init/README.md#import) |
| 6 | Construction turns file content into chunked file objects and directories into tree objects, saved through one Storage Save into packs. | [Project Init: construction](../engine/project-init/README.md#construction), [Files](../core/files/README.md#construction), [Filesystem trees](../core/filesystem-trees/README.md), [Physical storage: saves](../core/physical-storage/README.md#saves) |
| 7 | Working rows are discarded, `Save::finish` completes the objects, and the LayerStack and its genesis Layer are written in one transaction. | [History: records](../core/history/README.md#records), [Store](../engine/store/README.md#what-it-does) |
| 8 | The first Branch is forked from the genesis Layer and the session closes without a seal. | [Project Init: live add](../engine/project-init/README.md#live-add), [Store calls: Branch history](../engine/store-calls/README.md#branch-history) |
| 9 | The caller writes the project record and answers the project summary with `layer` and `root`. | [Stores and projects](../control/stores-and-projects/README.md#what-it-does) |

An uncertain outcome keeps the acquisition record and the project's entry for
diagnosis; nothing is guessed or resent
([Project Init: limits and failures](../engine/project-init/README.md#limits-and-failures)).
Sealing a new Store and the one-time install stream belong to `store.create`
and tests, not to this flow
([Project Init: seal and install](../engine/project-init/README.md#seal-and-install)).

## Related

- [Project Init](../engine/project-init/README.md), [Projects](../control/stores-and-projects/projects.md)
- [SDK: project](../api/sdk/project.md)
- [Store](../engine/store/README.md), [Core: Physical storage](../core/physical-storage/README.md)
