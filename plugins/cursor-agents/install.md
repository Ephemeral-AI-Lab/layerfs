Install LayerFS for Cursor {{version}} for me. Plugin id `cursor-agents`, package `layerfs-cursor-agents`. Cursor only. Do not add Claude Code skills, hooks, or configuration.

Do these steps in order. If anything fails, stop and give me the exact error, and don't retry it on your own.

1. Clone https://github.com/Ephemeral-AI-Lab/layerfs and check out commit {{layerfs.commit}} ({{layerfs.version}}). Use exactly this commit: it is the one this version was tested with. `git rev-parse HEAD` must print {{layerfs.commit}}. If it does not, stop. Export `LAYERFS_REPO` as that checkout.
2. Clone {{repository}} beside it and check out commit {{commit}}. `git rev-parse HEAD` must print {{commit}}. Its `layerfs.json` `commit` must be {{layerfs.commit}}.
3. Check the tools, once each. Stop if either is missing or Node is older than 20.
   `node -v`
   `git --version`
4. From the plugin repository root, run setup once, with `--layerfs` set to the checkout from step 1.
   On Linux, pass `--build-native` so the in-process Store addon is built:
   `node bin/layerfs-cursor-setup.js --build-native --layerfs "$LAYERFS_REPO"`
   On macOS and Windows, do not pass `--build-native`. Those routes use the worker. The native build throws on Windows, and macOS does not need a Rust toolchain for Store calls. Setup ignores the flag there:
   `node bin/layerfs-cursor-setup.js --layerfs "$LAYERFS_REPO"`
   Add a flag only when you already know the value: `--home`, `--prefix`, `--settings-file`, `--project`, `--store-path`, `--image`, `--daemon`, `--volume`, `--docker`, `--bundle`, `--container-prefix`, `--cursor-project`.
   Use `--settings-file` for the env file. Node reserves `--env-file`, so that name never reaches this script.
   On macOS and Windows, `--image` and `--bundle` are required or the worker start fails. Stop with that error.
   On Windows the process stays in the foreground to keep the worker alive. Leave it running and use another terminal for the later steps.
5. Source the env file the script writes (by default `layerfs.env` in the plugin repository) in the shell you will use for LayerFS:
   `set -a && source layerfs.env && set +a`
   If the script said the shim directory is not on PATH, prepend that directory to PATH in this shell.
6. Check, once each:
   `node bin/layerfs-cursor-setup.js check`
   `layerfs help`
   `node --test`
7. Read `.cursor/skills/layerfs-agent/SKILL.md` before editing the project. Do not create a git worktree.

Report what you ran and what each step returned. If anything fails, stop and give me the exact error, and don't retry it on your own.
