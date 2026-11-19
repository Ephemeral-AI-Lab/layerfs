Install DSH LayerCache {{version}} for me.

Use these steps in order and stop on the first error. Do not retry or replay a
LayerFS call or cache mutation after a refusal, timeout or lost response. Report
what ran and its exact outcome. Do not alter an existing Store or restart a dsh
profile with an active run.

1. Check Node 24.9 or newer, pnpm and DeepSeek Harness 0.2.2-rc.5. This release
   supports a Linux service and Linux/Windows-WSL2 dsh clients. On Windows, run
   the service in Linux/WSL2 and keep its Store on local Linux ext4/xfs/btrfs,
   never /mnt/c. macOS is not qualified. Do not install system-wide prerequisites
   without the user's permission.
2. Clone https://github.com/Ephemeral-AI-Lab/layerfs beside the plugin checkout
   and check out {{layerfs.commit}} ({{layerfs.version}}). Confirm its HEAD is
   exactly that SHA. Read its AGENTS.md, api/README.md, api/docs/agent.md and the
   .agents/skills/layerfs-js-sdk/ skill and references before LayerFS operations.
3. Clone {{repository}} and check out {{commit}}. Confirm HEAD is exactly that
   SHA and layerfs.json names {{layerfs.commit}}. Read README.md, docs/dsh.md,
   docs/layerfs-setup.md and docs/dsh-validation.md. Run npm ci --ignore-scripts,
   npm test and npm run check. Bound each test invocation at 120 seconds.
4. Use the pinned LayerFS public SDK and a prepared existing project. Build its
   native addon/daemon following its guide if missing. For shell, confirm the
   selected Docker Engine/FUSE runtime and immutable image ID. Store calls run
   in the Linux service process. Set its LAYERFS_* settings from the project
   owner; never guess a Store, initialize over one, or use an internal API.
   If no project is available, report the missing prerequisite before Init.
5. Start the service from the plugin checkout with node src/cli.js serve,
   --backend layerfs, --project naming that existing project, --root a private
   owner-lock directory and --namespace a versioned experiment. Link the pinned
   @layerfs/agent package or pass --sdk its public entry file. Use only one
   service per project/index and a shared lock path. Configure LAYERCACHE_TOKEN
   in both processes if authentication is enabled. Never print its value.
6. Download dsh-layercache-{{version}}.tgz and SHA256SUMS.txt from the plugin's
   GitHub release tagged v{{version}}. Verify the tarball checksum. Back up the
   chosen dsh profile's package.json and cordis.patch.yml before installation.
   Run dsh plugin --profile web add with the tarball's absolute path. It contains
   executable JavaScript and requires no build/prepare permission. Run
   dsh --profile web --dump-config and confirm its dsh-layercache layer.
7. Keep existing profile entries. Add the layercache row's config as docs/dsh.md
   describes: endpoint, backend: layerfs, tokenEnv, requestTimeoutMs, maxRollouts,
   and allowedTools. Default endpoint is http://127.0.0.1:8088. Keep shell absent
   unless the user intends trusted shell execution. Start dsh only after the
   service is ready. Startup refuses a backend mismatch; never change to local
   just to hide a failed LayerFS setup.
8. In a dsh session, use tvcache_run with a fresh task_id, rollout A and a complete
   history that writes /answer.txt then reads it. Close A with tvcache_close.
   Use a new label B with the same task_id/history: value must match, executed
   must be 0 and cache_hits must be 2. File paths belong to the cached project.
   Close B. Use cache:false for external effects. A failed/closed label does not
   reopen; inspect uncertain effects before deliberately starting a new label.

Report versions, both source SHAs, service backend, installation outcome and
observed value/cache_hits/executed. Do not report skipped native checks as passes.
