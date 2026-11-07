# project calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. Results were captured from the built `layerfs`
> command in a private home on 2026-11-02; ids are shortened.

A project is imported once from a host folder into an existing Store, and is
always named with its Store: `<store>/<project>`, such as `shop/api`. A bare
`api` is refused `InvalidInput`.

```js
project.init(project, folder, { timeout, on_progress })
project.list(store?, { page_size, page_pos })
project.get(project)
project.export(project, destination, { branch, on_progress })
project.remove(project)
```

## `project.init`

`project.init(project, folder, { timeout, on_progress })`

Imports the host folder `folder` as a new project into the Store named before
the `/`. Init never creates a Store: make one first with `store.create`.
Everything in the folder is imported, including `.git` and ignored files.

The folder is always on the caller's host. When the Store is a sqlite Store on
another machine, the folder is copied to that machine and Init runs there on
the copy, which is deleted afterwards: the machine needs free room for the copy
while Init runs, and progress reports `{ phase: "send", done, total }` in
bytes while the folder is copied.

Progress is `{ phase, done, total }` on every route (#536 §10b, from source
and tests): `check`, then `send` (another machine only; `done`/`total` in
bytes), `install` (Init adds the folder) and `record`; `done` and `total`
are null outside `send`.

| Arg | Meaning |
| --- | --- |
| `project` | `<store>/<project>`; names are 1–63 lowercase letters, digits, `_ . -`, starting and ending alphanumeric |
| `folder` | An absolute folder path on the caller's host |
| `timeout` | Milliseconds Init may take; default 300000 |
| `on_progress` | A function called with each progress update (shell: `--progress`, JSON lines on stderr) |

```js
import layerfs from "layerfs";

await layerfs.store.create("shop");
const p = await layerfs.project.init("shop/api", "/work/api", { on_progress: (u) => console.log(u) });
```

```bash
layerfs store create shop
layerfs project init shop/api /work/api --progress
```

```json
{"project":"shop/api","store":"shop","name":"api","kind":"sqlite","machine":"native",
 "created":1793574061265,"layer":"3275838904b6…","root":"be88b05d4e0c…"}
```

`layer` is `main`'s first Layer. The new project has one line, `main`, and no
Branch: fork one before you change anything. (S, the spec, describes the result
as `{ project, branch: "main", commit }`; the build returns the record above.)
A tiny folder sent no progress update in the live run (an older build;
the current one reports `check`, `install`, `record`).

Where Init runs depends on the Store: into a sqlite Store on `native` or a
PostgreSQL Store it runs in your process and reads the folder on this host.
Into a sqlite Store on another machine it runs in that machine's worker
(#536 H11), which reads `folder` **on that machine** (a path in the VM,
not on this host); the machine must be running
(`machine.start(name)` first).

**Refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `NotFound` | No Store of that name (message: ``store create <name>` makes one``) | Create the Store, or check the name |
| `Exists` | The Store already holds that project | Don't init again; the project is ready |
| `InvalidInput` | A bad name, a relative path or a bare project name | Fix the call |
| `Unavailable` | The Store's machine is stopped (message names `layerfs machine start <name>`) | Start the machine, then Init |
| `Failed`, `Unknown` | Init failed or its outcome is uncertain | Stop and report; read `project.list` and `log/init-failures.jsonl` in the home before acting |

**Start from a small skeleton** (shell), with a `.gitignore` and the
[`/.lww`](../rules/lww.md) it needs:

```bash
mkdir -p /work/api/.git
printf 'ref: refs/heads/main\n' > /work/api/.git/HEAD
printf '# API\n' > /work/api/README.md
printf 'node_modules/\ndist/\ncoverage/\n*.log\n' > /work/api/.gitignore
printf '%%include .gitignore\n' > /work/api/.lww
```

## `project.list`

`project.list(store?, { page_size, page_pos })`

Every project in the home, or one Store's.

```bash
layerfs project list
```

```json
{"items":[{"project":"shop/api","store":"shop","name":"api","kind":"sqlite","machine":"native","created":1793574061265}],"next_page_pos":null}
```

The live result had `next_page_pos` but no `has_more`.

## `project.get`

`project.get(project)`

The project's record: its Store, the Store's kind and machine.

```json
{"project":"shop/api","store":"shop","name":"api","kind":"sqlite","machine":"native","created":1793574061265}
```

## `project.remove`

`project.remove(project)`

Removes one project from its Store. Its Branches, Commits and `main` go with
it; there is no undo. The live result:

```json
{"stack":"3104401f11fb…","packs_kept":true,"project":"shop/api"}
```

`packs_kept: true`: the project's records are gone, but its stored content
stays in the Store file. Removing a project from a PostgreSQL Store is
`Unsupported` in this build. While a Sandbox of the project is recorded it is
refused `InUse`; `detail.related` lists each as `{ kind: "sandbox", name }`
(close them first). To delete a whole Store with every project in it, use
`store.remove`.

An agent working on a project never needs `remove`; it is for whoever owns the
home.
