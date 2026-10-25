Install LayerFS for Pi {{version}} for me.

1. Clone https://github.com/Ephemeral-AI-Lab/layerfs and check out commit {{layerfs.commit}} ({{layerfs.version}}). Use exactly this commit: it is the one this version was tested with.
2. Clone {{repository}} beside it and check out commit {{commit}}.
3. Check the machine before building: Linux x86_64, `/dev/fuse` present, `docker info` works through the local Unix socket, and Node 22.19 or newer (Rust runs in its builder image; no host toolchain is needed). If any is missing, stop and tell me which.
4. Build in the plugin checkout:
   - `npm ci`
   - `bash scripts/build-native.sh ../layerfs`: it refuses a LayerFS checkout that is not at {{layerfs.commit}} or has tracked changes, then builds the native binding and `layerfs-daemon` in the `rust:1.85.1-bookworm` image.
   - `npm run release:build -- --upstream ../layerfs`: it builds the plugin, the runtime image (Pi 1.4.0 included) and `release-out/pi-layerfs-{{version}}-linux-x64.tar.gz`.
5. Install: `tar -xzf release-out/pi-layerfs-{{version}}-linux-x64.tar.gz -C release-out`, then `bash release-out/pi-layerfs-{{version}}-linux-x64/install.sh`. It installs to `~/.local/share/pi-layerfs` and `~/.local/bin/pi-layerfs`; ask me before passing `--prefix`, `--bin-dir` or `--skip-register`.
6. Check that it works: `pi-layerfs --self-test` (add `~/.local/bin` to PATH if needed) must pass. To use it, run `pi-layerfs` in a disposable copy of a project and sign in to a model provider with Pi's `/login`; do not copy any credentials yourself.

Report what you ran and what each step returned. If anything fails, stop and give me the exact error, and don't retry it on your own.
