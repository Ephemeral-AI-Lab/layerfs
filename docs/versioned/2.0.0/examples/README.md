# Examples: the mountless flow and a Workspace

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](../README.md). Four runnable files, one per front
end, that do the same mountless work with the 2.0 call table: a Store, a
project, files, Branches, a diff, a merge into a new Branch, a publication,
history and refusals. Each was **run for this manual** and the output below
is what it printed. The [Workspace route](#the-workspace-route) at the end
was run too, in Node and Python (the shell run is in
[the calls](../calls.md#workspace-7)).

| File | Front end |
| --- | --- |
| [`cli-flow.sh`](cli-flow.sh) | the `layerfs` command |
| [`node-flow.mjs`](node-flow.mjs) | Node, `@layerfs/node` |
| [`python_sync_flow.py`](python_sync_flow.py) | Python, synchronous |
| [`python_async_flow.py`](python_async_flow.py) | Python, `layerfs.aio` |

Every file makes its own Disposable home and project folder in a temporary
directory and removes them. On Linux the temporary directory must be on a
filesystem with SQLite locks (ext4, xfs, btrfs; set `TMPDIR` if `/tmp` is
not).

## Identity of the run

| | |
| --- | --- |
| Source | branch `cs20-pkg-z` from `call-surface-2-0` at `61960e3f5f` (spec v19 with the conformance fixes); no product change after it |
| Host | macOS 26.4.1, arm64 |
| Tools | rustc 1.85.1, Node v24.21.0, Python 3.14.3 |
| Build | debug, through `tools/worktree.py cargo` (`-p layerfs-cli`, `-p layerfs-node -p layerfs-python`, `-p layerfs-daemon`) |

Nothing was packed, installed or published. The Node addon was
`target/debug/liblayerfs_node.dylib` copied to `api/node/layerfs.node`, the
Python module `target/debug/lib_native.dylib` copied to
`api/python/src/layerfs/_native.abi3.so` (both ignored by Git).
[api/README.md](../../../../api/README.md#package-and-install) has the steps
that build the release artifacts and install them outside the checkout.

```bash
# the command
LAYERFS=target/debug/layerfs TMPDIR=<scratch dir> bash docs/versioned/2.0.0/examples/cli-flow.sh
# Node: from a scratch folder whose node_modules/@layerfs/node links to api/node
cp docs/versioned/2.0.0/examples/node-flow.mjs <scratch>/ && (cd <scratch> && TMPDIR=<scratch dir> node node-flow.mjs)
# Python
PYTHONPATH=api/python/src TMPDIR=<scratch dir> python3 docs/versioned/2.0.0/examples/python_sync_flow.py
PYTHONPATH=api/python/src TMPDIR=<scratch dir> python3 docs/versioned/2.0.0/examples/python_async_flow.py
```

## The command

Exit 0 (ids shortened to 8 characters and `…`, the temporary folder to
`/tmp/layerfs-guide-XXXXXX`):

```console
$ layerfs store create shop
{"ok":true}
[exit 0]
$ layerfs project init shop/site /tmp/layerfs-guide-XXXXXX/site --progress
{"phase":"check","done":null,"total":null}
{"phase":"install","done":null,"total":null}
{"phase":"record","done":null,"total":null}
{"commit":"328cf399…"}
[exit 0]
$ layerfs branch fork shop/site agent main
{"commit":"328cf399…"}
[exit 0]
$ layerfs file write shop/site agent /notes.md -m 'add notes'
{"commit":"12876f06…"}
[exit 0]
$ layerfs file read shop/site agent /notes.md --text
# notes
[exit 0]
$ layerfs file list shop/site agent / --text
PATH        TYPE
/README.md  file
/docs       dir
/notes.md   file
[exit 0]
$ layerfs file edit shop/site agent /docs/a.txt --old-string two --new-string three -m 'edit a'
{"commit":"129313f0…"}
[exit 0]
$ layerfs file read shop/site agent /docs/a.txt --text
one
three
[exit 0]
$ layerfs branch fork shop/site left main
{"commit":"328cf399…"}
[exit 0]
$ layerfs branch fork shop/site right main
{"commit":"328cf399…"}
[exit 0]
$ layerfs file edit shop/site left /docs/a.txt --old-string one --new-string uno -m 'left edits a'
{"commit":"12302927…"}
[exit 0]
$ layerfs file write shop/site right /right.txt -m 'right adds a file'
{"commit":"12d58474…"}
[exit 0]
$ layerfs branch merge shop/site left right --into merged -m 'merge left and right'
{"commit":"12603974…"}
[exit 0]
$ layerfs commit diff shop/site merged --from main --text
PATH         CHANGE
/docs/a.txt  modified
/right.txt   added
[exit 0]
$ layerfs file read shop/site merged /docs/a.txt --text
uno
two
[exit 0]
$ layerfs commit list shop/site merged --text
COMMIT                                                              MESSAGE
12603974…  merge left and right
12302927…  left edits a
[exit 0]
$ layerfs branch list shop/site --text
BRANCH  COMMIT
agent   129313f0…
left    12302927…
main    328cf399…
merged  12603974…
right   12d58474…
[exit 0]
$ layerfs branch publish shop/site merged
{"commit":"3290100c…"}
[exit 0]
$ layerfs commit list shop/site main
{"items":[{"commit":"3290100c…","source_branch":"merged"},{"commit":"328cf399…"}]}
[exit 0]
$ layerfs file read shop/site nope /README.md
{"error":"NotFound","message":"Branch nope does not exist"}
[exit 125]
$ layerfs file write shop/site main /x.txt
{"error":"InvalidInput","message":"main changes only through publish","fix":"layerfs branch fork shop/site work"}
[exit 125]
$ layerfs file read shop/site agent /README.md --colour
layerfs: file.read: unknown option --colour
accepted options: --text, --json, --detail, --at, --line, --limit
usage: layerfs file read <project> <branch> <path> [--at C] [--line 1] [--limit 2000]
`layerfs help file read` for more
[exit 2]
$ layerfs file read shop/site agent
layerfs: file read is missing <path>
usage: layerfs file read <project> <branch> <path> [--at C] [--line 1] [--limit 2000]
`layerfs help file read` for more
[exit 2]
$ layerfs file list shop/site agent /docs --scope /docs --scope-project shop/site --scope-branch ns
{"error":"NotFound","message":"Branch ns.agent does not exist"}
[exit 125]
$ layerfs store list --scope /docs --scope-project shop/site --scope-branch ns
{"error":"InvalidInput","message":"out of scope: store.list"}
[exit 125]
```

- `--progress` writes each event to stderr as one JSON line before the
  result.
- `branch merge left right --into merged` writes the merge to a new Branch
  and leaves `left` as it is; `commit list` of it follows first parents.
- A refusal is exit 125 with `{error, message}` and, when LayerFS knows
  one, a `fix` line, on stderr. The unknown option `--colour` and a missing
  positional are exit 2 with a plain line on stderr, and no call is made.
- Under `--scope-branch ns` a Branch name is composed into the namespace, so
  `agent` is looked up as `ns.agent`.

## Node

Exit 0:

```console
store.get                   {"engine":"sqlite","state":"ready","machine":"native"}
project.init phases         check install record
file.read                   "# notes\n"
file.list /                 /README.md(file) /docs(dir) /notes.md(file)
file.read after edit        "one\nthree\n"
branch.merge                merged is at the merge Commit
commit.diff main..merged    modified /docs/a.txt, added /right.txt
file.read merged            "uno\ntwo\n"
commit.list merged          merge left and right | left edits a
branch.list                 agent left main merged right
commit.list main --limit 1  {"source_branch":"merged","more":true}
missing Branch              {"error":"NotFound","message":"Branch nope does not exist","fix":null}
write to main               {"error":"InvalidInput","message":"main changes only through publish","fix":"layerfs branch fork shop/site work"}
unknown option              {"error":"InvalidInput","message":"file.read: unknown option \"colour\"","fix":null}
scope: other call           {"error":"InvalidInput","message":"out of scope: store.list","fix":null}
scope: outside folder       {"error":"InvalidInput","message":"out of scope: path","fix":null}
```

## Python

`python_sync_flow.py` and `python_async_flow.py` each printed the same lines
as Node, with Python's JSON spacing, and exited 0. The first and the
refusal lines:

```console
store.get                   {"engine": "sqlite", "state": "ready", "machine": "native"}
project.init phases         check install record
missing Branch              {"error": "NotFound", "message": "Branch nope does not exist", "fix": null}
write to main               {"error": "InvalidInput", "message": "main changes only through publish", "fix": "layerfs branch fork shop/site work"}
unknown option              {"error": "InvalidInput", "message": "file.read: unknown option \"colour\"", "fix": null}
scope: other call           {"error": "InvalidInput", "message": "out of scope: store.list", "fix": null}
scope: outside folder       {"error": "InvalidInput", "message": "out of scope: path", "fix": null}
```

A refusal raises `layerfs.LayerFSError` (Node: `LayerFSError`); `fix` is
`None` (`null`) when there is none. In Node and Python an unknown option is
a refusal (`InvalidInput`), not exit 2.

## The Workspace route

A Workspace needs a Sandbox, and a Sandbox on `native` needs a daemon of
this build. On the scratch homes it was `target/debug/layerfs-daemon`, set
with `layerfs machine update native --daemon <path>` (Node:
`machine.update("native", { daemon })`); an installed bundle puts it where
`layerfs home get --detail` shows `daemon.path`. Node, run (paths and ids
shortened):

```js
await lfs.sandbox.create("web/site", "tests", { mount: "agent" });
// {"workspace":"tests:agent","path":"…/machine/native/run/layerfs-260d8ebe-tests/workspaces/1"}
const run = await lfs.workspace.exec("tests:agent", "ls && echo built > out.txt", {
  on_progress: (p) => console.log(p), // {"phase":"start","done":0,"total":1,"exec":"exec-52e9eb931534ce95"}
});
// {"exit_code":0,"stdout":"README.md\n"}
await lfs.workspace.status("tests:agent");                         // {"changes":[{"path":"/out.txt","change":"added"}]}
await lfs.workspace.commit("tests:agent", { message: "ls output" }); // {"commit":"12a504d8…"}
await lfs.workspace.unmount("tests:agent");                        // {"ok":true}
await lfs.sandbox.remove("tests");                                 // {"ok":true}
```

Python, run:

```python
lfs.sandbox.create("web/site", "tests")                # {"ok": true}
ws = lfs.workspace.mount("tests", "py")["workspace"]   # "tests:py"
lfs.workspace.exec(ws, "ls")                           # {"exit_code": 0, "stdout": "README.md\n"}
lfs.workspace.commit(ws, message="ls")                 # {"unchanged": true}, see below
lfs.workspace.unmount(ws)                              # {"ok": true}
lfs.sandbox.remove("tests")                            # {"ok": true}
```

Nothing commits or unmounts for you. `workspace.commit` with nothing to
commit, on a Branch that has no Commit of its own, answered
`{"unchanged": true}` with `commit` null, where its row names the head (see
[limitations](../limitations.md#observed-in-the-runs-for-this-manual)).

## Not shown, and where it is

- **Moving a Store and backups**: [Stores](../machines-and-stores.md#stores)
  and [backups](../machines-and-stores.md#backups).
- **`project export`**: [the calls](../calls.md#project-8).
- **MCP**: [the MCP front end](../../../architecture/api/front-ends/mcp.md).
