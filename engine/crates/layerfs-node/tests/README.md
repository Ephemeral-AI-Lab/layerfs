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
```

Use the worktree's registered Cargo resources for these commands. Node 20 or
later must be available as `node`, or selected by `LAYERFS_TEST_NODE`. The fork
fixture also needs `cc`. Tests locate the addon beside the test executables and
copy it to a private temporary directory with the `.node` suffix Node requires.

`native_discovery` covers gone and stopped containers, exact refusal custody,
number replacement during inspection, and discovery-lock release while a forked
child retains the descriptor. Its C fixture interposes `flock` only in the child
Node process; the fixture's fork is bounded and released on test unwinding.

`native_admission` covers initial connection admission and its `Busy` deadline.
The deadline test pays the product's full 60-second readiness wait. The mock
daemon has bounded socket waits and stops independently of a test assertion.
Neither target needs Docker, FUSE, or a Store. The JavaScript API remains outside
the CI lanes under the owner's existing policy; run these targets explicitly.
