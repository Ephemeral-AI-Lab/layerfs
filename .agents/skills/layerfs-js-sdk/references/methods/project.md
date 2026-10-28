# project methods

> **Status:** Current general guide. Part of the `layerfs-js-sdk` skill. Examples were
> captured live in issue #334 Phase A at `b30e4fb8f`. Opaque IDs are shortened;
> the full replies and trace are in `target/issue334/phase-a/`. The client
> factories and `list_sandboxes` are #428's; result shapes are unchanged.

Examples use `layerfs` as connected in [SKILL.md](../../SKILL.md#connect-to-a-project).

## `LayerFS.open(name)` and `close()`

A client of an existing project. It takes no options: settings come from the
environment (`LAYERFS_*` only), and an options object is
refused `InvalidInput`. `open` opens the project's Store and looks the project
up once, so a missing project is refused `NoSuchProject` at once; if it fails, nothing
stays open. There is no `new LayerFS(…)`: it's refused `InvalidInput`.

```js
import { LayerFS } from "@layerfs/agent";

const layerfs = await LayerFS.open("shop");
try {
  // … calls …
} finally {
  await layerfs.close();
}
```

`close()` releases the Store and leaves Workspaces mounted. Every later call on that
client is refused `InvalidInput`; a second `close()` does nothing. `await using layerfs =
await LayerFS.open("shop")` closes it at the end of the block.

## `LayerFS.init(name, sourcePath, timeout?)`

Creates the project, then opens it: copies a host folder into a new Store, once.
Afterwards the files live only in LayerFS.

| Arg | Type | Required | Meaning |
| --- | --- | --- | --- |
| `name` | string | yes | 1–63 characters from `a-z`, `0-9`, `_`, `-`, `.`, starting and ending with a letter or digit |
| `sourcePath` | string | yes | Host folder to copy. Everything in it is imported, including `.git` and ignored files |
| `timeout` | number | no | Milliseconds Init may take, a positive whole number. **Default 300 s (300000); no unlimited option**. Init reads every file, so a large tree (`node_modules`, build output) takes a while |

**Returns:** an open client; what Init returned is on its `created`:
`{ kind: "Initialized", project, layer, … }`, where `layer` is `main`'s first Layer.

**Refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `Exists` | This `LAYERFS_HOME` already has the project | Don't init again: the project is ready to use |
| `InvalidInput` | Bad path, name or timeout, or an options object | Use an absolute folder path and a positive whole `timeout`; settings go in the environment |
| `Failed` with `Deadline` in the message | Init ran longer than `timeout`; the project's entry is kept | Stop and report; don't init the name again |
| `Unknown`, `Failed` | Init is uncertain or failed | Stop and report |

**Start from a small skeleton** (shell): a folder with a `README.md` and a `.git/HEAD`,
the shape the SDK's own test harness uses.

```bash
mkdir -p /work/shop-skeleton/.git
printf 'ref: refs/heads/main\n' > /work/shop-skeleton/.git/HEAD
printf '# Shop\n' > /work/shop-skeleton/README.md
```

```js
const layerfs = await LayerFS.init("shop-a-methods", "/work/shop-skeleton");
console.log(JSON.stringify(layerfs.created, null, 2));
```
```json
{
  "kind": "Initialized",
  "project": "shop-a-methods",
  "layer": "324afe090aa7…",
  "root": "e34e2dc43565…",
  "store": "/layerfs-store/global/store.sqlite"
}
```

`store` is the installed Store file (`LAYERFS_STORE_PATH`). A `leftover: { path, message }`
field appears only if Init could not delete its temporary sealed copy; the project is
still ready, so report it and go on.

**Notes:** run it once per project. Later processes with the same `LAYERFS_HOME` use
`LayerFS.open(name)`; `fs.*` and `branch.*` calls work right away.

## `project.list_branches(pageSize?, pagePos?)`

Lists the project's Branches, including the synthetic `main` entry. For `main`,
`id` and `head` are null and `base` is its current publication Layer, even after
publication. Record the Layer and its published source Commit when reporting the
final main reference.

| Arg | Type | Required | Meaning |
| --- | --- | --- | --- |
| `pageSize` | number | no | 1–1000, default 50 |
| `pagePos` | number | no | Offset, default 0 |

**Returns:** a page `{ items, hasMore, nextPagePos }`. Each item:

| Field | Meaning |
| --- | --- |
| `name` | Branch name |
| `id` | Branch id |
| `base` | The `main` Layer it's based on |
| `head` | Newest Commit, or `null` before its first Commit |
| `forkedFrom` | The node it was forked from, or `null` |

**Refusals:** `NoSuchProject` (wrong name, or `init` not run with this `LAYERFS_HOME`),
`InvalidInput`.

```js
const page = await layerfs.project.list_branches();
const exists = page.items.some((b) => b.name === "cart");
console.log(JSON.stringify(page, null, 2));
```
```json
{
  "items": [
    {
      "name": "main",
      "id": null,
      "base": "3279dac7de68…",
      "head": null
    },
    {
      "name": "work",
      "id": "1190a141ae4473594aaf0536f674a3fb99",
      "base": "32a35edb1f9f…",
      "head": "12804de7ce8d…",
      "forkedFrom": "32a35edb1f9f…"
    }
  ],
  "hasMore": false,
  "nextPagePos": null
}
```

## `project.list_sandboxes(pageSize?, pagePos?)`

The project's running Sandboxes, lowest number first, from their records. It
inspects each recorded container once, dials no daemon and starts nothing; a Sandbox
that closed itself or whose container is gone is left out (the runtime reclaims it).

| Arg | Type | Required | Meaning |
| --- | --- | --- | --- |
| `pageSize` | number | no | 1–1000, default 50 |
| `pagePos` | number | no | Offset, default 0 |

**Returns:** a page `{ items, hasMore, nextPagePos }`. Each item is
`{ sandbox, name, image, workspaces }`: its number, its container name, the image digest
it runs (one project's Sandboxes may differ) and how many Workspaces it holds.

```js
const { items } = await layerfs.project.list_sandboxes();
for (const s of items) console.log(s.sandbox, s.name, s.workspaces);
```
