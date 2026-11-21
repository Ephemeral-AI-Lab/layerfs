# LayerFS 2.0 call surface: frozen spec

> **Status:** Frozen specification, version 20, 2026-11-19 (version 20: `home.leftovers` and
> `home.prune` removed, owner 2026-11-19; version 18 of 2026-11-18 brought up to
> `layerfs_v2_0_0` @ `ff114a5265`, #738–#741). Base `layerfs_v2_0_0` @ `caff58a43b`.
> Target behavior, not implemented behavior: source and the call table establish what is built.

The public call table of LayerFS 2.0, reshaped: two-level `family.call` names, a
short answer form for agents and a long form with `--detail` for the UI, one
six-state status model, one doctor per family, and the Doctor page of the
dsh-layerfs plugin.

| File | What it is |
| --- | --- |
| [calls.json](calls.json) | The contract as data: every call's name, positionals, options, `--with` sections, short keys, long keys, examples; the state, refusal, list, progress and doctor shapes. Generated; the contract test reads it. |
| [families/](families/) | The source of `calls.json`: one JSON file per family. [SCHEMA.md](families/SCHEMA.md) is their format. |
| [DECISIONS.md](DECISIONS.md) | Rulings R1–R9, corrections C1–C14, every other decision applied at the freeze, the version 19 and 20 changes, and the coordinator rulings after the 3080 pass. |
| [layerfs-call-surface.html](layerfs-call-surface.html) | The readable page, generated. |
| [status.html](status.html), [pg.html](pg.html) | The status model with the Doctor section, and PostgreSQL Stores (sources of the page). |
| [today-table.json](today-table.json) | The call table of the base commit, extracted from the code. |
| [rollout-body.html](rollout-body.html) | The pre-implementation review and the six-phase rollout plan. |
| [doctor-page/index.html](doctor-page/index.html) | The working mock of the Doctor page (vanilla JS); the port's visual and behavioural reference. |

Rebuild after editing a family file, from this folder:

```sh
python3 -I build.py "$PWD"
```

The build asserts the rules in `families/SCHEMA.md` and the decisions, and
writes `calls.json`, `pairs.html` and `layerfs-call-surface.html` only when
every rule passes. `python3 -I build.py "$PWD" --today <repo>` re-extracts
`today-table.json` from a checkout.

84 calls in 10 families: project 9, file 14, sandbox 7, workspace 7, branch 6,
commit 3, home 5, machine 14, store 13, backup 6 (version 20 removed
`home.leftovers` and `home.prune`; this branch adds `project.prune`; see
DECISIONS.md).
