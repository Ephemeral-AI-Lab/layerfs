# @layerfs/mcp

`@layerfs/mcp` exposes the public Node SDK over MCP stdio. It has no HTTP
transport and implements no Store or daemon behavior of its own.

```sh
node api/mcp/src/server.js
```

The process accepts newline-delimited MCP on stdin and writes only protocol
frames to stdout. Diagnostics go to stderr. Configure the same `LAYERFS_*`
environment used by `@layerfs/agent`, including `LAYERFS_STORE_PATH` and the
Sandbox deployment settings when those operations are used.

`layerfs_init` or `layerfs_open` establishes the process's one client. A second
open is refused until `layerfs_close`; closing is idempotent. The server accepts
Workspace handles returned by its mount/list calls, unmounts Workspaces it
created before closing, and performs the same cleanup on stdin EOF or SIGINT.
Cleanup failures are diagnostics, never successful unmount results.

Successful tool results carry the SDK result as JSON text and structured
content. Bytes become `{ "$layerfs": "bytes", "base64": "..." }` recursively.
A LayerFS refusal is an `isError` result containing one JSON text block with
`code`, `message`, and `detail`; non-refusal exceptions use code `Failed` and
do not expose stacks. Strict Zod schemas reject unknown keys before the SDK is
called. Branch names use lowercase alphanumerics plus `.`, `_`, and `-`, with
alphanumeric endpoints and a 63-byte limit.

`layerfs_events` remains in flight. It first emits
`notifications/layerfs_snapshot` with `{ cursor, snapshot }`, then emits
`notifications/layerfs_event` with `{ event }`. MCP cancellation unsubscribes;
as specified by MCP, a cancelled request receives no tool result.

The exact 50-tool schemas and SDK parity matrix are in
[`../agent/test/fixture/area-p-contracts.md`](../agent/test/fixture/area-p-contracts.md#p5--mcp-server-d11-stdio).
`layerfs_fs_apply` accepts exactly the SDK's seven change kinds (`write`,
`edit`, `mkdir`, `chmod`, `rm`, `cp`, `mv`); `layerfs_fs_read` returns the
whole SDK read answer, and `layerfs_branch_merge.units` takes plain patterns or
`{ name, patterns }` groups.
Push/pull remain blocked by R7 and have no tools. P12/P13 operations remain
pending merge reconciliation rather than speculative MCP names.

## Tests

`node --test api/mcp/test/stdio.test.js` runs the stdio suite against the fake
binding. `node --test api/mcp/test/real-store.test.js` drives a real Disposable
Store through the built addon; direct Store access needs an ext4/xfs/btrfs
filesystem, so name one with `LAYERFS_MCP_TEST_TMPDIR` (for example `$HOME` in
WSL) — the default temp directory is usually tmpfs, which the SDK refuses with
`Unsupported`.
