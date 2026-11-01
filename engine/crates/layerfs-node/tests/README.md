# Native binding tests

> **Status:** Current test guide.

These Linux tests load the ordinary built addon in Node and call its exported
`call` and `backendCall` methods. They never include private binding source or
add a test-only product API. Build `layerfs-node` first, then compile the test
targets with `--no-run` before running them under an explicit wall timeout:

```sh
cargo +1.85.1 build --manifest-path Cargo.toml --locked -p layerfs-node
cargo +1.85.1 test --manifest-path Cargo.toml --locked -p layerfs-node --tests --no-run
timeout 30 cargo +1.85.1 test --manifest-path Cargo.toml --locked -p layerfs-node --test native_discovery
timeout 90 cargo +1.85.1 test --manifest-path Cargo.toml --locked -p layerfs-node --test native_admission
timeout 100 cargo +1.85.1 test --manifest-path Cargo.toml --locked -p layerfs-node --test native_connection_wait
timeout 60 cargo +1.85.1 test --manifest-path Cargo.toml --locked -p layerfs-node --test native_local
```

Use the worktree's registered Cargo resources for these commands. Node 20 or
later must be available as `node`, or selected by `LAYERFS_TEST_NODE`. The fork
fixture also needs `cc`. Tests locate the addon beside the test executables and
copy it to a private temporary directory with the `.node` suffix Node requires.

`native_discovery` covers gone and stopped containers, exact refusal custody,
number replacement during inspection, and discovery-lock release while a forked
child retains the descriptor. Its C fixture interposes `flock` only in the child
Node process; the fixture's fork is bounded and released on test unwinding.

`native_local` covers native Sandbox records (`LAYERFS_SANDBOX=native`) through
`runtime.sandbox.reclaim`: a record naming any directory other than where its
deployment puts that Sandbox is refused and nothing is deleted, the configured
`localDir` must be absolute and plain, with no `localDir` the record's
directory must be real and hold an `owner` marker naming its record's
directory (#529), a stopped native Sandbox is deleted and its
number freed, a damaged boot or port is refused, a native record is used only by
a native deployment (another deployment's reclaim leaves it out, its close
refuses it), a record of the former `local` runtime is never used or
deleted, `LAYERFS_SANDBOX` maps the former pair and refuses conflicts, and
`runtime.sandbox.records` lists every backend's record with its state. Record handling is the same code on Linux and macOS, so it runs here;
starting a native Sandbox is supported on macOS only (caller identity, loopback NFS); Linux refuses `LAYERFS_SANDBOX=native` at start and runs local Sandboxes with `namespaces` (#533 O27). The native records these tests write stand for Sandboxes a Linux host recorded before 2.0, which are still reopened, stopped and reclaimed by their records.

`native_admission` covers initial connection admission and its `Busy` deadline.
The deadline test pays the product's full 60-second readiness wait.
`native_connection_wait` covers a call that finds every connection of its
process lent: it is refused `Busy` after the same 60-second wait (#508), and
waits about 64 seconds in all. The mock
daemon has bounded socket waits and stops independently of a test assertion.
Neither target needs Docker, FUSE, or a Store. The JavaScript API remains outside
the CI lanes under the owner's existing policy; run these targets explicitly.
