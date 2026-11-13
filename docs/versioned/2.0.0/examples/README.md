# Examples: setup and mountless usage

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](../README.md). Four runnable files, one per front
end, that do the same mountless work: a Store, a project, files, Branches, a
diff, a merge, history and refusals. Each was **run from an installed
artifact in a clean directory outside the checkout**, and the output below is
what it printed. The Workspace route is written out at the
[end](#the-workspace-route-not-run) and was **not run**.

| File | Front end | Installed from |
| --- | --- | --- |
| [`node-flow.mjs`](node-flow.mjs) | Node | `layerfs-2.0.0.tgz` (`npm pack`, then `npm install`) |
| [`python_sync_flow.py`](python_sync_flow.py) | Python, synchronous | the wheel (`maturin build`, then `pip install`) |
| [`python_async_flow.py`](python_async_flow.py) | Python, `layerfs.aio` | the same wheel |
| [`cli-flow.sh`](cli-flow.sh) | the `layerfs` command | the one binary, copied alone into an empty directory |

Every file makes its own Disposable home and project folder in a temporary
directory and removes them. The temporary directory must be on a filesystem
with SQLite locks (ext4, xfs, btrfs; set `TMPDIR` if `/tmp` is not).

## Identity of the run

| | |
| --- | --- |
| Source | the tree pinned in the [interface report](../../../general/interface-report-2.0.0.md#pinned-identity): `layerfs_v2_0_0` `02133596a` plus PRs 586, 588 and 589 merged locally (`8363711d9`) |
| Host | Ubuntu 26.04.1 under WSL2 (Linux 6.6.87.2-microsoft-standard-WSL2), x86_64, glibc 2.43; the wheel is tagged `manylinux_2_39` |
| Tools | rustc 1.85.1, Node v24.22.2, npm 11.19.1, Python 3.14.4, maturin 1.16.0 |
| Build | `--release`, `--locked --offline`, `CARGO_BUILD_JOBS=3` |

| Artifact | sha256 |
| --- | --- |
| `layerfs-2.0.0.tgz` | `df19f1e8be0660e39cb443d38b5d8e12621ae857a0f2de730cdd78a109195040` |
| `layerfs-2.0.0-cp310-abi3-manylinux_2_39_x86_64.whl` | `6b2bbe395ef76f9497fc10051b2c2df49b872ba932b3a472f1294148b32b9029` |
| `layerfs` (command) | `85fdd85c1afbd04a56bb67be153dc9d7f6d1282c1fd68ba1d69b05e46114c59a` |

These hashes belong to this tree. The earlier builds in
[artifact-qualification-2.0.0.md](../../../general/artifact-qualification-2.0.0.md)
come from another tree and have other hashes. Nothing was published.

## Setup

From the repository root, with `$OUT` an empty directory
([api/README.md](../../../../api/README.md#package-and-install) has the same
steps):

```bash
npm --prefix api run build:native
(cd api/node && npm pack --pack-destination "$OUT")
(cd api/python && maturin build --release --locked -o "$OUT")
cargo +1.85.1 build --release --locked -p layerfs-cli
cp target/release/layerfs "$OUT/layerfs"
```

Then in clean directories outside the checkout, with `NODE_PATH`,
`PYTHONPATH`, `LAYERFS_NATIVE` and `LAYERFS_HOME` unset (Node installs 1
package; the only `layerfs.node` on disk is the one in `node_modules`):

```bash
# Node
npm init -y && npm install "$OUT/layerfs-2.0.0.tgz"
cp <checkout>/docs/versioned/2.0.0/examples/node-flow.mjs . && TMPDIR=<ext4 dir> node node-flow.mjs

# Python
python3 -m venv venv && ./venv/bin/pip install --no-index "$OUT"/layerfs-2.0.0-*.whl
TMPDIR=<ext4 dir> ./venv/bin/python python_sync_flow.py
TMPDIR=<ext4 dir> ./venv/bin/python python_async_flow.py

# the command alone
cp "$OUT/layerfs" . && ./layerfs --version        # layerfs 2.0.0
LAYERFS=./layerfs TMPDIR=<ext4 dir> bash cli-flow.sh
```

## Node

```console
$ TMPDIR=<ext4 dir> node node-flow.mjs
store.create                shop sqlite disposable
project.init phases         check install record
file.read                   "# notes\n"
file.list /                 README.md(file) docs(dir) notes.md(file)
file.read after edit        "one\nthree\n"
branch.merge                Merged
branch.diff main..merged    Modified /docs/a.txt, Added /right.txt
file.read merged            "uno\ntwo\n"
commit.list merged          merge left and right | left edits a
branch.list                 agent left main merged right
missing Branch              {"code":"NoSuchBranch","message":"Branch nope does not exist","detail":null}
write to main               {"code":"ReadOnly","message":"main changes only through publish","detail":null}
unknown option              {"code":"InvalidInput","message":"file.read: unknown option \"colour\"","detail":null}
scope: other call           {"code":"InvalidInput","message":"out of scope: store.list","detail":null}
scope: outside folder       {"code":"InvalidInput","message":"out of scope: path","detail":null}
```

Exit 0. Notes on what it shows:

- `project.init` printed three phase events for a small local project. Events
  mark the entry of a phase; the result is the only completion
  ([Events](../../../architecture/api/calls/errors-and-events.md#progress-events)).
- `main` is read-only for writes: it changes only through `branch.publish`.
  The example forks Branches instead.
- `commit.list merged` lists the Commits of the Branch `merged`: the merge
  Commit and the Commit of `left`.
- A refusal is `{code, message, detail}`; a bad option name is refused
  `InvalidInput` before any call. The last two refusals are a folder-scoped
  client: another call (`store.list`) and a path outside the scope's folder.

## Python

```console
$ TMPDIR=<ext4 dir> ./venv/bin/python python_sync_flow.py
store.create                shop sqlite disposable
project.init phases         check install record
file.read                   "# notes\n"
file.list /                 README.md(file) docs(dir) notes.md(file)
file.read after edit        "one\nthree\n"
branch.merge                Merged
branch.diff main..merged    Modified /docs/a.txt, Added /right.txt
file.read merged            "uno\ntwo\n"
commit.list merged          merge left and right | left edits a
branch.list                 agent left main merged right
missing Branch              {"code": "NoSuchBranch", "message": "Branch nope does not exist", "detail": null}
write to main               {"code": "ReadOnly", "message": "main changes only through publish", "detail": null}
unknown option              {"code": "InvalidInput", "message": "file.read: unknown option \"colour\"", "detail": null}
scope: other call           {"code": "InvalidInput", "message": "out of scope: store.list", "detail": null}
scope: outside folder       {"code": "InvalidInput", "message": "out of scope: path", "detail": null}
```

Exit 0. `python_async_flow.py` (the same calls as coroutines through
`layerfs.aio`) printed the same fifteen lines, byte for byte (compared with
`diff`), and exited 0. Cancelling an `aio` coroutine stops its callbacks and
not the native call; read the state again after a cancel
([Python](../../../architecture/api/front-ends/bindings.md#layerfs-python)).

## CLI

The command prints each result as one line of JSON, or a readable form with
`--text`. Exit 0 is done, 2 a wrong command line, 125 a refusal. In the output
below, hex ids are shortened to four digits and `…`, times are `…`, the
temporary directory is `<tmp>`, and runs of spaces in `--text` tables are
squeezed; nothing else is changed.

```console
$ LAYERFS=./layerfs TMPDIR=<ext4 dir> bash cli-flow.sh
$ layerfs store create shop
{"name":"shop","kind":"sqlite","machine":"native","profile":"disposable","created":…,"sqlite":"3.53.2","projects":[]}
[exit 0]
$ layerfs project init shop/site <tmp>/site --progress
{"progress":{"phase":"check","done":null,"total":null}}
{"progress":{"phase":"install","done":null,"total":null}}
{"progress":{"phase":"record","done":null,"total":null}}
{"project":"shop/site","store":"shop","name":"site","kind":"sqlite","machine":"native","created":…,"layer":"3271…","root":"be73…"}
[exit 0]
$ layerfs branch fork shop/site agent main
{"kind":"Forked","branch":"agent","base":"3271…","head":null}
[exit 0]
$ layerfs file write shop/site agent /notes.md -m 'add notes'
{"kind":"Committed","branch":"agent","head":"1253…","commit":{"id":"1253…","branch":"agent","parent":null,"second_parent":null,"base":"3271…","root":"6da5…","via":"fs","message":"add notes","merged_from":null,"time":…,"logical":23,"new_canonical":704,"new_stored":367,"new_metadata":322,"displaced":null,"lww":[],"lww_truncated":false}}
[exit 0]
$ layerfs file read shop/site agent /notes.md --text
# notes
[exit 0]
$ layerfs file list shop/site agent / --text
kind  ok
snapshot  agent@…
root  6da5…
path  /

NAME  PATH  TYPE  INODE
README.md  /README.md  file  2
docs  /docs  dir  3
notes.md  /notes.md  file  5
has_more  false
next_page_pos  -
[exit 0]
$ layerfs file edit shop/site agent /docs/a.txt --old two --new three -m 'edit a'
{"kind":"Committed","branch":"agent","head":"12d1…","commit":{"id":"12d1…","branch":"agent","parent":"1253…","second_parent":null,"base":"3271…","root":"a571…","via":"fs","message":"edit a","merged_from":null,"time":…,"logical":25,"new_canonical":611,"new_stored":297,"new_metadata":250,"displaced":null,"lww":[],"lww_truncated":false}}
[exit 0]
$ layerfs file read shop/site agent /docs/a.txt --text
one
three
[exit 0]
$ layerfs branch fork shop/site left main
{"kind":"Forked","branch":"left","base":"3271…","head":null}
[exit 0]
$ layerfs branch fork shop/site right main
{"kind":"Forked","branch":"right","base":"3271…","head":null}
[exit 0]
$ layerfs file edit shop/site left /docs/a.txt --old one --new uno -m 'left edits a'
{"kind":"Committed","branch":"left","head":"1242…","commit":{"id":"1242…","branch":"left","parent":null,"second_parent":null,"base":"3271…","root":"f420…","via":"fs","message":"left edits a","merged_from":null,"time":…,"logical":15,"new_canonical":528,"new_stored":283,"new_metadata":238,"displaced":null,"lww":[],"lww_truncated":false}}
[exit 0]
$ layerfs file write shop/site right /right.txt -m 'right adds a file'
{"kind":"Committed","branch":"right","head":"121a…","commit":{"id":"121a…","branch":"right","parent":null,"second_parent":null,"base":"3271…","root":"7d87…","via":"fs","message":"right adds a file","merged_from":null,"time":…,"logical":26,"new_canonical":708,"new_stored":372,"new_metadata":324,"displaced":null,"lww":[],"lww_truncated":false}}
[exit 0]
$ layerfs branch merge shop/site merged left right -m 'merge left and right'
{"kind":"Merged","fast_forward":false,"branch":"merged","head":"12f0…","root":"a2d3…","sources":[{"ref":"left","node":"1242…"},{"ref":"right","node":"121a…"}],"merge_base":"3271…","conflicts":[],"lww":[],"conflicts_truncated":false,"lww_truncated":false,"units":[],"units_truncated":false}
[exit 0]
$ layerfs branch diff shop/site main merged --paths-only --text
kind  ok
from  {"ref":"main","label":"main@…","root":"be73…"}
to  {"ref":"merged","label":"merged@…","root":"a2d3…"}

PATH  KIND  TYPE  CONTENT  IDENTITY  METADATA  HUNKS  HUNKS_OMITTED  BEFORE
/docs/a.txt  Modified  file  true  false  true  -  false  {"type":"file","inode":4}
/right.txt  Added  file  false  false  false  -  false  -
has_more  false
next_page_pos  -
next_cursor  -
[exit 0]
$ layerfs file read shop/site merged /docs/a.txt --text
uno
two
[exit 0]
$ layerfs commit list shop/site merged --text
ID  BRANCH  PARENT  SECOND_PARENT  BASE  ROOT  VIA  MESSAGE  MERGED_FROM  TIME  LOGICAL  NEW_CANONICAL  NEW_STORED  NEW_METADATA  DISPLACED  LWW_TRUNCATED
12f0…  merged  1242…  121a…  3271…  a2d3…  merge  merge left and right  -  …  26  578  250  250  -  false
1242…  merged  -  -  3271…  f420…  fs  left edits a  -  …  15  528  283  238  -  false
has_more  false
next_page_pos  -
[exit 0]
$ layerfs branch list shop/site --text
NAME  ID  BASE  HEAD  FORKED_FROM
agent  1106…  3271…  12d1…  3271…
left  1155…  3271…  1242…  3271…
main  -  3271…  -  -
merged  11ad…  3271…  12f0…  1242…
right  11c9…  3271…  121a…  3271…
has_more  false
next_page_pos  -
[exit 0]
$ layerfs file read shop/site nope /README.md
{"code":"NoSuchBranch","message":"Branch nope does not exist","detail":null}
[exit 125]
$ layerfs file write shop/site main /x.txt
{"code":"ReadOnly","message":"main changes only through publish","detail":null}
[exit 125]
$ layerfs file read shop/site agent /README.md --colour
layerfs: file.read: unknown option --colour
[exit 2]
$ layerfs file read shop/site agent
layerfs: file.read takes 3 to 4 positional arguments, not 2
[exit 2]
$ layerfs file list shop/site agent /docs --scope /docs --scope-project shop/site --scope-branch ns
{"code":"NoSuchBranch","message":"Branch ns.agent does not exist","detail":null}
[exit 125]
$ layerfs store list --scope /docs --scope-project shop/site --scope-branch ns
{"code":"InvalidInput","message":"out of scope: store.list","detail":null}
[exit 125]
```

Exit 0. What it shows beyond the packages:

- `--progress` writes each progress event to stderr as one JSON line before
  the result; the SDK functions receive the payload without the `progress`
  wrapper.
- A Commit is `id`, `branch`, `parent`, `second_parent`, `base`, `root`, `via`,
  `message`, `merged_from`, `time` and size counters. It has no author and no
  labels (see [history features that are gone](../migration.md#10-history-features-that-are-gone)).
- Under `--scope-branch ns` a Branch name is composed into the namespace, so
  `agent` is looked up as `ns.agent`.
- The unknown option `--colour` and a missing positional are exit 2, with one
  plain line on stderr and no call made; a refusal is exit 125 with the JSON on
  stderr.

## Not shown, and where it is

- **Backups and moving a Store** are not repeated here: see
  [Store push and pull](../machines-and-stores.md#sqlite-stores) and
  [backups](../machines-and-stores.md#backups).
- **`project.export`** ([#53](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/53))
  is not implemented: there is no such call in the table, and no example.
- **MCP** is shelved (O32); there is nothing to set up.
- **PostgreSQL** needs a build with `--features postgresql`; the default
  artifacts refuse it `Unsupported`. See
  [artifact-qualification-2.0.0.md](../../../general/artifact-qualification-2.0.0.md#postgresql-and-tls).

## The Workspace route (NOT RUN)

This is the documented route, written from the call table
([workspace family](../../../architecture/api/sdk/workspace.md)). **It
was not run.** A Workspace needs a Sandbox, and a Sandbox needs the LayerFS
daemon and the sealed Linux bundle. With the installed command in a
Disposable home on this host:

```console
$ layerfs sandbox create shop/site tests native
{"code":"Unavailable","message":"no LayerFS daemon at <home dir>/.cache/layerfs/bundle/2.0.0/linux-x86_64/layerfs-daemon; install the bundle","detail":null}
[exit 125]
```

An earlier run on this host with a built daemon was refused at
`unsupported runtime capability: user namespaces are not blocked`
(Events: what was run).
Nothing below is claimed to work.

```console
$ layerfs branch fork shop/site agent main
$ layerfs sandbox create shop/site tests native
$ layerfs workspace mount tests agent --name build         # answers ws "tests:build"
$ layerfs workspace exec tests:build 'make test' --stream
$ layerfs workspace status tests:build
$ layerfs workspace commit tests:build -m 'tests pass'     # one Commit; nothing commits for you
$ layerfs workspace unmount tests:build                    # Dirty while changes are uncommitted
$ layerfs sandbox close tests                              # MountsActive while a Workspace is mounted
```

```js
// Node
await layerfs.sandbox.create("shop/site", "tests", "native");
const { ws } = await layerfs.workspace.mount("tests", "agent", { name: "build" });
const run = await layerfs.workspace.exec(ws, "make test", { stdout: (b) => process.stdout.write(b) });
await layerfs.workspace.commit(ws, { message: "tests pass" });
await layerfs.workspace.unmount(ws);
await layerfs.sandbox.close("tests");
```

```python
# Python
layerfs.sandbox.create("shop/site", "tests", "native")
ws = layerfs.workspace.mount("tests", "agent", name="build")["ws"]
run = layerfs.workspace.exec(ws, "make test", stream=True)
layerfs.workspace.commit(ws, message="tests pass")
layerfs.workspace.unmount(ws)
layerfs.sandbox.close("tests")
```

`exec` output arrives after the command ends, never live, and its delivery is
unverified ([Events](../../../architecture/api/calls/errors-and-events.md#workspaceexec-output)).
There is no implicit Commit and no implicit unmount.
