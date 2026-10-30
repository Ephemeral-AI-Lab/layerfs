# @layerfs/runtime

Runtime management for LayerFS homes, including project Init and removal, worker lifecycle, and Sandbox upkeep.

Requires Node.js 20 or newer and `@layerfs/agent` at the same exact version. The npm tarball contains JavaScript only. Linux Store calls use the agent package's external native addon; macOS and Windows also require a sealed Linux runtime bundle containing the native addon and daemon. The sealed bundle is external to this package.

See the [runtime guide](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/api/docs/runtime.md) for platform requirements and settings.
