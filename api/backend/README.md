# @layerfs/backend

Backend calls for an open LayerFS project, including Store history and storage data and daemon Workspace and process state.

Requires Node.js 20 or newer and `@layerfs/agent` at the same exact version. It uses the agent package's external native addon on Linux or the `@layerfs/runtime` worker on macOS and Windows; no native binary is bundled.

See the [backend guide](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/api/docs/backend.md) for usage.
