Install LayerFS for Magpie {{version}} for me.

1. Clone https://github.com/Ephemeral-AI-Lab/layerfs beside this repository and check out commit {{layerfs.commit}} ({{layerfs.version}}). Use exactly this commit: it is the one this version was tested with.
2. Build the LayerFS native binding in that checkout: run `npm --prefix api install` (it only links the workspace packages and downloads nothing), then `npm --prefix api run build:native`. This needs a Rust toolchain (Cargo) and produces `api/binding/layerfs.node`. If the build fails, stop and report the error.
3. Clone {{repository}} beside it and check out commit {{commit}}.
4. Check the plugin against the real SDK: in the plugin checkout, set `LAYERFS_SDK_PATH` to the LayerFS checkout's `api/agent/sdk/src/index.js`, set `MAGPIE_LAYERFS_NATIVE_TEST=1`, and run `bun test`. Expected: 59 pass, 0 fail. (A plain `bun test` without those variables gives 56 pass and 3 skip; that is the sim side only.)
5. Install the plugin into Magpie from this checkout — the package is `@magpie-community/tools-layerfs`, and its registry build is sim-only, so the native backend comes from this checkout — then configure the host: set `LAYERFS_SDK_PATH` (the same path as in step 4) in the environment of the Magpie plugin host, and enable the plugin's `native` option (or set `MAGPIE_LAYERFS_NATIVE=1`). Leave the `exec` option off unless I ask for it.
6. Restart Magpie and ask an agent to call `layerfs_status`: it should report the native backend. If it reports sim, the opt-in or `LAYERFS_SDK_PATH` did not reach the host; tell me which one is missing.

Report what you ran and what each step returned. If anything fails, stop and give me the exact error, and don't retry it on your own.
