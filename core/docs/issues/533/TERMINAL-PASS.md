# #533 step 19: terminal pass on this Mac

> **Status:** Dated checkpoint, 2026-11-02; not release evidence or a product
> contract. Delegated by the owner (asleep); nothing is pushed or merged.
> The current result is the [seven-kind pass at `6837bcfce2`](#seven-kind-pass-at-6837bcfce2-2026-11-02):
> 12/12 on all seven kinds and on the two Linux rows, every other step-19
> item PASS (Docker unavailable: PASS, simulated), and only R2, Windows and
> Durable NOT_RUN. The final product commit is `e9c0e793d3`: three fixes
> after the pass (`f43ec45fa0` and before) and two from the final review,
> all outside the cell path, each followed by a resealed bundle and C01 on
> every VM kind and ssh (see "After the pass" and "After the final
> review"). The earlier runs follow unchanged.

Run on branch `issue533-v2` at product commit `11551a8d4f` (the twelve-cell
runner commit; the product source is `6775d06789`'s). Host: this Mac (arm64,
14 CPUs, 36 GiB, macOS Darwin 25.4.0), Docker 29.5.2. All binaries are
**debug** builds (`cargo +1.85.1`, `target/debug`), so every time below is
diagnostic only: it is not a measurement and is not compared with #529's
release numbers. One sample per cell; the 1-minute load is in each receipt.
Store profile: Disposable only. Durable: NOT_RUN — disabled by owner until
explicit reauthorization.

Receipts: [`receipts/11551a8d4f/`](receipts/11551a8d4f/), one JSON per cell
(`lfs533-cell/1`), written by `api/node/test/verify/cells/cell.mjs`.

**Rerun at product commit `a487409fe5`** (owner instruction, 2026-11-02:
build the `docker` and PostgreSQL Sandbox paths before step 19). The rows
below carry the rerun's verdict where it changed; the first run's rows are
kept in the section after the table. Receipts:
[`receipts/a487409fe5/`](receipts/a487409fe5/).

## Seven-kind pass at `6837bcfce2` (2026-11-02)

**This is the step-19 result.** The owner's condition, relayed on
2026-11-02: the 12 #516 cells PASS on all seven machine kinds (`native`,
`docker`, `apple-container`, `microsandbox`, `lima`, `firecracker`, `ssh`),
and the Linux rows pass too. The sections after this one are the earlier
runs, kept unchanged.

Every cell below ran at product commit `6837bcfce2` with one sealed Linux
bundle (`tools/linux_bundle.py`, debug profile, `cargo +1.85.1`), hash
`4a77c75e…`, source head `6837bcfce2`, image `sha256:1d91885d…`. All
binaries are **debug** builds, so every time is diagnostic only. One sample
per cell; the 1-minute load is in each receipt. Store profile: Disposable
only. Durable: NOT_RUN — disabled by owner until explicit reauthorization.
Receipts: [`receipts/6837bcfce2/`](receipts/6837bcfce2/), 108 files
(`lfs533-cell/1`). As in P2-30, this Mac's per-user temp folder in a
macOS `native` receipt's Workspace path reads `<tmp>/…` (here and in
`receipts/44db721d95/` and `164e1297cb/`); nothing else was changed.

### Result: one row per kind

| Kind | Row | Machine | Cells | Store native → machine → native |
| --- | --- | --- | --- | --- |
| `native` | macOS | this Mac's native daemon | **PASS 12/12** | – (stays native) |
| `docker` | macOS | Docker Desktop 29.5.2; the Store in the home's docker worker container | **PASS 12/12** | moved out in each cell; the cell's home and worker are removed after |
| `apple-container` | macOS | `ac1`, `container` 1.6.1, the bundle copied in (no bind mount) | **PASS 12/12** | out and back in every cell |
| `microsandbox` | macOS | `msb1`, `msb` 0.7.11 from `~/.microsandbox/bin`, put on `PATH` by the runner | **PASS 12/12** | out and back in every cell |
| `lima` | macOS | `lima1`, Lima 2.2.1, vz, 2 CPUs, 4 GiB | **PASS 12/12** | out and back in every cell |
| `firecracker` | macOS | `fc1`, Firecracker v1.17.0 on the nested vz host `fch` (4 CPUs, 4 GiB), 2,048 MiB, 2 MiB huge pages, no CPU template | **PASS 12/12** | out and back in every cell |
| `ssh` | macOS | `ssh1`, a throwaway Lima vz VM (`lfs533-ssh`, 2 CPUs, 4 GiB) | **PASS 12/12** | out and back in every cell |
| `native` | Linux, Lima QEMU VM | `lfs533-linux`: Ubuntu 24.04.5, kernel 6.8.0-147, 4 CPUs, 4 GiB, as root; namespaces Sandboxes | **PASS 12/12** | – (stays native) |
| `docker` | Linux, Lima QEMU VM | the same VM, Docker Engine 29.1.3; the Store in the docker worker container | **PASS 12/12** | moved out in each cell; the cell's home is removed after |

Every row: C03, C09 and C11 are `UpToDate` and the rest `Committed`, as in
#529's rows. The VM kinds' Sandboxes run the namespaces runtime inside the
guest. Each VM kind's C01 started its machine from stopped on the first call
(`lima1`, `ac1`, `msb1`, `ssh1`'s worker; `fc1` after the warm-up below).
The microVM's config read `"huge_pages":"2M"` and `"mem_size_mib":2048` with
no `cpu_template`, and 1,023 of the host's 1,100 huge pages were in use.

Host: this Mac (arm64, 14 CPUs, 36 GiB, macOS 26.4.1), Node v24.21.0.

### Commands

macOS, one cell per command (`$W` is the worktree, `$H` a home that holds
the VM machines; a `docker` or `native` cell makes its own home):

```bash
cd $W/api/node && PATH=$HOME/.microsandbox/bin:$PATH \
  LFS533_DAEMON=$W/target/debug/layerfs-daemon LFS533_HOME=$H \
  LFS533_IMAGE=sha256:afa1ef9c27294f9c7298820c06483e6fb1ddafc43a7cc7d6dec8cd2e67abf38e \
  perl -e 'alarm 115; exec @ARGV' node test/verify/cells/cell.mjs C01 lima1 \
  $W/core/docs/issues/533/receipts/6837bcfce2/macos-lima1-C01.json
```

Linux, as root in the VM, with the same sealed bundle and a copy of
`api/node` built with the Linux addon (`LFS533_PRODUCT` names the commit,
since the copy has no Git):

```bash
B=/root/.cache/layerfs/bundle/2.0.0/linux-aarch64
cd /opt/lfs533/api/node && LFS533_PRODUCT=6837bcfce2 LFS533_DAEMON=$B/layerfs-daemon \
  LFS533_LAYERFS=$B/layerfs \
  LFS533_IMAGE=sha256:1499b86412fa5c7474cf28832aca519531ba0f08dc79f5e1f7e441da9ab7a0a7 \
  timeout 115 node test/verify/cells/cell.mjs C01 native /opt/lfs533/receipts/linux-native-C01.json
```

(`1499b864…` is the same Sandbox image loaded into Docker 29's containerd
store, which names it differently; its config digest is `1d91885d…`.)

### Declared setup

None of these is timed product work; each is recorded so the pass can be
repeated.

- **Machines made once per kind** in one home (`lima1`, `ac1`, `msb1`,
  `ssh1`, `fc1` on `fch`), each by `layerfs machine create`, before the
  cells; each cell made, moved, moved back and removed its own Store there.
- **Loading a rebuilt bundle.** A running machine keeps the worker and
  bundle it started with (there is no `machine stop`). After each rebuild the
  VMs were stopped with `limactl stop` / `container stop`, or left to the idle
  stop, so the next call started them with the new bundle.
- **Huge pages on `fch`**: `echo 1100 > /proc/sys/vm/nr_hugepages` after
  `limactl start lfs-0d868af2-fch`, as #516 and #527 did. Nothing in the
  product sets them (DECISIONS, "Gaps").
- **Warm-up move for `fc1`**: a throwaway Store `warmfc` moved `fc1 →
  native` (58 s, which started `fch`'s worker, copied the bundle's
  `firecracker/` folder and booted the microVM) so C01 did not pay a cold
  start against its 115 s limit. Earlier heads did the same `native → fc1`.
- **Firecracker's files**, #516's recipe without Node: Firecracker v1.17.0,
  the CI kernel `vmlinux-6.18.56` and the CI Ubuntu 24.04 rootfs, each
  checked against its sha256, a fresh root key, a 5 GiB sparse ext4. Built as
  root in the throwaway `lfs533-ssh` VM (apt inside a Docker container hung),
  then placed at the sealed bundle's `firecracker/` after sealing, without
  the public key. `tools/linux_bundle.py` does not build it.
- **apple-container** stages the bundle by copy, as the owner directed
  (no bind mount of the host cache); the copy leaves out `firecracker/`.
- **The Linux VM** (`lfs533-linux`) came from Lima's `docker-rootful`
  template with its image replaced by Ubuntu 24.04 (`--set '.images=…'`; the
  template's default is 26.04). Its Docker provisioning did not run, so
  `docker.io` was installed with apt, which took about 9 minutes (above the
  5-minute limit; run in the background). `sysctl -w
  user.max_user_namespaces=0` was set before the native row, as #527 lists:
  with user namespaces allowed, `native` refuses (sample kept below).
- **Lima disk names stay short** (`lfs-0d868af2-<name>`).

### Times

Times in ms (debug build) from each receipt's timed phase; the move columns
are the cell's `store.move` and `store.move.back` in seconds.

| Receipt | Verdict | Mount | Exec | Commit (kind) | Unmount | Move out / back (s) | Load |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| linux-docker-C01 | PASS | 11.6 | 814.4 | 555.1 (Committed) | 8.3 | 0.6 / – | 1.1 |
| linux-docker-C02 | PASS | 11.1 | 1473.6 | 562.8 (Committed) | 8.1 | 0.3 / – | 1.2 |
| linux-docker-C03 | PASS | 11.6 | 1325.1 | 21.7 (UpToDate) | 6.8 | 0.4 / – | 1.7 |
| linux-docker-C04 | PASS | 13.0 | 1124.6 | 605.7 (Committed) | 7.7 | 0.3 / – | 1.7 |
| linux-docker-C05 | PASS | 11.3 | 1182.0 | 605.5 (Committed) | 7.8 | 0.3 / – | 1.9 |
| linux-docker-C06 | PASS | 11.4 | 1088.2 | 6337.2 (Committed) | 15.0 | 0.4 / – | 2.0 |
| linux-docker-C07 | PASS | 11.6 | 2228.3 | 12639.8 (Committed) | 22.4 | 0.3 / – | 3.0 |
| linux-docker-C08 | PASS | 11.7 | 1107.4 | 6328.8 (Committed) | 14.4 | 0.4 / – | 2.6 |
| linux-docker-C09 | PASS | 13.4 | 167.1 | 5.1 (UpToDate) | 10.6 | 0.3 / – | 2.6 |
| linux-docker-C10 | PASS | 13.7 | 1216.8 | 169.5 (Committed) | 18.7 | 0.3 / – | 2.5 |
| linux-docker-C11 | PASS | 14.0 | 272.8 | 1474.4 (UpToDate) | 15.2 | 0.3 / – | 2.5 |
| linux-docker-C12 | PASS | 12.6 | 435.8 | 249.9 (Committed) | 7.1 | 0.3 / – | 2.4 |
| linux-native-C01 | PASS | 6.5 | 782.4 | 548.6 (Committed) | 4.3 | – | 2.6 |
| linux-native-C02 | PASS | 6.6 | 1413.7 | 551.1 (Committed) | 4.2 | – | 2.3 |
| linux-native-C03 | PASS | 6.3 | 1319.7 | 18.7 (UpToDate) | 3.2 | – | 2.6 |
| linux-native-C04 | PASS | 6.8 | 1099.7 | 608.9 (Committed) | 3.7 | – | 2.7 |
| linux-native-C05 | PASS | 6.2 | 1159.4 | 601.4 (Committed) | 4.0 | – | 2.7 |
| linux-native-C06 | PASS | 6.2 | 1079.3 | 6339.1 (Committed) | 10.7 | – | 2.8 |
| linux-native-C07 | PASS | 6.6 | 2198.1 | 12620.7 (Committed) | 17.3 | – | 2.8 |
| linux-native-C08 | PASS | 6.3 | 1086.2 | 6339.5 (Committed) | 10.9 | – | 2.2 |
| linux-native-C09 | PASS | 9.5 | 155.8 | 1.2 (UpToDate) | 5.4 | – | 2.2 |
| linux-native-C10 | PASS | 9.3 | 1194.7 | 159.4 (Committed) | 14.2 | – | 2.1 |
| linux-native-C11 | PASS | 9.8 | 248.5 | 1460.9 (UpToDate) | 12.0 | – | 2.1 |
| linux-native-C12 | PASS | 6.1 | 407.1 | 244.4 (Committed) | 3.2 | – | 2.4 |
| macos-ac1-C01 | PASS | 8.1 | 515.1 | 527.6 (Committed) | 6.3 | 10.4 / 0.2 | 3.8 |
| macos-ac1-C02 | PASS | 7.9 | 1050.4 | 524.2 (Committed) | 6.7 | 0.1 / 0.2 | 3.5 |
| macos-ac1-C03 | PASS | 7.5 | 944.8 | 20.6 (UpToDate) | 5.0 | 0.1 / 0.1 | 3.4 |
| macos-ac1-C04 | PASS | 7.6 | 771.5 | 575.5 (Committed) | 6.0 | 0.1 / 0.1 | 3.9 |
| macos-ac1-C05 | PASS | 7.4 | 825.2 | 569.4 (Committed) | 6.1 | 0.1 / 0.1 | 3.9 |
| macos-ac1-C06 | PASS | 8.0 | 1010.3 | 6101.5 (Committed) | 10.3 | 0.1 / 0.1 | 3.9 |
| macos-ac1-C07 | PASS | 8.0 | 2066.4 | 12183.8 (Committed) | 15.6 | 0.1 / 0.1 | 4.2 |
| macos-ac1-C08 | PASS | 8.3 | 1022.2 | 6102.8 (Committed) | 10.4 | 0.1 / 0.1 | 2.8 |
| macos-ac1-C09 | PASS | 11.3 | 103.7 | 3.9 (UpToDate) | 6.4 | 0.1 / 0.1 | 2.8 |
| macos-ac1-C10 | PASS | 10.2 | 1101.3 | 149.7 (Committed) | 11.5 | 0.1 / 0.1 | 2.8 |
| macos-ac1-C11 | PASS | 11.3 | 222.2 | 1398.3 (UpToDate) | 10.2 | 0.1 / 0.1 | 3.1 |
| macos-ac1-C12 | PASS | 7.6 | 301.6 | 231.8 (Committed) | 5.9 | 0.1 / 0.1 | 3.1 |
| macos-docker-C01 | PASS | 13.0 | 671.2 | 557.9 (Committed) | 8.4 | 0.8 / – | 5.4 |
| macos-docker-C02 | PASS | 14.1 | 1295.6 | 552.6 (Committed) | 9.9 | 0.8 / – | 4.4 |
| macos-docker-C03 | PASS | 16.3 | 1088.5 | 25.2 (UpToDate) | 7.5 | 0.8 / – | 4.0 |
| macos-docker-C04 | PASS | 14.7 | 1028.8 | 603.4 (Committed) | 10.4 | 0.7 / – | 4.6 |
| macos-docker-C05 | PASS | 15.0 | 1098.2 | 604.5 (Committed) | 9.3 | 0.8 / – | 4.3 |
| macos-docker-C06 | PASS | 13.3 | 1061.5 | 6173.9 (Committed) | 15.5 | 0.8 / – | 4.1 |
| macos-docker-C07 | PASS | 14.3 | 2151.7 | 12318.0 (Committed) | 21.3 | 0.8 / – | 4.6 |
| macos-docker-C08 | PASS | 13.4 | 1083.4 | 6171.8 (Committed) | 16.3 | 0.8 / – | 4.0 |
| macos-docker-C09 | PASS | 21.5 | 149.9 | 5.2 (UpToDate) | 9.2 | 0.8 / – | 3.8 |
| macos-docker-C10 | PASS | 18.0 | 1170.3 | 167.1 (Committed) | 16.3 | 0.8 / – | 4.0 |
| macos-docker-C11 | PASS | 15.3 | 239.3 | 1451.5 (UpToDate) | 16.0 | 0.8 / – | 4.1 |
| macos-docker-C12 | PASS | 14.5 | 391.0 | 247.5 (Committed) | 9.3 | 0.8 / – | 3.9 |
| macos-fc1-C01 | PASS | 18.7 | 946.2 | 670.8 (Committed) | 14.4 | 0.1 / 0.2 | 8.5 |
| macos-fc1-C02 | PASS | 18.2 | 1679.9 | 718.9 (Committed) | 25.7 | 0.1 / 0.2 | 8.2 |
| macos-fc1-C03 | PASS | 14.8 | 1669.5 | 33.4 (UpToDate) | 12.1 | 0.1 / 0.1 | 7.8 |
| macos-fc1-C04 | PASS | 18.5 | 1419.5 | 721.8 (Committed) | 12.4 | 0.1 / 0.1 | 7.5 |
| macos-fc1-C05 | PASS | 13.2 | 1430.1 | 736.5 (Committed) | 11.9 | 0.1 / 0.1 | 7.1 |
| macos-fc1-C06 | PASS | 16.1 | 1276.6 | 7282.0 (Committed) | 38.6 | 0.1 / 0.1 | 7.0 |
| macos-fc1-C07 | PASS | 17.5 | 2621.6 | 14526.7 (Committed) | 27.3 | 0.1 / 0.1 | 5.7 |
| macos-fc1-C08 | PASS | 15.6 | 1317.3 | 7265.8 (Committed) | 17.6 | 0.1 / 0.1 | 5.3 |
| macos-fc1-C09 | PASS | 14.9 | 164.7 | 9.5 (UpToDate) | 13.1 | 0.1 / 0.1 | 5.3 |
| macos-fc1-C10 | PASS | 15.4 | 1494.0 | 183.0 (Committed) | 18.4 | 0.1 / 0.1 | 5.2 |
| macos-fc1-C11 | PASS | 21.2 | 315.6 | 1689.0 (UpToDate) | 17.3 | 0.1 / 0.1 | 5.3 |
| macos-fc1-C12 | PASS | 16.4 | 592.7 | 313.9 (Committed) | 13.7 | 0.1 / 0.1 | 5.2 |
| macos-lima1-C01 | PASS | 8.8 | 586.7 | 537.2 (Committed) | 7.3 | 20.0 / 0.2 | 3.3 |
| macos-lima1-C02 | PASS | 8.7 | 1170.4 | 542.8 (Committed) | 6.3 | 0.1 / 0.2 | 4.0 |
| macos-lima1-C03 | PASS | 8.7 | 955.2 | 20.9 (UpToDate) | 5.3 | 0.1 / 0.1 | 4.0 |
| macos-lima1-C04 | PASS | 8.5 | 882.1 | 574.0 (Committed) | 6.2 | 0.1 / 0.1 | 4.0 |
| macos-lima1-C05 | PASS | 8.7 | 941.6 | 577.3 (Committed) | 6.3 | 0.1 / 0.1 | 4.0 |
| macos-lima1-C06 | PASS | 8.0 | 1021.7 | 6138.4 (Committed) | 15.4 | 0.1 / 0.1 | 3.9 |
| macos-lima1-C07 | PASS | 9.2 | 2092.2 | 12250.7 (Committed) | 17.5 | 0.1 / 0.1 | 4.1 |
| macos-lima1-C08 | PASS | 8.4 | 1029.0 | 6142.3 (Committed) | 14.3 | 0.1 / 0.1 | 3.7 |
| macos-lima1-C09 | PASS | 10.8 | 126.5 | 4.5 (UpToDate) | 7.3 | 0.1 / 0.1 | 3.8 |
| macos-lima1-C10 | PASS | 10.5 | 1134.4 | 158.4 (Committed) | 15.8 | 0.1 / 0.1 | 3.8 |
| macos-lima1-C11 | PASS | 11.2 | 221.5 | 1410.6 (UpToDate) | 11.8 | 0.1 / 0.1 | 3.7 |
| macos-lima1-C12 | PASS | 8.3 | 315.5 | 233.3 (Committed) | 6.2 | 0.1 / 0.1 | 3.9 |
| macos-msb1-C01 | PASS | 7.6 | 667.6 | 605.4 (Committed) | 57.0 | 24.2 / 0.2 | 4.2 |
| macos-msb1-C02 | PASS | 8.1 | 1133.5 | 576.1 (Committed) | 51.6 | 0.1 / 0.2 | 5.0 |
| macos-msb1-C03 | PASS | 7.2 | 1076.1 | 72.8 (UpToDate) | 54.2 | 0.1 / 0.1 | 4.9 |
| macos-msb1-C04 | PASS | 7.3 | 906.9 | 623.2 (Committed) | 51.8 | 0.1 / 0.1 | 4.9 |
| macos-msb1-C05 | PASS | 7.4 | 930.9 | 632.6 (Committed) | 59.1 | 0.1 / 0.1 | 4.7 |
| macos-msb1-C06 | PASS | 7.7 | 1086.8 | 6160.8 (Committed) | 53.4 | 0.1 / 0.1 | 4.8 |
| macos-msb1-C07 | PASS | 7.4 | 2106.6 | 12236.0 (Committed) | 60.7 | 0.1 / 0.1 | 4.6 |
| macos-msb1-C08 | PASS | 7.9 | 1079.8 | 6149.0 (Committed) | 55.4 | 0.1 / 0.1 | 4.3 |
| macos-msb1-C09 | PASS | 10.6 | 185.0 | 51.0 (UpToDate) | 52.5 | 0.1 / 0.1 | 4.3 |
| macos-msb1-C10 | PASS | 10.0 | 1202.4 | 195.5 (Committed) | 59.1 | 0.1 / 0.1 | 4.3 |
| macos-msb1-C11 | PASS | 10.6 | 291.2 | 1448.6 (UpToDate) | 60.1 | 0.1 / 0.1 | 4.1 |
| macos-msb1-C12 | PASS | 7.4 | 408.7 | 294.4 (Committed) | 49.9 | 0.1 / 0.1 | 4.5 |
| macos-native-C01 | PASS | 28.8 | 1948.8 | 620.6 (Committed) | 44.1 | – | 4.8 |
| macos-native-C02 | PASS | 27.8 | 6425.1 | 624.6 (Committed) | 43.0 | – | 4.6 |
| macos-native-C03 | PASS | 24.9 | 2813.7 | 79.8 (UpToDate) | 19.7 | – | 6.6 |
| macos-native-C04 | PASS | 29.9 | 4220.3 | 651.2 (Committed) | 39.2 | – | 6.3 |
| macos-native-C05 | PASS | 28.7 | 4345.1 | 650.0 (Committed) | 41.5 | – | 5.7 |
| macos-native-C06 | PASS | 27.4 | 1745.1 | 6166.0 (Committed) | 21.9 | – | 5.7 |
| macos-native-C07 | PASS | 28.1 | 3451.7 | 12245.5 (Committed) | 31.1 | – | 5.8 |
| macos-native-C08 | PASS | 29.1 | 1761.7 | 6115.0 (Committed) | 25.5 | – | 5.8 |
| macos-native-C09 | PASS | 29.0 | 86.3 | 67.6 (UpToDate) | 27.1 | – | 5.5 |
| macos-native-C10 | PASS | 29.5 | 1830.3 | 257.0 (Committed) | 20.3 | – | 5.5 |
| macos-native-C11 | PASS | 29.0 | 658.6 | 1517.1 (UpToDate) | 21.7 | – | 5.2 |
| macos-native-C12 | PASS | 28.8 | 750.8 | 308.9 (Committed) | 29.2 | – | 4.6 |
| macos-ssh1-C01 | PASS | 9.6 | 613.3 | 774.8 (Committed) | 7.3 | 16.2 / 0.2 | 3.9 |
| macos-ssh1-C02 | PASS | 9.0 | 1158.4 | 530.9 (Committed) | 7.8 | 0.1 / 0.2 | 5.2 |
| macos-ssh1-C03 | PASS | 9.0 | 958.8 | 21.2 (UpToDate) | 6.1 | 0.1 / 0.1 | 5.0 |
| macos-ssh1-C04 | PASS | 9.5 | 906.4 | 581.8 (Committed) | 7.2 | 0.1 / 0.1 | 5.0 |
| macos-ssh1-C05 | PASS | 9.5 | 958.5 | 585.0 (Committed) | 7.0 | 0.1 / 0.1 | 5.2 |
| macos-ssh1-C06 | PASS | 8.9 | 1029.1 | 6137.9 (Committed) | 12.6 | 0.1 / 0.1 | 4.9 |
| macos-ssh1-C07 | PASS | 9.3 | 2091.3 | 12321.2 (Committed) | 19.8 | 0.1 / 0.1 | 4.7 |
| macos-ssh1-C08 | PASS | 9.9 | 1061.4 | 6148.6 (Committed) | 14.2 | 0.1 / 0.1 | 4.8 |
| macos-ssh1-C09 | PASS | 11.2 | 131.2 | 4.4 (UpToDate) | 7.9 | 0.1 / 0.1 | 4.8 |
| macos-ssh1-C10 | PASS | 11.5 | 1127.1 | 158.9 (Committed) | 15.6 | 0.1 / 0.1 | 4.8 |
| macos-ssh1-C11 | PASS | 11.5 | 224.8 | 1413.7 (UpToDate) | 14.3 | 0.1 / 0.1 | 4.7 |
| macos-ssh1-C12 | PASS | 9.0 | 324.2 | 233.6 (Committed) | 6.1 | 0.1 / 0.1 | 4.7 |

### The other step-19 items

Owner direction (relayed 2026-11-02): run every other step-19 item at the
final product commit, one sample each; only R2 (no account), Windows (O5)
and Durable may stay NOT_RUN. Each ran once, in a second home made for it
(machines created there, listed below), with the same binaries as the
cells. HEAD was the docs commit `9834bce77e`; the product source is
`6837bcfce2`'s (the runner checks that nothing under `engine`, `core/crates`
or the Node package changed since). Receipts are in
[`receipts/6837bcfce2/`](receipts/6837bcfce2/). Two rows ran later, at
`f43ec45fa0`, after the fixes in the next section: the idle stop shown by
`machine list`, and the Store moved to `docker` and back. Those fixes touch
only `machine.list`/`get`/`status` and the worker's idle clock, which none
of the other rows relies on. The final product commit then moved to `e9c0e793d3`
(final review); its two fixes change only when a worker may stop and the
list probe's cleanup, so these rows hold for the same reason ("After the
final review").

| Step-19 item | Verdict | Evidence |
| --- | --- | --- |
| Two Lima machines of different sizes, each with two Stores, used at once | **PASS** | `la` (2 CPUs, 4 GiB) and `lb` (4 CPUs, 6 GiB), each with a 30 GiB data disk (`limactl list` shows Lima's 100 GiB sparse OS disk). Four cells started together: C01 and C04 on `la`, C05 and C12 on `lb` (`concurrent-*`), each with its own Store (`c01`, `c04`, `c05`, `c12`) moved native → machine → native. All four Stores were on their machines at once for 5.8 s (11:38:04.95–11:38:10.75); the four cells finished in 7 s. Each verify printed `VERIFY OK`; each Commit has its own head (`12cb0fe3…`, `120c1b4a…`, `12ff9d34…`, `12b74174…`) and its own Sandbox (`lfs-185ba2c9-c01.p-1` …). Warm-up: one throwaway Store moved to each machine and back first. |
| An agent creating and removing a machine | **PASS** | `agent-machine.txt`, through the `layerfs` command: `machine create ac2 apple-container` (10.6 s); Store `held` moved in; `machine get ac2` lists it; `machine remove ac2` refused `InUse` ("machine ac2 holds Stores [held] …", exit 125, nothing removed); one cell used it (`agent-ac2-C01`: Store `c01` in, exec, Commit, verify, back); `held` moved back and removed; `machine remove ac2` removed it (0.8 s); `container list -a` and `container volume list` empty. The first five lines' exit status was the logging pipe's, not the command's (marked in the receipt); the refusal's own exit, 125, is in the second `machine remove`. |
| A Mac with Docker unavailable: the VM kinds still work | **PASS (simulated)** | Docker was not stopped (other sessions' containers run). One cell on each VM kind with no `docker` on `PATH` and `DOCKER_HOST=unix:///nonexistent/…` (the wrapper checks `docker` is absent first): `nodocker-ac2-C01`, `nodocker-la-C01`, `nodocker-ssh1-C01`, `nodocker-msb1-C01`, `nodocker-fc1-C01`, all PASS. `native` without Docker passed earlier (`receipts/11551a8d4f/macos-native-nodocker-C01`). |
| Backup to and restore from MinIO, `backup list` | **PASS** | Throwaway MinIO `lfs533-minio2` (`minio_service.sh`): `minio-backup_family.txt`, `layerfs-api --features machine --test backup_family` 4/4 live (create, list states compared with the Store and the copy, restore, foreign copies and unrecorded links, `layerfs-exported-from`); the bucket held each test's objects afterwards, so none said NOT_RUN. `minio-remote-storage.txt`, `layerfs-remote-storage --test minio -- --include-ignored` 9/9. MinIO stopped and removed. |
| Init on `native`, moves to `docker` and one VM, and back | **PASS** | VMs: every VM-kind cell above Inits on native and moves its Store native → machine → native (`store.move` / `store.move.back` in each receipt). `docker`: `receipts/f43ec45fa0/docker-round-trip.txt`, Init on native, `store move rt docker`, a read there, `store move rt native`, the same read (root `a8c4f009…`), same heads both ways. The docker cells move out only and remove their home. |
| Linux host (`native`, `docker`) | **PASS** | The two Linux rows above, 12/12 each in a Lima QEMU VM. |
| Backup to and restore from R2 | **NOT_RUN** | No R2 account or credentials on this host. |
| Windows | **NOT_RUN** | Deferred by owner (2026-11-02): no MSVC toolchain and no Windows host to build or test on (O5); P1-28 out of 2.0's verified scope (DECISIONS). |
| Durable Store profile | **NOT_RUN** | Disabled by owner until explicit reauthorization; every Store here was Disposable. |
| Init into PostgreSQL, then `docker` and `native` Sandboxes on it | **PASS** | Throwaway `postgres:17-bookworm` (17.11) `lfs533-pg2`, one fresh database per run, a generated password (in no receipt or home). `pg-postgres_connect.txt` 1/1 (connect made the `layerfs` schema, 22 tables). `pg-sandbox_machines.txt` 3/3 live: a PostgreSQL Store's Sandboxes on `native` and `docker` each committing into the Store, and a sqlite Store's Sandbox on the docker machine. `cli-pg-native.txt` and `cli-pg-docker.txt`: connect, Init, a Sandbox, fork, mount, exec, Commit into PostgreSQL, read back (`added-lfs_native`, `added-lfs_docker`), close, no password in the home; the docker run's worker container and volume removed after. The macOS CLI and daemon for these were built with `postgresql` (a copy outside `target/debug`, so the cells' binaries stayed as they were); the sealed bundle's daemon already has it. |
| The same calls from the binary, Node and Python, compared | **PASS** | `calls-node.txt`: `node --test api/node/test/calls.mjs api/node/test/shell.mjs` 5/5 (the Node addon built at this source). `calls-python.txt`: `test_calls.py -v` 7/7 with `LAYERFS_DAEMON` set, so the live exec test ran too (the module rebuilt at this source; a first run without the daemon skipped that one test and is not kept). |
| Machines: none running while idle, shown by `machine list` | **PASS** at `f43ec45fa0` | `ac1` and `lb` had already idle-stopped before polling began at 12:23:51 (`ac1` last used 12:11, `lb` idle-stopped 12:15). The other five (`la`, `msb1`, `ssh1`, `fch`, `fc1`) stopped about 10 minutes after their last real use while `machine list` and `machine status` polled about every minute, and the list then read "stopped" for every machine (`receipts/f43ec45fa0/idle-poll.txt`, next section). Started by the first call: every VM kind's C01 at both commits. At `6837bcfce2` the list still answered recorded states, which is what the next section fixes. |

Commands that the launcher refused before running anything: the first two
`sandbox_machines` attempts asked for a 110 s test timeout; the launcher
allows at most 100 s, so no test ran. The run above used 100 s.

### After the pass: three fixes and a new bundle

Showing the idle stop through `machine list` found three defects outside
the cell path. Each was fixed with a test (DECISIONS, "Terminal pass
fixes"); the first and third are delegated owner decisions.

| Commit | Change | Production LOC |
| --- | --- | --- |
| `c89727281a` | `machine list`/`get` answer each VM and ssh machine's live state: the `machine status` probe, all at once within 10 s; it starts nothing, takes no lock and writes nothing back. A failed or late probe is `unknown`, with the reason in `error`. Before, they answered the record: `lb` idle-stopped at 11:48:23 and stayed "running" in the list. | 131767 -> 131825 (+58) |
| `1f41e6bc4f` | A worker counts as up only when it answers its secret (`{"ready":true}`), with no request sent. Before, a bare TCP accept counted: `ssh1`'s `ssh -L` forward outlived its worker, so `ssh1` read "running". | 131825 -> 131850 (+25) |
| `f43ec45fa0` | Only an authenticated request that does work (any but `worker.ping`) restarts a worker's idle clock. A connection, the secret alone, a ping, or a denied or malformed frame does not. Before, every probe postponed the idle stop. This is the guest worker, so the bundle was resealed. | 131850 -> 131852 (+2) |

**New bundle** at `f43ec45fa0`: `tools/linux_bundle.py` daemon, worker,
helper, seal; hash `c8baa5a9b660a8ee816b544e6a57dad0ccb2177408d57b77afa73884f06fc6f9`,
the same Sandbox image (`1d91885d…`, saved from `afa1ef9c…`).
`firecracker/` was moved aside during the seal and put back. The Node addon
and the CLI were rebuilt at the same commit; the macOS daemon's sources did
not change.

**Why the 108 cells at `6837bcfce2` still hold.** The first two changes
touch only `machine.list`, `machine.get` and `machine.status` on the host;
no cell step calls them. The third changes only which connections restart a
worker's idle clock. Every cell step reaches a worker as a real request,
which restarts the clock as before. Neither the data path nor the Store,
Sandbox or Workspace calls changed.

**C01 on the new bundle**, one per VM kind and ssh, receipts in
[`receipts/f43ec45fa0/`](receipts/f43ec45fa0/) (`newbundle-*-C01`). Each
guest's `/layerfs/bundle/bundle.json` read `c8baa5a9…`.

| Machine | Kind | C01 | Timed exec (ms) | `store.move` (s) |
| --- | --- | --- | ---: | ---: |
| `ac1` | `apple-container` | PASS | 480.3 | 0.1 (made on the new bundle just before) |
| `msb1` | `microsandbox` | PASS | 565.4 | 26.8 (cold start and bundle copy) |
| `la` | `lima` | PASS | 586.5 | 18.4 (cold start and bundle copy) |
| `fc1` | `firecracker` | PASS | 704.3 | 31.7 (microVM boot and bundle copy, 2 MiB huge pages) |
| `ssh1` | `ssh` | PASS | 540.4 | 21.3 (worker start and bundle copy) |

The first attempt to start `fc1` on the new bundle failed
(`fc1-warm-failed.txt`). This was a warm-up move, not a cell. `fch` had
rebooted at its idle stop and lost its hand-set huge pages, so the 2 GiB
microVM ran on ordinary memory. Copying the 2.75 GB image took 375 s, and
then the `.sealed` write failed on an ssh banner timeout. The microVM kept
running with no worker and held `fch` up. It was stopped by hand, 1,100
huge pages were set again (the declared setup), and the cell's own cold
start, copy included, then took 31.7 s. Both findings are open in
DECISIONS.

**Idle stop under polling** (`idle-poll.txt`): 11 rounds, from 12:23:51 to 12:33:40, each one
`machine list` and one `machine status` per VM machine, about a minute
apart. They did not keep anything up. `ac1` had already stopped (its C01
ended 12:11); `la` and `msb1` stopped by 12:26:40, `ssh1`'s worker by
12:27:36 (C01s about 12:16), and `fch` with `fc1` just after 12:32:27
(`fc1`'s C01 ended 12:23). Each was listed "stopped" from the next round
on; list and status always agreed. The last round lists every VM and ssh
machine stopped. Each round's list took 0.47 to 0.64 s.

### After the final review: two fixes and a new bundle

The final review of the three fixes above and their docs (DECISIONS,
"Final review fixes") found two defects in the worker and the list probe,
missing host-side tests, and three points in this file (the failure row,
the idle row and the header, all corrected above).

| Commit | Change | Production LOC |
| --- | --- | --- |
| `b15c89334d` | A worker cannot stop with an authenticated connection open: the stop decision and the admission of a connection whose secret matched take one lock, so a call admitted at the end of the idle window is read and answered, and one after the decision is refused `Unavailable` before its request is sent. Only `store.call`, `store.file`, `sandbox.host`, `host.call` and `host.backend` restart the idle clock (before: any authenticated frame but `worker.ping`). This is the guest worker. | 131852 -> 131903 (+51) |
| `e9c0e793d3` | A `machine.list`/`get` probe past its 10 s bound is killed with its process group and reaped (before: its thread and `limactl`/`ssh` child kept running). Host tests for a `Denied` reply and a silent accept. | 131903 -> 131936 (+33) |

**New bundle** at `e9c0e793d3`: `tools/linux_bundle.py` daemon, worker,
helper, seal; hash `1ac2465687733096ecfb33810e7ecb14393fedb2d653e253bc5cf145abf5e657`,
the same Sandbox image (`1d91885d…`, saved from `afa1ef9c…`). The first
seal was refused ("the source changed since these steps built: daemon"):
copying the rebuilt Node addon into the tree changed the inputs after the
daemon step, so that step was built again (0.6 s) and the seal passed. The
Node addon and the CLI were rebuilt at the same commit; the macOS daemon's
sources did not change.

**Why the 108 cells at `6837bcfce2` still hold.** Neither fix touches the
data path or the Store, Sandbox or Workspace calls. The first changes when
a worker may stop: every cell step reaches its worker as an admitted
connection carrying one of the five operations, which restarts the idle
clock as before, and a connection is now additionally held up until it
closes. The second changes only `machine.list`/`get`, which no cell step
calls.

**Setup for these runs** (none timed product work): the previous machines
had been removed, so a new home made `la`, `ac1`, `msb1`, `fch` (1,100 huge
pages set by hand, as before), `ssh1` (a new throwaway `lfs533-ssh` VM with
a new root key) and `fc1`, each by `layerfs machine create`/`connect` on the
new bundle. Firecracker's files were built again by the same recipe in the
ssh VM and placed at the bundle's `firecracker/` after the seal. `fch` was
made before they were placed, so the first `machine create fc1` was refused
with the four files missing on the host (a nested host copies them when its
worker starts); `fch` was stopped with `limactl stop`, woken by moving a
throwaway Store there and back (which copied them), its huge pages set
again, and `fc1` then made in 44 s. Each guest's
`/layerfs/bundle/bundle.json` read `1ac24656…` (`fc1` was made from `fch`'s
copy).

**C01 on the new bundle**, one per VM kind and ssh, receipts in
[`receipts/e9c0e793d3/`](receipts/e9c0e793d3/) (`newbundle-*-C01`). Every
machine was made on this bundle just before, so no move paid a cold start.

| Machine | Kind | C01 | Timed exec (ms) | `store.move` (ms) | Load |
| --- | --- | --- | ---: | ---: | ---: |
| `la` | `lima` | PASS | 595.2 | 73.1 | 7.3 |
| `ac1` | `apple-container` | PASS | 616.3 | 96.8 | 7.3 |
| `msb1` | `microsandbox` | PASS | 684.1 | 65.3 | 5.0 |
| `ssh1` | `ssh` | PASS | 607.1 | 62.0 | 4.9 |
| `fc1` | `firecracker` | PASS | 894.2 | 136.3 | 4.3 |

`ssh1`'s cell ran while `fch` was being restarted for `fc1` (declared
interference; the load is in the receipt).

**Short idle check** (`idle-la.txt`): `la`'s last real use was its C01,
which ended at 13:04:22. From 13:06:04, one `machine list` and one
`machine status la` ran about every minute (10 rounds, each list 0.36 to
0.81 s). `la` read "running" through 13:13:29 and "stopped" at 13:14:25, in
list and status alike: it stopped 10 minutes after its last request
(600 s, then the 5 s look and the guest's poweroff), and the polling did
not postpone it. The other machines were removed during the check, which
does not touch `la`.

### Samples that failed, and earlier heads

Failed samples are kept; none is replaced by a later PASS. Each led to a fix
with its own test (DECISIONS, "Terminal pass fixes"), except the
stale-worker sample, which needed only a VM stop.

| Receipt | Failure | Fixed by |
| --- | --- | --- |
| `receipts/2f5cc7f180/macos-lima1-C01` | `sandbox.create`: `/layerfs/data` not traversable by others | `005a30c93c` |
| `receipts/005a30c93c/macos-lima1-C01-old-worker` | the same; the VM still ran the old worker (before the stop-to-reload step above) | stopping the VM |
| `receipts/005a30c93c/macos-lima1-C01` | `/layerfs/data must be owned by this user and private` | `44db721d95` |
| `receipts/164e1297cb/macos-ac1-C01` | `store.move`: starting the worker failed (exit 143); the launch script killed its own reused pid | `b10dbcac8c` |
| `receipts/b10dbcac8c/linux-docker-C01` | `store.move`: "machine docker already holds a Store file named cells"; Linux `docker` ran in process | `4c740437cd` |
| `receipts/164e1297cb/linux-native-C01-userns-allowed` | `sandbox.create`: `InvalidInput`, "user namespaces are not blocked" (`ok: false`) | nothing in the product: see the note below |

Note on the last row, beside its recorded failure, which stands: it ran
before the declared setup `user.max_user_namespaces=0` was applied in the
Linux VM. `native` refusing a host where user namespaces are allowed is
what #527 requires, so this failure is the refusal the product should give
there; after the setting, Linux `native` ran 12/12. (A version of this
file at `9834bce77e` listed it as a "negative check" outside this table;
it is back here as recorded.)

A microVM that would not boot after the host rebooted (its huge pages gone)
was found by a warm-up move, not a cell; fixed by `164e1297cb`.

Full 12/12 PASS rows at earlier heads, kept as diagnostics (not the result):
`44db721d95` (lima1, ssh1, native, ac1, docker), `4ce9fb2138` (lima1, msb1,
fc1), `0170995ce4` (lima1, ssh1, ac1), `164e1297cb` (native, fc1, docker,
msb1, Linux native; lima1 C01–C06).

### Overruns of the command rules

- `docker.io` in the Linux VM: about 9 minutes, in the background.
- `limactl` and `docker` (Go) ignore `perl alarm`; the long ones (VM start,
  warm-up moves) ran in the background instead.

### What this pass does not show

- Release-build timings: every number is from a debug build.
- The other step-19 items are in the sections above; only R2, Windows and
  Durable are NOT_RUN.
- The full 12 cells on the resealed bundle (`f43ec45fa0`): one cell (C01)
  per VM kind ran there; the 108 hold for the reasons given above.
- Docker really stopped: simulated only (other sessions use it).
- Windows: deferred by owner (2026-11-02); no MSVC toolchain and no Windows
  host (O5, P1-28).

### Cleanup

After the cells, through `layerfs` in the pass's home: `store remove
warmfc`; `machine remove fc1` (10 s), `fch`, `lima1`, `ac1`, `msb1`;
`machine disconnect ssh1`. `machine remove msb1` was first refused
`Unsupported` ("msb is not on the search path", nothing changed) because
that command ran without the runner's `PATH`; with `~/.microsandbox/bin` on
`PATH` it removed the machine and its volume. Afterwards the home held only
`machine/native` and `machine/docker`, `msb list` and `msb volume list` were
empty, `container list -a` and `container volume list` were empty, and no
`lfs-0d868af2-*` Lima VM remained. Then `limactl delete -f` of the
throwaway `lfs533-linux` (with its Docker image) and `lfs533-ssh`, and
removal of the home, the bundle's `firecracker/` folder, the root key and
the scratch copies. Lima's other VMs (`lfc-*`) were not touched.
`python3 tools/cleanup.py check` afterwards: 297.7 GB free.

Second round, after the other step-19 items and the idle demo, through
`layerfs` in the second home: `machine remove la`, `lb`, `ac1`, `msb1`
(with `~/.microsandbox/bin` on `PATH`), `machine disconnect ssh1`.
`machine remove fc1` was refused `Unavailable` ("the lima machine fch is
stopped") and `machine remove fch` was refused `InUse` (it hosts `fc1`):
both had idle-stopped, removing a nested microVM needs its Lima host up,
and no call starts a stopped machine. `limactl start` of fch by hand, then
`machine remove fc1` and `fch` both succeeded. This is recorded as a gap
(HANDOFF, Not verified), not changed after the final product commit.
`ac2` had already been removed by the agent sequence. Afterwards the home
held only `machine/native` and `machine/docker`, and Lima kept only its
`lfc-*` VMs (untouched); `limactl delete -f lfs533-ssh`; `msb list`,
`msb volume list`, `container list -a` and `container volume list` were
empty; no `lfs-*` Docker container remained, and the only `lfs-` volumes
are the launcher's target and Cargo home. MinIO (`lfs533-minio2`) was
stopped and removed, PostgreSQL (`lfs533-pg2`) stopped (`--rm`), and the
docker workers of the PostgreSQL and round-trip homes removed with their
homes. The bundle's `firecracker/` folder, the second home, the throwaway
ssh key, the PostgreSQL-feature binaries and the CLI copy were removed;
the resealed bundle (`c8baa5a9…`) stays in the user cache.
`python3 tools/cleanup.py check` afterwards: 290.5 GB free.

Third round, after the final review's runs, through `layerfs` in their
home: `machine remove fc1` (11 s, while `fch` was still up), `fch`, `ac1`,
`msb1`, `machine disconnect ssh1`, and `la` after its idle check. Then
`limactl delete -f lfs533-ssh`, and removal of the bundle's
`firecracker/` folder, the home and the throwaway ssh key. Afterwards Lima
kept only its `lfc-*` VMs (untouched); `msb list`, `msb volume list`,
`container list -a` and `container volume list` were empty; no `lfs-*`
Docker container remained, and the only `lfs-` volumes are the launcher's
target and Cargo home (and another worktree's, untouched); no stand-in,
worker or Firecracker process remained. The resealed bundle (`1ac24656…`)
stays in the user cache. `python3 tools/cleanup.py check` afterwards:
291.2 GB free.

## Result (earlier runs, at `11551a8d4f` and `a487409fe5`)

| Step-19 item (README §6, step 19) | Verdict | Evidence |
| --- | --- | --- |
| The 12 #516 cells on `native` (macOS) | **PASS 12/12**, at `11551a8d4f` and again at `a487409fe5` | `macos-native-C01…C12` below |
| The 12 cells on `docker` (macOS) | **PASS 12/12** at `a487409fe5` | The Store moved to `docker`, the Sandbox started by the docker worker beside itself (private test bundle at `LAYERFS_BUNDLE`, see below). At `11551a8d4f` it was refused `Unsupported` (`receipts/11551a8d4f/macos-docker-C01.json`). |
| The 12 cells on `apple-container`, `microsandbox`, `lima`, `firecracker`, `ssh` | **NOT_RUN** | At `11551a8d4f` there was no sealed Linux bundle (O16). At `a487409fe5` a private test bundle exists (below), and `limactl` and `container` are installed, but the VM runs were not part of the owner's instruction, and creating a VM downloads and boots an OS image for longer than the 2-minute command rule allows. `msb` is not installed. No ssh host. |
| Two Lima machines of different sizes, two Stores each, at once | **NOT_RUN** | Same: no sealed Linux bundle. |
| Init on `native`, moves to `docker` and one VM, and back | **PARTLY**: Init on native and the move to `docker` PASS; the VM and the move back NOT_RUN | At `a487409fe5` every docker cell and `cli-docker-sqlite.txt` move their Store to `docker` through the real docker worker. A VM: NOT_RUN (see the VM row). Back to `native`: not run. |
| Backup to and restore from MinIO, `backup list` | **PASS** | `tests/minio_service.sh start lfs533-minio`, then `python3 tools/worktree.py cargo --resources <resources.json> --platform macos --timeout 90 -- test -p layerfs-api --features machine --test backup_family` 1/1 live (create, list states, restore, a worker Store); `python3 tools/worktree.py cargo --resources <resources.json> --platform macos --timeout 90 -- test -p layerfs-remote-storage --test minio -- --include-ignored` 9/9 (after the step-17 `from_env` move). MinIO stopped and removed after. |
| Backup to and restore from R2 | **NOT_RUN** | No R2 account or credentials on this host. |
| Init into PostgreSQL | **PASS** | Throwaway `postgres:17-bookworm` (`lfs533-pg-main`): `layerfs store connect pg postgres://postgres@127.0.0.1:…/lfs3 --password env:LFSPW` made the `layerfs` schema; `layerfs project init pg/cli <folder> --progress` in 227 ms (debug) with one progress line; `file list` and `store status` (reachable, server 17.11) answered; no password in any home file. `python3 tools/worktree.py cargo --resources <resources.json> --platform macos --timeout 90 -- test -p layerfs-api --features postgresql,machine --test postgres_connect` 1/1 on two fresh databases. Needs a build with `--features postgresql` (off by default, as in 1.1.0). |
| `docker` Sandboxes on the PostgreSQL Store | **PASS** at `a487409fe5` | `cli-pg-docker.txt`: connect, Init, a Sandbox on `docker` whose daemon dials the server as `host.docker.internal`, fork, mount, exec, Commit into PostgreSQL, read back, close; no password in the home. Also `api-sandbox_machines.txt`. At `11551a8d4f` it was refused. |
| `native` Sandboxes on the PostgreSQL Store (O3) | **PASS** at `a487409fe5` | `cli-pg-native.txt`, the same steps on `native` with a daemon built with `postgresql`. |
| A Mac with Docker stopped: `native` still works | **PASS (simulated)** | Docker was not stopped (other sessions use it). C01 on `native` ran with no `docker` on `PATH` and `DOCKER_HOST` at a missing socket: PASS (`macos-native-nodocker-C01`). The VM kinds' half is NOT_RUN (no bundle). |
| Machines: none running while idle; one started by the first call; an agent creating and removing a machine | **PARTLY** | `machine list` in a fresh home: only `native` and `docker`, no worker running. The idle stop passed in `worker_idle` (step 13, in-process worker). At `a487409fe5` the docker worker was started by the first call that needed it (`store move … docker`). Starting a VM on the first call and an agent creating and removing one: NOT_RUN. |
| The same calls from the binary, Node and Python, compared | **PASS** | At this commit: `perl -e 'alarm 100; exec @ARGV' node --test api/node/test/calls.mjs api/node/test/shell.mjs` 5/5 (all 57 calls refused identically by the command and Node; a live Store read both ways, identical JSON) and `perl -e 'alarm 100; exec @ARGV' python3 api/python/test/test_calls.py` 4/4 (all 57 calls and the live reads identical to the command; `layerfs.aio`). |
| Linux host (`native`, `docker`) | **NOT_RUN** | No Linux or ssh host is available to this pass. |
| Windows | **NOT_RUN** | Not run (O5). |

## The rerun at `a487409fe5`

Host as above; Docker 29.5.2; one sample per cell; debug builds, so times
are diagnostic only. The guest binaries come from the launcher's sealed
Linux build `build-linux-0c84a1ae4ed4` (head `a487409fe5`, features
`layerfs-daemon/postgresql`), copied with a hand-written `bundle.json` into
a private folder named by `LAYERFS_BUNDLE`; nothing was placed in the user
cache. The docker machine's image was the local Sandbox image
`sha256:afa1ef9c…f38e`. Each docker cell removed its own worker container
and data volume. The Node package was run with the addon rebuilt at the
commit. A first `macos-docker-C01` attempt failed at `store.move` ("the
docker machine's worker container needs an image") because the package
loaded a stale addon from before `a487409fe5`; a second attempt was killed
by macOS because the new addon was copied over the loaded file. Both were
harness setup faults, not product outcomes; the receipt path was reused by
the run below, so their receipts are not kept.

| Receipt | Verdict | Mount | Exec | Commit (kind) | Unmount | Load |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| macos-docker-C01 | PASS | 14.4 | 682.2 | 552.1 (Committed) | 9.4 | 4.7 |
| macos-docker-C02 | PASS | 18.1 | 1246.8 | 553.8 (Committed) | 11.2 | 4.1 |
| macos-docker-C03 | PASS | 13.4 | 929.4 | 26.4 (UpToDate) | 8.0 | 4.3 |
| macos-docker-C04 | PASS | 14.6 | 975.5 | 604.0 (Committed) | 9.1 | 4.1 |
| macos-docker-C05 | PASS | 14.5 | 987.0 | 596.2 (Committed) | 9.9 | 3.9 |
| macos-docker-C06 | PASS | 15.9 | 1074.5 | 5180.9 (Committed) | 19.3 | 3.7 |
| macos-docker-C07 | PASS | 18.8 | 2084.9 | 10337.1 (Committed) | 25.5 | 4.1 |
| macos-docker-C08 | PASS | 16.9 | 1102.3 | 5192.1 (Committed) | 16.8 | 3.6 |
| macos-docker-C09 | PASS | 21.9 | 147.2 | 7.4 (UpToDate) | 10.8 | 4.1 |
| macos-docker-C10 | PASS | 21.6 | 1135.6 | 171.3 (Committed) | 21.0 | 4.5 |
| macos-docker-C11 | PASS | 21.4 | 259.7 | 1460.6 (UpToDate) | 18.0 | 4.5 |
| macos-docker-C12 | PASS | 14.4 | 381.1 | 247.3 (Committed) | 9.2 | 4.4 |
| macos-native-C01 | PASS | 25.6 | 1534.8 | 674.3 (Committed) | 47.8 | 3.8 |
| macos-native-C02 | PASS | 26.4 | 7526.2 | 672.6 (Committed) | 49.1 | 3.8 |
| macos-native-C03 | PASS | 26.1 | 2656.3 | 136.1 (UpToDate) | 25.5 | 3.1 |
| macos-native-C04 | PASS | 26.6 | 5503.6 | 703.1 (Committed) | 49.8 | 2.9 |
| macos-native-C05 | PASS | 26.8 | 5521.4 | 707.4 (Committed) | 44.4 | 3.3 |
| macos-native-C06 | PASS | 25.9 | 1762.9 | 5551.5 (Committed) | 26.6 | 3.0 |
| macos-native-C07 | PASS | 26.1 | 3471.3 | 10937.9 (Committed) | 32.7 | 3.1 |
| macos-native-C08 | PASS | 28.6 | 1768.4 | 5528.5 (Committed) | 30.4 | 2.9 |
| macos-native-C09 | PASS | 28.9 | 97.3 | 117.9 (UpToDate) | 26.4 | 2.7 |
| macos-native-C10 | PASS | 29.7 | 1851.5 | 317.1 (Committed) | 23.1 | 2.9 |
| macos-native-C11 | PASS | 25.5 | 654.5 | 1588.3 (UpToDate) | 27.3 | 3.0 |
| macos-native-C12 | PASS | 28.7 | 763.4 | 351.3 (Committed) | 31.8 | 2.9 |

Each cell ran as one command (`$S` is the private bundle's folder):

```bash
cd api/node && LFS533_DAEMON=$PWD/../../target/debug/layerfs-daemon \
  LFS533_IMAGE=sha256:afa1ef9c27294f9c7298820c06483e6fb1ddafc43a7cc7d6dec8cd2e67abf38e \
  LAYERFS_BUNDLE=$S/bundle perl -e 'alarm 100; exec @ARGV' \
  node test/verify/cells/cell.mjs C01 docker ../../core/docs/issues/533/receipts/a487409fe5/macos-docker-C01.json
```

Beside the cells, at the same commit:

- `api-sandbox_machines.txt`: `cargo test -p layerfs-api --features
  machine,postgresql --test sandbox_machines`, 3/3 live (PostgreSQL
  Sandboxes on `native` and `docker`, a sqlite Store's Sandbox on the
  docker machine, the refusals).
- `cli-pg-native.txt`, `cli-pg-docker.txt`, `cli-docker-sqlite.txt`: the
  same paths through the `layerfs` command, one fresh home and database
  each.

Afterwards no `lfs-*` container or worker volume and no test home remained;
the PostgreSQL container was stopped (`--rm`).

## The twelve cells on `native` at `11551a8d4f`

Times in ms (debug build), from each receipt's timed phase. Commit kinds match
#529's `mac` row: C03, C09 and C11 are `UpToDate`.

| Receipt | Verdict | Mount | Exec | Commit (kind) | Unmount | Load |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| macos-native-C01 | PASS | 25.0 | 1437.8 | 681.8 (Committed) | 47.5 | 2.9 |
| macos-native-C02 | PASS | 33.0 | 7418.9 | 661.0 (Committed) | 47.0 | 2.9 |
| macos-native-C03 | PASS | 26.0 | 2768.9 | 98.9 (UpToDate) | 23.5 | 2.9 |
| macos-native-C04 | PASS | 26.1 | 4946.4 | 692.7 (Committed) | 42.5 | 2.9 |
| macos-native-C05 | PASS | 31.2 | 5086.4 | 695.1 (Committed) | 50.9 | 4.2 |
| macos-native-C06 | PASS | 26.2 | 1756.9 | 5633.2 (Committed) | 26.3 | 4.1 |
| macos-native-C07 | PASS | 32.7 | 3501.0 | 11060.9 (Committed) | 28.5 | 3.8 |
| macos-native-C08 | PASS | 31.1 | 1788.5 | 5616.3 (Committed) | 31.4 | 4.0 |
| macos-native-C09 | PASS | 25.5 | 93.0 | 114.4 (UpToDate) | 30.6 | 3.8 |
| macos-native-C10 | PASS | 33.0 | 1895.3 | 314.6 (Committed) | 26.9 | 3.6 |
| macos-native-C11 | PASS | 31.7 | 670.9 | 1578.5 (UpToDate) | 26.4 | 3.8 |
| macos-native-C12 | PASS | 25.5 | 769.5 | 357.8 (Committed) | 27.5 | 3.8 |
| macos-native-nodocker-C01 | PASS | 26.2 | 1482.3 | 679.9 (Committed) | 53.6 | 5.0 |
| macos-docker-C01 | FAIL (refused at `sandbox.create`) | – | – | – | – | 3.3 |

Each cell ran as one command in a fresh home:

```bash
cd api/node && LFS533_DAEMON=$PWD/../../target/debug/layerfs-daemon \
  perl -e 'alarm 100; exec @ARGV' node test/verify/cells/cell.mjs C01 native ../../core/docs/issues/533/receipts/11551a8d4f/macos-native-C01.json
```

After the cells: no NFS mount and no `layerfs-daemon` process remained.

## What this pass does not show

- Any VM kind or ssh: none ran. The docker machine's worker ran at
  `a487409fe5` from a private test bundle; no installer places a bundle in
  the user cache yet (O16).
- Anything on Linux: `remote_store` and the namespaces backend are Linux-only
  and did not build or run here.
- Release-build timings: every number above is from a debug build.
