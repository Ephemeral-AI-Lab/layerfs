Install LayerFS for Codex parallel verify {{version}} for me.

Do these steps in order. Stop at the first one you cannot finish and report the step plus the exact error. Do not retry a LayerFS call that failed, and do not run init again for the same project name.

1. Check Node 20 or newer with `node -v`. The first line must be `v20` or a newer major. If it is older, stop.
2. Clone https://github.com/Ephemeral-AI-Lab/layerfs and check out commit {{layerfs.commit}} ({{layerfs.version}}). Use exactly this commit: it is the one this version was tested with. `git rev-parse HEAD` must print {{layerfs.commit}}. If it does not, stop. Export `LAYERFS_REPO` as that checkout.
3. Clone {{repository}} beside it and check out commit {{commit}}. `git rev-parse HEAD` must print {{commit}}.
4. In the plugin checkout, run `npm test`. Success is exit 0 and every test passing. This does not need Docker, a LayerFS daemon, or a Rust toolchain. You can also run `node bin/codex-parallel-verify.js --help`.
5. In the LayerFS checkout, `api/` is an npm workspace. Run `npm --prefix "$LAYERFS_REPO/api" install`. That links the packages from this commit; it does not download them from the registry. From the plugin checkout, confirm both public entry points load:

```bash
node --input-type=module -e 'import { loadSdk } from "./src/load.js"; const sdk = await loadSdk(); console.log(typeof sdk.LayerFS.open, typeof sdk.LayerFSRuntime.open)'
```

Success prints `function function`. If it does not, stop. Do not import `@layerfs/agent/internal`.

6. A live init needs a Linux `layerfs-daemon` and a runtime image built from commit {{layerfs.commit}}. Build them the way `api/README.md` and `api/how-to-use/runtime.md` in the LayerFS checkout describe (`npm --prefix "$LAYERFS_REPO/api" run build:native` builds the addon on a macOS or Linux host). Then set `LAYERFS_HOME` (for example `$HOME/.layerfs-codex`), `LAYERFS_IMAGE` to that image's sha256 id, and `LAYERFS_DAEMON` to the absolute path of `layerfs-daemon`. On macOS and Windows also set `LAYERFS_BUNDLE` to the sealed Linux bundle from that same commit (it contains `layerfs-daemon`, `layerfs.node`, and `manifest.json`). The CLI calls `runtime.worker.start()` once and sets `LAYERFS_WORKER_ENDPOINT` to `$LAYERFS_HOME/worker/endpoint.json`. Do not start a second worker for the same home. On Linux, leave `LAYERFS_WORKER_ENDPOINT` unset so Store calls stay in-process. On Windows, pass `--wsl-distro`, `--wsl-user`, `--wsl-node`, and `--wsl-home` when the runtime defaults (Ubuntu, root, `/usr/local/bin/node`) are wrong.
7. Initialize once from the plugin checkout:

```bash
node bin/codex-parallel-verify.js init --name shop --source "$PWD" --json
```

`--name` is a LayerFS project name: 1–63 characters, lowercase letters, digits, underscore, dot, or dash, starting and ending with a letter or digit. Exit 0 means the project exists. Exit 125 means LayerFS refused the call. Read the JSON on stderr and stop. Do not run init again for the same name. The ledger is `$LAYERFS_HOME/.codex-parallel-verify/shop.json`.

Report what you ran and what each step returned. If anything fails, stop and give me the exact error, and don't retry it on your own.
