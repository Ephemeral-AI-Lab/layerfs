# Quickstart

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](README.md). It walks through one home, a Store,
a project, Branch and file calls, a Sandbox with a Workspace, and a Commit,
in the shell, in JavaScript and in Python.

Each example says whether it was run. "Run" means it was run on macOS
(arm64) against a private temporary home with the debug build of branch
`issue533-v2`, on 2026-11-02. Long ids in the output are shortened with `…`.

## Before you start

- **The command.** `cargo +1.85.1 build -p layerfs-cli` builds `layerfs`
  into the worktree's `target/debug/`. `layerfs help` lists every call.
- **Node.** The `@layerfs/node` package is in `api/node`. Its native addon is
  built with `npm --prefix api run build:native`.
- **Python.** The `layerfs` package is in `api/python`. Its module is built
  with `cd api/python && maturin build --locked` (a debug wheel; add
  `--release` for the one to install). A macOS arm64 wheel and a Linux x64
  wheel were built and installed on this branch; Linux arm64 was not.
  [api/README.md](../../../api/README.md#package-and-install) lists the
  steps to install the Node tarball, the wheel and the command outside the
  checkout.
- **A Sandbox** on the `native` machine needs the LayerFS daemon (see
  [Sandboxes](#4-a-sandbox-a-workspace-and-a-commit)).

No package is published. The steps above are from
[api/README.md](../../../api/README.md) and
[api/python/README.md](../../../api/python/README.md).

## 1. One home

The home is `LAYERFS_HOME`, or `~/.layerfs` when it is unset. It is created
on the first call. Use a private folder:

```bash
export LAYERFS_HOME="$HOME/lfs-demo"
```

Nothing else needs setting. See [the home](home.md).

## 2. A Store and a project (shell)

Run:

```console
$ layerfs store create shop
{"name":"shop","kind":"sqlite","machine":"native","profile":"disposable","created":1793574130507,"sqlite":"3.53.2","projects":[]}

$ layerfs project init shop/api ./src --progress
{"project":"shop/api","store":"shop","name":"api","kind":"sqlite","machine":"native","created":1793574130648,"layer":"3228…","root":"3e8c…"}

$ layerfs project list
{"items":[{"project":"shop/api","store":"shop","name":"api","kind":"sqlite","machine":"native","created":1793574130648}],"next_page_pos":null}
```

- `store create` makes an empty sqlite Store on the `native` machine.
- `project init` imports the folder `./src` as project `api` of Store
  `shop`. Init never creates a Store. In this run the small folder printed
  no progress lines; the current build prints `check`, `install` and
  `record` as `{"progress": {"phase", "done", "total"}}` lines on stderr.
- A project is always written `<store>/<project>`.

## 3. Branches and files (shell)

Run:

```console
$ layerfs branch fork shop/api agent main
{"kind":"Forked","branch":"agent","base":"3228…","head":null}

$ echo "# Shop API" | layerfs file write shop/api agent /README.md -m "Update the title"
{"kind":"Committed","branch":"agent","head":"123a…","commit":{"id":"123a…","branch":"agent","parent":null,…,"message":"Update the title",…}}

$ layerfs file read shop/api agent /README.md --text
# Shop API

$ layerfs file list shop/api agent / --text
kind  ok
snapshot  agent@3a54ebd60426
root  2865…
path  /

NAME       PATH        TYPE  INODE
README.md  /README.md  file  2
docs       /docs       dir   3
has_more  false
next_page_pos  -

$ layerfs file edit shop/api agent /docs/a.txt --old two --new three -m "edit a"
{"kind":"Committed","branch":"agent",…}

$ layerfs file diff shop/api main /docs/a.txt agent /docs/a.txt
{"kind":"Text",…,"content_equal":false,"same_inode":true,"hunks_omitted":false,"hunks":[{"op":" ","text":"one"},{"op":"-","text":"two"},{"op":"+","text":"three"},{"op":" ","text":""}]}

$ layerfs branch publish shop/api agent
{"kind":"Added","source":{"branch":"agent","commit":"1288…"},"published_layer":{…},"based_on":"3228…","overwrote_count":0,"overwrote":[],"overwrote_has_more":false}
```

- Each file write, edit, rm, mkdir, mv, cp and apply makes one Commit on the
  Branch. Without `-m`, the message is `Apply filesystem changes`.
- `file write` reads its content from stdin, or from `--from <file>`.
- `branch publish` makes the Branch's head the next Layer of `main`. `main`
  changes only this way.

## 4. A Sandbox, a Workspace and a Commit

**Not run** for this manual. The step 12 commit on this branch records one
live run of this flow on a macOS `native` machine (loopback NFS): create,
mount, exec, status, commit, unmount and close.

```bash
# not run
layerfs sandbox create shop/api tests native
layerfs workspace mount tests agent           # answers {"ws": "tests:/…", …}
layerfs workspace exec "tests:/…" "make test"
layerfs workspace status "tests:/…"
layerfs workspace commit "tests:/…" -m "test output"
layerfs workspace unmount "tests:/…"
layerfs sandbox close tests
```

- A Sandbox runs on the machine that holds its Store. A sqlite Store
  created by `store create` is on `native`, so its Sandboxes run on
  `native`. Another machine is refused `Unsupported`.
- On macOS, `native` runs the daemon as a host process with a loopback NFS
  mount. On Linux it uses namespaces and needs root.
- The `native` daemon is the native machine record's `daemon` (written by
  `layerfs import-env` from 1.1.0's `LAYERFS_DAEMON`), else
  `~/.cache/layerfs/bundle/<version>/<os>-<arch>/layerfs-daemon`.
- `workspace mount` answers the Workspace as `ws`, `<sandbox>:<path>`. Pass
  that string to the other `workspace` calls.
- `workspace exec` answers `exit_code`, `stdout` and `stderr` with exit 0.
  With `--stream` the shell writes the output and exits with the command's
  own code. Output over 1 MiB reaches only `--stream` (or the package's
  `stdout`/`stderr` functions); the JSON answer has that stream `null`
  with its byte count and `streamed: true`.
- `workspace commit` saves the Workspace's changes to its Branch, merging
  first if the Branch moved since the mount; `--force` publishes over a
  moved head instead.
- `sandbox close` refuses `MountsActive` while a Workspace is mounted;
  `--force` stops the Sandbox anyway and lists the Workspaces it could not
  drain under `unknown`.
- A Sandbox is not named `native` or `docker` (`InvalidInput`).

## 5. The same in JavaScript

Run (Store, Init, files and a refusal; not the Sandbox part):

```js
import layerfs, { LayerFSError } from "@layerfs/node";

const lfs = layerfs.client({ home: "/path/to/home" });
await lfs.store.create("web");
const project = await lfs.project.init("web/site", "./site", {
  on_progress: (p) => console.log(p),
});
await lfs.branch.fork("web/site", "agent", "main");
await lfs.file.write("web/site", "agent", "/README.md", "Hello\n", { message: "docs" });
const { text } = await lfs.file.read("web/site", "agent", "/README.md"); // "Hello\n"

try {
  await lfs.file.read("web/site", "agent", "/missing");
} catch (e) {
  if (e instanceof LayerFSError) console.log(e.code); // "NoSuchPath"
}

const page = await lfs.commit.list("web/site", "agent", { page_size: 1 });
// page.items, page.has_more, page.next_page_pos
```

Not run:

```js
await lfs.sandbox.create("web/site", "tests", "native");
const { ws } = await lfs.workspace.mount("tests", "agent");
const run = await lfs.workspace.exec(ws, "ls", {
  stdout: (bytes) => process.stdout.write(bytes),
});
await lfs.workspace.commit(ws, { message: "ls output" });
await lfs.workspace.unmount(ws);
await lfs.sandbox.close("tests");
```

## 6. The same in Python

Run (not the Sandbox part):

```python
import asyncio
import layerfs
from layerfs import aio

lfs = layerfs.client(home="/path/to/home")
lfs.branch.fork("web/site", "py", "main")
lfs.file.write("web/site", "py", "/notes.md", "from python\n", message="notes")
print(lfs.file.read("web/site", "py", "/notes.md")["text"])
print(lfs.file.read("web/site", "py", "/notes.md", start_line=1, end_line=1)["end_line"])

try:
    lfs.store.get("nope")
except layerfs.LayerFSError as e:
    print(e.code, e.message)            # NotFound no Store named nope

async def main():
    page = await aio.client(home="/path/to/home").branch.list("web/site")
    print([b["name"] for b in page["items"]])

asyncio.run(main())
```

Not run:

```python
lfs.sandbox.create("web/site", "tests", "native")
ws = lfs.workspace.mount("tests", "py")["ws"]
print(lfs.workspace.exec(ws, "ls")["exit_code"])
lfs.workspace.commit(ws, message="ls")
lfs.workspace.unmount(ws)
lfs.sandbox.close("tests")
```

## Next

- [The calls](calls.md) for every argument and option.
- [Machines, Stores and backups](machines-and-stores.md) to move a Store to
  another machine, connect PostgreSQL or back up to a bucket.
