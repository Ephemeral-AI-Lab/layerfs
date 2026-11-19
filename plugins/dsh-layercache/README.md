## What it does

DSH LayerCache adds `tvcache_run` and `tvcache_close` to DeepSeek Harness. Equivalent
trajectories reuse both tool observations and immutable LayerFS filesystem state;
diverging trajectories fork independent Branches. It implements TVCache's stateful
prefix-cache method independently, with a Node service and Python training client.

## How it works

The Cordis plugin sends explicit full tool histories to a LayerCache service. Its
Linux process uses the public `@layerfs/agent` SDK to store prefix records, chunked
results and immutable Commit/Layer refs. dsh's usual tool policy and result logging
remain active; cached counts and snapshot refs are persisted in result metadata.

Each rollout label belongs to its calling dsh session. Incremental requests retain
already-executed uncached effects. Failed or closed labels cannot silently restart;
HTTP requests are attempted once. Ordinary dsh filesystem/Bash tools keep their
own environments and do not participate in this cache.

## Requirements and install

Use dsh 0.2.2-rc.5, Node 24.9+ and pnpm. Install the release tarball with
`dsh plugin --profile web add /absolute/path/dsh-layercache-0.1.0.tgz`.
Start the Linux service against an existing LayerFS project using the pinned
LayerFS source; shell calls additionally require Docker/FUSE. Windows dsh can
connect to that Linux service. This plugin does not provision a worker or Store.
The repository has English/Chinese quickstarts and deployment instructions.

The default endpoint is `http://127.0.0.1:8088`, and plugin startup verifies that
it is a LayerFS service. A local directory reference backend is available only
when explicitly selected. The default tool allowlist contains file read/write/
append; enable shell deliberately for trusted command execution.

## Checked behavior and limits

27 Node tests include the actual published Cordis/dsh registry and native tool
executor; 4 Python integration tests cover the service client. A separate real
dsh → HTTP → LayerFS Store/FUSE scenario checks shared prefixes, A/B branch
isolation, full replay with zero execution, uncached-shell continuation and cleanup.
The release tarball was installed through the official dsh plugin command.

This is a preview, without a live-language-model Web UI or GPU-training claim.
LayerFS 1.1.0's tested Store is Disposable and does not promise power-loss durability.
Filesystem snapshots do not capture RAM, sockets or GPU state. One service owns
an index; pruning does not reclaim LayerFS history. LayerFS 2.0 and macOS have not
been qualified. The trusted execution service is not a multi-tenant sandbox.

## License and source

Apache-2.0. Independent of TVCache, DeepSeek and LayerFS; no upstream endorsement
is implied. Install exactly the listed plugin and LayerFS commits. The plugin's
repository includes the corresponding source, tests, license and qualification logs.
