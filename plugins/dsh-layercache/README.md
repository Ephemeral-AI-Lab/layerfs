## What it does

DSH LayerCache adds `tvcache_prepare`, `tvcache_run`, and `tvcache_close` to DeepSeek
Harness. Equivalent trajectories reuse both tool results and immutable LayerFS
filesystem states. Diverging trajectories fork independent branches. The plugin
implements TVCache-style prefix caching and includes a Node service and Python
client for post-training workloads.

## How it works

The Cordis plugin sends explicit full tool histories to a LayerCache service. Its
Linux process uses the public `@layerfs/agent` SDK to store prefix records, chunked
results, and immutable Commit or Layer refs. dsh applies its usual tool policy
and logs cache hit counts, execution counts, and snapshot refs.

Each rollout label belongs to its calling dsh session. Incremental requests retain
already accepted steps. Cache hits validate the parent checkpoint and revision,
tool policy, resource versions, and expiry. A tool can return its checkpoint
while the service publishes the cache record in the background. Eligible
rollouts in the same process can already reuse the pending record. Ordinary dsh
tools retain their own execution environments.

## Requirements and install

Use dsh 0.2.2-rc.5, Node 24.9+ and pnpm. Follow the Install prompt to check out
the two pinned commits and package the plugin with `npm pack`. Install it with
`dsh plugin --profile web add /absolute/path/dsh-layercache-0.1.0.tgz`.
Start the Linux service against an existing LayerFS project using the pinned
LayerFS source; shell calls additionally require Docker/FUSE. Windows dsh can
connect to that Linux service. This plugin does not provision a worker or Store.
The repository has English/Chinese quickstarts and deployment instructions.

The default endpoint is `http://127.0.0.1:8088`, and plugin startup verifies that
it is a LayerFS service. A local directory reference backend is available only
when explicitly selected. The default tool allowlist contains file read/write/
append; enable shell deliberately for trusted command execution.

## Video tool benchmarks

On LayerFS 1.1.0, fixed FFmpeg tool traces with synchronous publication ran
6.54× faster on two Terminal-Bench `video-processing` fixtures (11.65 s to 1.78 s,
mean wall time over three trials) and 3.97× faster on the 100-video EgoSchema
manifest (6,881.16 s to 1,733.95 s, sum of per-video elapsed times).
Cached and uncached outputs matched in both workloads. The measurements cover
tool execution and cache overhead. See the [benchmark details](https://github.com/aotenjou/dsh-layercache/blob/747d82652e28594055581f77aa11d50e3e47168c/README.md#video-tool-benchmarks).

## Checked behavior and limits

The Node suite uses the published Cordis/dsh tool registry and execution pipeline.
Python tests cover the service client. A separate dsh → HTTP → LayerFS Store/FUSE
integration scenario checks shared prefixes, branch isolation, replay,
uncached-shell continuation, and cleanup.

The tested LayerFS 1.1.0 Store uses the Disposable profile.
Filesystem snapshots do not capture RAM, sockets or GPU state. One service owns
an index; pruning does not reclaim LayerFS history. LayerFS 2.0 and macOS have not
been qualified. The trusted execution service is not a multi-tenant sandbox.

## License and source

Apache-2.0. The plugin's repository includes the corresponding source, tests,
license, and qualification logs.
