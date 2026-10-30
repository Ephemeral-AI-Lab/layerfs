# @layerfs/agent

LayerFS JavaScript SDK for working with projects, Branches, files, Sandboxes, and Workspaces.

Requires Node.js 20 or newer. The npm tarball contains JavaScript only: on Linux, set `LAYERFS_NATIVE` to a host-built `layerfs.node`; the checkout-relative addon fallback is not present in an installed tarball. On macOS and Windows, use the worker endpoint supplied by `@layerfs/runtime`. Store calls on other platforms fail `Unsupported`.

See the [agent guide](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/api/docs/agent.md) for settings, calls, native builds, and platform requirements.
