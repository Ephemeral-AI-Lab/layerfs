### Task 1: Linux Environment and Recovery Baseline

**Files:**
- Create: `tools/docker/area-p-runtime.Dockerfile`
- Create: `tools/docker/README.md`
- Modify: `.gitignore` only if the runtime recipe generates a local file not already ignored

**Interfaces:**
- Produces: local image `layerfs-area-p-runtime:node24-python3`, a printed `resources.json` path, and verified `origin`/`upstream` remotes consumed by Tasks 2–4.

- [ ] **Step 1: Prove the current recovery state**

Run:
```bash
git -C E:/layerfs-connect/layerfs status --short --branch
git -C E:/layerfs-connect/layerfs remote -v
git -C E:/layerfs-connect/layerfs bundle verify E:/layerfs-connect/backups/layerfs-area-p-baseline-582bc786.bundle
```
Expected: branch `area-p/p1-p15`, bundle contains baseline tag, `origin` is the private Area P repository, `upstream` is public LayerFS. Then run `git -C E:/layerfs-connect/layerfs remote set-url --push upstream DISABLED_NO_UPSTREAM_PUSH` and confirm no upstream push URL remains usable.

- [ ] **Step 2: Install and qualify the dedicated WSL control plane**

Run `wsl.exe --install -d Ubuntu --no-launch`. If Windows requires restart or interactive setup, stop this task and report that exact blocker; do not alter Podman/Docker utility distributions. After installation:
```bash
wsl.exe -d Ubuntu -u root -- sh -lc 'apt-get update && apt-get install -y --no-install-recommends python3 lsof git ca-certificates && python3 --version && docker version && test -d /mnt/e/layerfs-connect/layerfs'
```
Expected: Python 3, Docker Engine API >=1.48, and the checkout visible at the pinned path.

- [ ] **Step 3: Write the runtime recipe and its documentation**

`area-p-runtime.Dockerfile` starts `FROM node:24.21.0-bookworm` and installs only `git`, `procps`, `lsof`, `python3`, `python3-venv`, and `python3-pip` with apt lists removed. `tools/docker/README.md` records exact build/pull commands, the no-Durable rule, and that FUSE capability comes from the product's Docker configuration rather than an unqualified image claim.

- [ ] **Step 4: Pull/build and pin all image identities**

Run in Ubuntu:
```bash
docker pull rust:1.85.1-bookworm
docker pull node:24.21.0-bookworm
docker build --file /mnt/e/layerfs-connect/layerfs/tools/docker/area-p-runtime.Dockerfile --tag layerfs-area-p-runtime:node24-python3 /mnt/e/layerfs-connect/layerfs/tools/docker
docker image inspect rust:1.85.1-bookworm node:24.21.0-bookworm layerfs-area-p-runtime:node24-python3 --format '{{.RepoTags}} {{.Id}}'
```
Expected: all images local; registration subsequently uses these exact resolved IDs.

- [ ] **Step 5: Register and prepare private build resources**

Run:
```bash
python3 /mnt/e/layerfs-connect/layerfs/tools/worktree.py register --worktree /mnt/e/layerfs-connect/layerfs --root /root/.cache/layerfs-worktrees --owner area-p --build-image rust:1.85.1-bookworm --runtime-image layerfs-area-p-runtime:node24-python3 --sandbox-image layerfs-area-p-runtime:node24-python3
python3 /mnt/e/layerfs-connect/layerfs/tools/worktree.py prepare --resources <printed-resources.json> --platform linux
```
Expected: a registration whose source is the pinned checkout, Linux Cargo/target volumes are private, dependencies are fetched only in `prepare`, and the printed resource path is recorded in `.omc/ultragoal/plans/area-p-20261028/ledger.jsonl` evidence.

- [ ] **Step 6: Run existing cheap baselines under explicit timeouts**

Run in Ubuntu with `timeout 100` for each test command:
```bash
cd /mnt/e/layerfs-connect/layerfs
timeout 100 python3 -B -m unittest tools.test_worktree
timeout 100 docker run --rm --pull never -v /mnt/e/layerfs-connect/layerfs:/work:ro -w /work layerfs-area-p-runtime:node24-python3 node api/agent/test/calls.mjs
timeout 100 docker run --rm --pull never -v /mnt/e/layerfs-connect/layerfs:/work:ro -w /work layerfs-area-p-runtime:node24-python3 node --test api/cli/test/cli.test.mjs
```
Expected: all exit 0 within 100 seconds; otherwise retain output and diagnose before product work.

- [ ] **Step 7: Commit the environment recipe**

Run the production LOC counter before staging and against the staged tree, then commit only Task 1 files with `build: add Area P Linux runtime recipe`, the LOC comparison and `Co-Authored-By: Claude Code <noreply@anthropic.com>`. Push is a separate user-approved operation subject to the OMC Git guardrail.
