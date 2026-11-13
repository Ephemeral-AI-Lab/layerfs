# LayerFS 2.0.0 interface report

> **Status:** Current general guide; a dated evidence record, not a release receipt. Nothing here was published.

What the 2.0.0 interface offers (the call table and its three front ends: Node,
Python, the `layerfs` command), what changed from the earlier surfaces, what is
missing, and what was and was not run. It closes the interface review of issue
[#40](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/40) (parent
[#30](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/30)). Old pull
request receipts and functional benchmarks do not qualify the current
interface; only what is linked below does. MCP is shelved (O32) and is not part
of this report.

## Pinned identity

| | |
| --- | --- |
| Base | `layerfs_v2_0_0` at `02133596a` (merge of PR #584). It contains the four interface PRs #573 (scope), #574 (cli), #575 (python) and #576 (node) |
| Open PRs merged on top | #586 coverage matrix and cross-front-end checks (branch head `0cf2a3784`), #588 progress, event and exec-output page (`73e9fc9cc`), #589 artifact qualification (`1a1194f52`), in that order |
| Merge head | `8363711d9`: a local merge, never pushed. The one conflict was the link table of `call-table/README.md` (the coverage row and the events row were both kept). Hashes of the three branch heads are local; the PR numbers are the stable names |
| Added by this report | the migration guide, the examples and this page (documents and runnable files only; no Rust, Node or Python source) |
| Host | Ubuntu 26.04.1 under WSL2 (Linux 6.6.87.2), x86_64, glibc 2.43 |
| Tools | rustc 1.85.1, Node v24.22.2, npm 11.19.1, Python 3.14.4, maturin 1.16.0; `--release --locked --offline`, `CARGO_BUILD_JOBS=3` |

Artifacts built from the merge head (the hashes belong to this tree; the ones in
[artifact-qualification-2.0.0.md](artifact-qualification-2.0.0.md) belong to the
tree of PR #589):

| Artifact | sha256 |
| --- | --- |
| `layerfs-2.0.0.tgz` | `df19f1e8be0660e39cb443d38b5d8e12621ae857a0f2de730cdd78a109195040` |
| `layerfs-2.0.0-cp310-abi3-manylinux_2_39_x86_64.whl` | `6b2bbe395ef76f9497fc10051b2c2df49b872ba932b3a472f1294148b32b9029` |
| `layerfs` command | `85fdd85c1afbd04a56bb67be153dc9d7f6d1282c1fd68ba1d69b05e46114c59a` |

## Capability matrix

**implemented**: in the source and the docs, with the evidence linked;
**changed**: the old capability exists in another shape; **retired**: removed
on purpose by a recorded decision; **missing**: no 2.0 equivalent. "Run" says
what was run for this report.

| Capability | Status | Evidence in the repository | Run here |
| --- | --- | --- | --- |
| One call table: 61 calls in nine families, generated into Node, Python (sync and `aio`) and the command | implemented | [coverage.md](../architecture/api/calls/coverage.md) (generated matrix, `gen-coverage.mjs --check`), [table.md](../architecture/api/calls/README.md) | `gen-coverage.mjs --check` exit 0; `node --test test/coverage.mjs test/contract.mjs`: 14 tests, 13 pass, 0 fail, 1 skipped (the `Unknown`/`Unresolved` case, NOT RUN) |
| Same answers and refusals in all front ends (110 steps, fresh Disposable homes) | implemented | [coverage.md: what is checked](../architecture/api/calls/coverage.md#what-is-checked-across-the-front-ends) | the run above |
| Mountless file and history work (write, read, list, edit, fork, diff, merge, `commit.list`) | implemented | [examples](../versioned/2.0.0/examples/README.md), [calls](../versioned/2.0.0/calls.md) | the four example flows from installed artifacts, all exit 0 |
| Installed from artifacts outside the checkout | implemented (Linux x64 only) | [artifact-qualification-2.0.0.md](artifact-qualification-2.0.0.md), [api/README.md](../../api/README.md#package-and-install) | tarball, wheel and the lone binary installed and used |
| Node package `layerfs`: ES module, Node 22, MIT, typings | changed (was `@layerfs/agent`) | [node.md](../architecture/api/front-ends/bindings.md#layerfs-node) | installed from the tarball |
| Python package `layerfs`, sync and `layerfs.aio`, stubs and `py.typed` | changed (new; no earlier release had one) | [python.md](../architecture/api/front-ends/bindings.md#layerfs-python) | installed from the wheel; sync and `aio` flows |
| Native `layerfs` command | changed (was the Node `@layerfs/cli`) | [cli.md](../architecture/api/front-ends/README.md#the-layerfs-command) | used alone; exits 0, 2 and 125 seen |
| Exact large integers in Node (`bigint`), Python `int` exact | implemented | [node.md: Values](../architecture/api/front-ends/bindings.md#layerfs-node), [coverage.md](../architecture/api/calls/coverage.md) (`ints` group) | in the contract run above; not run: a result beyond 2^53 |
| Bytes as `{ "base64": ... }`, text only for `file.read` | changed | [node.md](../architecture/api/front-ends/bindings.md#layerfs-node), [python.md](../architecture/api/front-ends/bindings.md#layerfs-python), [errors.md](../architecture/api/calls/errors-and-events.md) (`Binary`) | `values` group of the contract run |
| Folder scope `{ project, folder, branch? }` | changed, with four open gaps | [table.md: Scopes](../architecture/api/calls/README.md#folder-scope), [calls](../versioned/2.0.0/calls.md#folder-scope), `layerfs-api/tests/scope_references.rs` | two refusals on a scoped client in each example; not run: scope across all front ends |
| Errors as `{ code, message, detail }`; `Unknown` and `Unresolved` without replay | implemented | [errors.md](../architecture/api/calls/errors-and-events.md) | refusals in the examples; not run: a real `Unknown` |
| Progress (`on_progress`, `--progress`) on five calls | changed (one stream, no job registry, O22) | [events.md](../architecture/api/calls/errors-and-events.md#progress-events) | `project.init` events in the examples |
| `workspace.exec` output | changed (buffered, delivered after the command) | [events.md](../architecture/api/calls/errors-and-events.md#workspaceexec-output) | **NOT RUN**, UNVERIFIED |
| `layerfs doctor` / `doctor()` / `close()` | implemented (new) | [doctor.md](../architecture/control/doctor/README.md) | doctor on a clean home, on a broken record and on a relative home; exit 0, 0, 125 |
| Workspace flow: mount, exec, commit, unmount | implemented in source | [workspace family](../architecture/api/sdk/workspace.md), [quickstart](../versioned/2.0.0/quickstart.md#4-a-sandbox-a-workspace-and-a-commit) | **NOT RUN**: `sandbox create` refused `Unavailable` (no daemon bundle) |
| Backups and Store push and pull | implemented elsewhere | [machines and Stores](../versioned/2.0.0/machines-and-stores.md), the backup and transfer campaigns | not re-run here |
| Historical reads at a Commit or Layer id, paging to 1000 | changed (an optional `commit` positional; `page_size` 1 to 1000) | [`file.read`](../architecture/api/sdk/file.md#fileread), [`branch.list`](../architecture/api/sdk/branch.md#branchlist) | page and window cases in the contract run; a live read at a Commit id was not run |
| MCP server, 50 tools | retired (O32) | [coverage.md](../architecture/api/calls/coverage.md#outside-the-table), [limitations](../versioned/2.0.0/limitations.md) | n/a |
| `@layerfs/backend` operator facade, background processes | retired (O25) | [limitations](../versioned/2.0.0/limitations.md#not-built-refused-as-unsupported) | n/a |
| Event subscription and the trace feed | retired (O26); the feed is not delivered | [home and records](../architecture/control/home/README.md#traces-and-the-call-log) | n/a |
| `branch.graph`, `changed_paths`, commit author and labels, per-call results, project summary, binary `file.read` and offset | missing, **not restored** (owner decision ([#31](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/31#issuecomment-6657201351)): follow the project baseline) | [migration: history features](../versioned/2.0.0/migration.md#10-history-features-that-are-gone) | n/a |
| `project.export` | missing | not in the table; [#53](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/53) | n/a |

## Compatibility table

Old is 1.1.0 or the unmerged PR #521; the full row-by-row mapping, with the
reason and a pointer for every row, is the
[migration guide](../versioned/2.0.0/migration.md).

| Area | Old | 2.0.0 | Breaking |
| --- | --- | --- | --- |
| Packages | `@layerfs/agent`, `@layerfs/cli`, `@layerfs/backend`, `@layerfs/runtime`, `@layerfs/mcp` | npm `layerfs`, PyPI `layerfs`, native `layerfs` (none published) | yes |
| Entry points | `LayerFS.open`, `LayerFS.init`, `client.call` | `client({ home, scope })`, `store.create` and `project.init`; no generic call | yes |
| Project naming | one implicit project per client | `"<store>/<project>"` in every call | yes |
| Method names and argument order | `fs.*`, `branch.list_commits`, `branch.merge(a, b, name)` and others | `file.*`, `commit.list`, `branch.merge(project, name, a, b)` and others | yes |
| Options | positionals only | positionals, then an options object or keywords | yes |
| Result keys | camelCase | snake_case | yes |
| Large integers (Node) | `bigint` sent as a `Number` | exact; unsafe `number` refused | yes, for code that relied on rounding |
| Bytes | `{ "$layerfs": "bytes", "base64" }` (#521, MCP) | `{ "base64" }`, key reserved | yes (#521 only) |
| Folder scope | `{ folder, branch? }`, `sandbox.*` allowed | `{ project, folder, branch? }`, `sandbox.*` refused, `exec` is a `cd` prefix | yes |
| Sandbox | numbered, project implicit | named, bound to one project and a machine | yes |
| Workspace `mount` of `main` | allowed | refused; fork a Branch | yes |
| Progress and events | event streams, job registry, trace feed | `on_progress` on five calls | yes |
| Settings | the 1.1.0 platform settings | `LAYERFS_HOME`, records, `layerfs import-env` once | yes |
| Homes | 1.1.0 layout | converted once on the first 2.0 call | yes (one way) |
| Store profile | Disposable | Disposable; Durable disabled | no |
| Attempts | one attempt, no replay | one attempt, no replay | no |
| MCP | `@layerfs/mcp` (#521) | none | yes (#521 only) |
| Plugins in `plugins/` | `@layerfs/agent` and others | not migrated | yes |

## Known gaps

Every gap has an owner. None is fixed by this report.

| Gap | Evidence | Owner |
| --- | --- | --- |
| **Scope** leaves four things unconfined and undecided: Commit and Layer ids, an id in `branch`, the Branch of a Workspace (`workspace.commit` can write outside the namespace), and a scope without `branch` | [table.md: Scopes](../architecture/api/calls/README.md#folder-scope) | [#33](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/33) |
| **CLI exit codes**: `workspace exec --stream` exits with the command's code, so 2 and 125 look like a usage error or a refusal; `doctor` exits 0 with `"ok": false` (by design); a closed stdout while printing is 125 | [cli.md](../architecture/api/front-ends/README.md#limits-and-failures), [Streams](../architecture/api/front-ends/README.md#the-layerfs-command) | [#36](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/36) |
| **`file.apply` with `[]`** commits nothing and answers `UpToDate` while the page says a nonempty list; **`file.rm`** of a missing path answers the message `Content(PathNotFound)`; engine refusals name arguments in **camelCase** (`startLine`, `pageSize`) | [coverage.md: known gaps](../architecture/api/calls/coverage.md#what-is-checked-across-the-front-ends) | [#32](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/32) |
| **Result schemas**: the table has argument kinds and no result schema, so typings answer a loose `Result` | [node.md](../architecture/api/front-ends/bindings.md#layerfs-node), [python.md](../architecture/api/front-ends/bindings.md#layerfs-python) | [#34](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/34), [#35](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/35) |
| **`workspace.exec` output is UNVERIFIED** (no Sandbox could be started) and **seven decisions are open**: the shared payload for counters, a throttle, a finalize phase, the dead `machine.create` event, the unbounded Node queue, an exec failure after the command ran being `Failed`, an `async` progress function that rejects ending the process | [errors-and-events.md: progress events](../architecture/api/calls/errors-and-events.md#progress-events) | [#37](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/37) |
| **PostgreSQL and TLS** are not in the default artifacts; the refusal says "build it with --features postgresql", which an npm or pip user cannot act on. Whether published artifacts carry them is an owner decision | [artifact-qualification: PostgreSQL and TLS](artifact-qualification-2.0.0.md#postgresql-and-tls) | [#39](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/39) |
| **`firecracker` and `ssh`**: disabled by product policy (layerfs-project#7, implemented in `b919f50ea` / #577, which this base contains). On a command built from this base `machine platforms` reports both `ready: false` with that reason, and `machine create` / `machine connect` answer `Unsupported` (exit 125) with the machine list and the home unchanged. An earlier draft of this report called them still selectable; that came from artifacts built before #577 and was wrong for this base | [machines-and-stores](../versioned/2.0.0/machines-and-stores.md), [limitations](../versioned/2.0.0/limitations.md) | [#7](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/7) (closed) |
| **GLIBC 2.39**: every Linux artifact built here needs it, so a host with an older glibc (Ubuntu 22.04, Debian 12) cannot run them; release builds should come from the oldest supported host | [artifact-qualification](artifact-qualification-2.0.0.md#artifacts) | [#39](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/39) |
| **Node >=22** (`engines`, because exact integers use `JSON.rawJSON`) against the Node of the Sandbox runtime image: not checked here | [node.md](../architecture/api/front-ends/bindings.md#layerfs-node) | [#39](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/39) |
| **No publication runbook**, no per-platform prebuilds, no macOS or aarch64 artifacts; CI excludes the Node, JS and Python packages (owner decision 2026-10-23); Python without its native module fails with a circular-import message | [artifact-qualification: gaps](artifact-qualification-2.0.0.md#gaps-and-owner-decisions) | [#39](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/39) |
| **MCP is shelved** (O32 stands, owner decision ([#31](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/31#issuecomment-6657201351))): no server, no packaging, no examples | [coverage.md](../architecture/api/calls/coverage.md#outside-the-table) | [#38](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/38) |
| **`project.export`** is not implemented | [#53](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/53) | [#53](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/53) |
| **History features not restored** (owner decision ([#31](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/31#issuecomment-6657201351))) | [migration](../versioned/2.0.0/migration.md#10-history-features-that-are-gone) | this report |
| **`commit.get` costs the length of the history**; the trace feed that O26 promised is not delivered | [limitations](../versioned/2.0.0/limitations.md#not-built-refused-as-unsupported), [home and records](../architecture/control/home/README.md#traces-and-the-call-log) | [#37](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/37) |
| **Plugins** in `plugins/` still describe the 1.1.0 packages | `grep -l "@layerfs/agent" plugins/*/README.md` | no issue named |
| **Base fails `clippy -D warnings`**: `unused import: ORDINAL_END` at `engine/crates/layerfs-persistence/src/backend/mod.rs:10` (verified on the merge head, see Checks) | below | no issue named; one import in `layerfs-persistence` |
| **Three tests fail on the base** in CI: `layerfs-api` `tests/workspace_names.rs` (`a_name_is_unique_in_its_sandbox`, `every_workspace_call_takes_the_name_or_the_path`, both need a built daemon) and `layerfs-persistence` `tests/sqlite_publication.rs:63` (`wal_full_settings_and_unavailable_engine_are_explicit`). Recorded for baseline `d4304741` in the CI note of the interface PRs; **not re-run here** | the interface review of #30 (a CI run of the four PRs, reproduced on the unmodified baseline); not in the repository | no issue named |
| **The `tools/` unit test `test_decode_observations.py::test_every_named_section_matches_its_source_width` fails on the base** (`AssertionError: 58 != 55 : 13`; the `fmt and guards` CI job runs `python3 -B -m unittest discover -s tools` and goes red; reproduced on a tree whose `tools/` is untouched; the decoder's source was last changed by `68c24f530`, "count PostgreSQL round trips, prepares and connects") | below | no issue named; owner of that change |

## NOT RUN

Across the whole effort, recorded as not run so that nobody reads their absence
as a pass:

- macOS (native arm64, loopback NFS), Lima, Apple Container, Microsandbox,
  Firecracker, Windows, Linux aarch64.
- The Docker machine route, and every Sandbox, Workspace and daemon-backed call.
  On this host the native Sandbox was refused: first
  `/root is not traversable by others (o+x)`, then, with a built daemon and a
  home under `/var/tmp`, `unsupported runtime capability: user namespaces are
  not blocked`; with the installed command and no daemon bundle,
  `Unavailable`. Changing `user.max_user_namespaces` is a change to the shared
  VM and was not made.
- PostgreSQL: run only for the opt-in build, in the artifact report (create,
  Init, fork, write, read, `commit.list`, through the installed addon, wheel
  and command). Not run through the contract cases, scope, progress or Sandbox,
  and the default artifacts refuse it.
- The `tls` build and any https bucket; R2 backups; backup progress.
- Python 3.10 to 3.13 (3.14 only), Node 22 and 20 (24 only), hosts with glibc
  below 2.39.
- A real `Unknown` or `Unresolved` outcome; folder scope across all front ends;
  queue flooding, a full disk during exec, a slow consumer.
- `workspace.exec` output delivery (over and under 1 MiB, with and without a
  sink, stdout/stderr order).
- CI: unchanged and does not cover the Node and Python packages.
- `project.export`, MCP (shelved).

## Checks run for this report

Linux x64 under WSL2, on the merge head with the three artifacts built from it.

| Check | Result |
| --- | --- |
| `node api/scripts/gen-coverage.mjs --check` | exit 0 |
| `node --test test/coverage.mjs test/contract.mjs` in `api/node` (release addon, release command, the installed wheel's Python) | 14 tests: 13 pass, 0 fail, 1 skipped (`Unknown`/`Unresolved`, NOT RUN) |
| example flows from installed artifacts (Node, Python sync, Python async, command) | each exit 0; output in [examples](../versioned/2.0.0/examples/README.md) |
| `cargo +1.85.1 clippy --locked --offline -p layerfs-persistence -- -D warnings` | **fails**: `unused import: ORDINAL_END`, `backend/mod.rs:10`, exit 101. The base fails, not this report |
| relative-link check of the four new or changed documents, files and #anchors (a small script kept outside the repository) | 167 links, 0 broken |
| `python3 -B tools/check_product_boundary.py` | PASS: scanned 1244 production Rust/SQL/JavaScript files |

## Owner decisions

Taken (2026-11-05, recorded in the [#31](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/31#issuecomment-6657201351) decision record, section 0): O32
stands and MCP stays shelved; the history features follow the project baseline
and are not restored; Node integers are exact; the package licence follows the
repository (MIT).

Open: the scope gaps (#33); the exit-code policy (#36); what to do about
`file.apply []`, the `file.rm` message and the camelCase refusals (#32); the
seven exec and progress decisions (#37); PostgreSQL and TLS in published
artifacts, the glibc floor and the publication policy (#39); the destination and counters of `project.export`
(#53).

---

← [General guides](../README.md) · [2.0.0 manual](../versioned/2.0.0/README.md)
