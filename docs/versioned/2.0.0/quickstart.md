# Quickstart

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](README.md). It walks through one home, a Store,
a project, Branch and file calls, a Sandbox with a Workspace, and a Commit,
in the shell, in JavaScript and in Python.

Every step below was run for this manual on macOS (arm64) against a private
scratch home with the debug build of branch `cs20-pkg-z` (from
`call-surface-2-0` at `61960e3f5f`), on 2026-11-19. Long ids are shortened to
8 characters and `…`, and the scratch folder to `/tmp/demo`.

## Before you start

- **The command.** `cargo +1.85.1 build -p layerfs-cli` builds `layerfs`
  into the worktree's `target/debug/`. `layerfs help` lists every call and
  `layerfs help <family> <call>` prints one.
- **Node.** The `@layerfs/node` package is in `api/node`. Its native addon is
  built with `npm --prefix api run build:native`.
- **Python.** The `layerfs` package is in `api/python`. Its module is built
  with `cd api/python && maturin build --locked` (add `--release` for the
  wheel to install).
  [api/README.md](../../../api/README.md#package-and-install) lists the
  steps to install the Node tarball, the wheel and the command outside the
  checkout.
- **A Sandbox** on the host machine needs the LayerFS daemon of the same
  build (see [step 4](#4-a-sandbox-a-workspace-and-a-commit)).

No package is published.

## 1. One home

The home is `LAYERFS_HOME`, or `~/.layerfs` when it is unset. Use a private
folder:

```bash
export LAYERFS_HOME="$HOME/lfs-demo"
```

```console
$ layerfs home get
{"home":"/tmp/demo/home","state":"broken","version":"2.0.0","reason":"missing","fix":"layerfs home init"}
$ layerfs home init
{"home":"/tmp/demo/home"}
$ layerfs home get
{"home":"/tmp/demo/home","state":"ready","version":"2.0.0","generation":2}
```

`home get` says what is wrong and the one command that fixes it. See
[the home](home.md).

## 2. A Store and a project (shell)

```console
$ layerfs store create shop
{"ok":true}
$ layerfs project init shop/api /tmp/demo/src --progress
{"phase":"check","done":null,"total":null}
{"phase":"install","done":null,"total":null}
{"phase":"record","done":null,"total":null}
{"commit":"32919f55…"}
$ layerfs project list
{"items":[{"project":"shop/api"}]}
$ layerfs project get shop/api --detail
{"store":"shop","state":"ready","commit":"32919f55…","op":null,"blocked_by":null,"reason":null,"fix":null,"project":"shop/api","engine":"sqlite","machine":"macos-host","created_at":"2026-11-18T22:04:02Z"}
```

- `store create` makes an empty sqlite Store on the host machine.
- `project init` imports the folder as project `api` of Store `shop` and
  answers main's first Commit. Init never creates a Store. `--progress`
  writes its phases to stderr.
- A project is always written `<store>/<project>`.
- Every answer is the short form: what an agent needs, with null, false and
  empty keys left out. `--detail` gives the long form, every key in a fixed
  order ([the form of an answer](calls.md#the-form-of-an-answer)).

## 3. Branches and files (shell)

```console
$ layerfs branch fork shop/api agent main
{"commit":"32919f55…"}
$ echo "# Shop API, agent" | layerfs file write shop/api agent /README.md -m "Update the title"
{"commit":"12c19c06…"}
$ echo "# x" | layerfs file write shop/api agent /README.md
{"commit":"123bef1a…"}
$ layerfs file read shop/api agent /README.md --text
# x
$ layerfs file list shop/api agent / --text
PATH        TYPE
/README.md  file
/docs       dir
$ layerfs file edit shop/api agent /docs/a.txt --old-string two --new-string three -m "edit a"
{"commit":"125470c8…"}
$ layerfs commit diff shop/api agent --with patch --path /docs/a.txt
{"items":[{"path":"/docs/a.txt","change":"modified","hunks":[{"op":" ","text":"one"},{"op":"-","text":"two"},{"op":"+","text":"three"},{"op":" ","text":""}]}]}
$ layerfs branch publish shop/api agent
{"commit":"32220c54…"}
$ layerfs commit list shop/api main
{"items":[{"commit":"32220c54…","source_branch":"agent"},{"commit":"32919f55…"}]}
```

- Each file write, edit, rm, mkdir, mv, cp and apply makes one Commit on the
  Branch. Without `-m`, the message is `Apply filesystem changes`.
- `file write` reads its content from stdin, or from `--from <file>`.
- A write to `main` is refused, with a `fix` that forks a Branch:
  `main` changes only through `branch publish`.
- `commit diff <project> <ref>` compares a Branch with its fork point, or a
  Commit with its parent; `--from R` names the other side.

## 4. A Sandbox, a Workspace and a Commit

```console
$ layerfs sandbox create shop/api tests --progress
{"phase":"start","done":0,"total":1}
{"phase":"start","done":1,"total":1}
{"ok":true}
$ layerfs workspace mount tests agent
{"workspace":"tests:agent","path":"/tmp/demo/home/sandbox/tests/workspaces/agent"}
$ layerfs workspace exec tests:agent 'echo hello > out.txt && uname -s'
{"exit_code":0,"stdout":"Darwin\n"}
$ layerfs workspace status tests:agent
{"changes":[{"path":"/out.txt","change":"added"},{"path":"/x.txt","change":"removed"}]}
$ layerfs workspace commit tests:agent -m "test output"
{"commit":"12bb5eae…"}
$ layerfs sandbox remove tests
{"error":"InUse","message":"Sandbox tests still holds 1 Workspace(s); commit or unmount them first","workspaces":["tests:agent"]}
$ layerfs workspace unmount tests:agent
{"ok":true}
$ layerfs sandbox remove tests
{"ok":true}
```

(`/x.txt` came from an earlier step of the same run that this page does not
show: `project hide-path` hid it after the Branch's head was written, and
the Workspace lists it as removed; see
[limitations](limitations.md#observed-in-the-runs-for-this-manual).)

- A Sandbox runs on the machine that holds its sqlite Store, or with
  `--machine M` on another machine for a PostgreSQL Store. A Store made by
  `store create` is on the host machine, so its Sandboxes run on the host machine.
- On macOS, `macos-host` runs the daemon as a host process with a loopback NFS
  mount. On Linux, `linux-host` uses namespaces and needs root.
- The host machine's daemon is its record's `daemon` (set by
  `layerfs machine update macos-host --daemon <path>`, as on the scratch home
  here), else `~/.cache/layerfs/bundle/<version>/<os>-<arch>/layerfs-daemon`.
  `layerfs home get --detail` shows which one this build would use.
- `workspace mount` answers the Workspace's handle, `<sandbox>:<name>`; its
  name is the Branch's unless `--name` gives one. Pass the handle to the
  other `workspace` calls. `main` is never mounted: fork a Branch.
- `workspace exec` answers `exit_code`, and `stdout` and `stderr` when not
  empty; the shell exits with the command's own code. With `--stream` the
  shell writes the output instead. Past 1 MiB a stream answers its last
  1 MiB and names the file that holds all of it.
- `workspace commit` saves the Workspace's changes to its Branch, merging
  first if the Branch moved since the mount; `--force` publishes over a
  moved head instead. Nothing commits or unmounts for you.
- `sandbox remove` refuses `InUse` while a Workspace is mounted; `--force`
  stops the Sandbox anyway and names the Workspaces it ended.
- A Sandbox cannot take a built-in machine name (`InvalidInput`).

## 5. The same in JavaScript

JavaScript:

```js
import layerfs, { LayerFSError } from "@layerfs/node";

const lfs = layerfs.client({ home: "/path/to/home" });
await lfs.store.create("web");
await lfs.project.init("web/site", "/path/to/site", { on_progress: (p) => console.log(p.phase) }); // check, install, record
await lfs.branch.fork("web/site", "agent", "main");
await lfs.file.write("web/site", "agent", "/notes.md", "# notes\n", { message: "add notes" });
const { text } = await lfs.file.read("web/site", "agent", "/notes.md"); // "# notes\n"

try {
  await lfs.file.read("web/site", "nope", "/README.md");
} catch (e) {
  if (e instanceof LayerFSError) console.log(e.error, e.message); // NotFound Branch nope does not exist
}

const page = await lfs.commit.list("web/site", "agent", { limit: 1 });
// {"items":[{"commit":"12799ab7…","message":"add notes"}]}: next_cursor only while there are more

await lfs.sandbox.create("web/site", "tests", { mount: "agent" });
// {"workspace":"tests:agent","path":"…/workspaces/agent"}
const run = await lfs.workspace.exec("tests:agent", "ls && echo built > out.txt");
// {"exit_code":0,"stdout":"README.md\nnotes.md\n"}
await lfs.workspace.commit("tests:agent", { message: "ls output" }); // {"commit":"12222172…"}
await lfs.workspace.unmount("tests:agent");                        // {"ok":true}
await lfs.sandbox.remove("tests");                                 // {"ok":true}
layerfs.close();
```

Run as one file on a fresh scratch home, with `machine.update("macos-host", {
daemon })` first (step 4). Longer runs are in [the examples](examples/README.md).

## 6. The same in Python

Run, after the JavaScript block, against the same home:

```python
import asyncio
import layerfs
from layerfs import aio

lfs = layerfs.client(home="/path/to/home")
lfs.branch.fork("web/site", "py", "main")
lfs.file.write("web/site", "py", "/notes.md", "from python\n", message="notes")
print(lfs.file.read("web/site", "py", "/notes.md")["text"])   # from python

try:
    lfs.store.get("nope")
except layerfs.LayerFSError as e:
    print(e.error, e.message)            # NotFound no Store named nope

lfs.sandbox.create("web/site", "tests")
ws = lfs.workspace.mount("tests", "py")["workspace"]   # "tests:py"
print(lfs.workspace.exec(ws, "ls")["exit_code"])       # 0
lfs.workspace.commit(ws, message="ls")  # {'commit': '12e9a2af…', 'unchanged': True}
lfs.workspace.unmount(ws)
lfs.sandbox.remove("tests")

async def main():
    page = await aio.client(home="/path/to/home").branch.list("web/site")
    print([b["branch"] for b in page["items"]])      # ['agent', 'main', 'py']

asyncio.run(main())
layerfs.close()
```

`ls` changed nothing, so the Commit answers `unchanged` with the Branch's
head. See also [`python_sync_flow.py`](examples/python_sync_flow.py) and
[`python_async_flow.py`](examples/python_async_flow.py).

## Next

- [The calls](calls.md) for every argument, option and answer.
- [Machines, Stores and backups](machines-and-stores.md) to move a Store to
  another machine, connect PostgreSQL or back up to a bucket.
- [Migrating](migration.md) from 1.1.0.
