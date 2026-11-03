# #535 G7 live receipt: PostgreSQL host reach from lima, apple-container, microsandbox (firecracker simulated)

- **When:** 2026-11-03 10:19–10:30 +08 (02:19–02:30 UTC)
- **Host:** macOS 26.4.1 (25E253), arm64, Apple M3 Max; application firewall disabled (`socketfilterfw --getglobalstate` → State = 0)
- **Tools:** limactl 2.2.1 · container CLI 1.6.1 (vmnet plugin `container-network-vmnet`, network `default` 192.168.64.0/24, kernel kata 6.18.35) · msb 0.7.11 (`~/.microsandbox/bin/msb`, not on PATH) · Docker Desktop 29.5.2 (context `desktop-linux`)
- **Disk:** 233.96 GB free at start, 219.90 GB at end (`tools/cleanup.py check`, threshold 100 GB)
- **Product code:** none written or run. Guests were created by hand with the flags in
  `connect/crates/layerfs-machine/src/platforms/{lima/template.rs,lima/create.rs,apple_container/create.rs,microsandbox/create.rs}`
  (worktree `layerfs-issue535-connect`). Every resource was named `g7probe-*`.
- **Level of the reach check:** protocol level, not LayerFS's schema-binding read. Each probe sends a PostgreSQL
  SSLRequest (server answers one byte, `N`) and a v3 StartupMessage `user=postgres database=g7probe` (server answers
  `R` + AuthenticationSASL `SCRAM-SHA-256`). That proves a PostgreSQL server is reached; it does not authenticate or
  read the schema (no `psql` in the guests; an `apt-get install postgresql-client` in the lima guest stalled and was
  stopped, `lima-apt.log`).

## Host listeners

| Port | What | Bound on host | Source |
| --- | --- | --- | --- |
| 55432 | PostgreSQL 17.11 (`postgres:17-bookworm` sha256:639ab7ceb90e, container `g7probe-pg`) | **127.0.0.1 only** (Docker Desktop publish) | `host-pg-run.log` |
| 55433 | same server | 0.0.0.0 | `host-pg-run.log` |
| 55442 | host-native Python TCP listener (`listen.py`, answers `G7OK <bind>`) | **127.0.0.1 only** | `host-listener.log` |
| 55443 | same listener | 0.0.0.0 | `host-listener.log` |

`lsof` confirmed the binds (`127.0.0.1:55432`, `*:55433`, `127.0.0.1:55442`, `*:55443`). The Python listener is a
non-Docker loopback server, so results do not depend on Docker Desktop's port proxy; both behaved the same everywhere.
`host-listener.log` records the peer address of every accepted connection.

Probe scripts: `probe.sh` (DNS/hosts/routes, SSLRequest to 55432/55433, line read from 55442/55443),
`pgstart.sh` (StartupMessage).

## lima — PASS (loopback-only reachable)

Created as `lima/create.rs` does: `limactl disk create g7probe-lima-data --size 1024MiB --format raw`,
`limactl create --name g7probe-lima --tty=false - < lima-template.yaml` (vz, Ubuntu 24.04 cloud image, `mounts: []`,
`containerd` off, the additional data disk, worker port forward 7070→127.0.0.1:55450, all other guest ports ignored),
`limactl start --timeout 5m`. Default (user-mode) networking, as the LayerFS template has no `networks` key.

Guest facts (`lima-probe.log`):
```
eth0 192.168.5.15/24; default via 192.168.5.2 dev eth0
/etc/hosts: 192.168.5.2	host.lima.internal
getent hosts host.lima.internal: 192.168.5.2     host.lima.internal
```
Reach (`lima-probe.log`, `lima-pgstart.log`):
```
PG SSLRequest host.lima.internal:55432 -> N          (127.0.0.1-only PostgreSQL)
PG SSLRequest host.lima.internal:55433 -> N
host-native listener host.lima.internal:55442 -> G7OK 127.0.0.1:55442   (127.0.0.1-only)
host-native listener host.lima.internal:55443 -> G7OK 0.0.0.0:55443
PG startup host.lima.internal:55432 -> R........SCRAM-SHA-256..
PG startup host.lima.internal:55433 -> R........SCRAM-SHA-256..
PG startup 192.168.5.2:55432 -> R........SCRAM-SHA-256..
PG startup 127.0.0.1:55432 -> Connection refused     (control: guest loopback is not the host)
```
The host saw these connections arrive from `127.0.0.1` (`host-listener.log` 10:21:49): Lima's host agent dials the
host loopback, so `pg_hba` sees a local client. (A first StartupMessage attempt returned rc=124 because the probe's
reader waited for 64 bytes; fixed to a single `dd` read and rerun once. The SSLRequest results were already positive.)
Side fact: `limactl create` itself rewrites the environment's `HTTPS_PROXY=http://127.0.0.1:15236` to
`http://192.168.5.2:15236` in the guest (`lima-create.log`), the same loopback→`192.168.5.2` mapping.

**Conclusion:** alias `host.lima.internal` (= 192.168.5.2, also the default gateway); a host listener bound only to
127.0.0.1 IS reachable through it. `reach`: loopback → `host.lima.internal` (as `lima/adapter.rs` has) is confirmed for
the LayerFS template's default networking.

## apple-container — loopback-only NOT reachable; gateway reaches 0.0.0.0 listeners

Created as `apple_container/create.rs` does: `container volume create -s 1024M g7probe-ac-data`, then
`container run --detach --name g7probe-ac --cpus 2 --memory 1024M --cap-add SYS_ADMIN --cap-add SYS_RESOURCE
--cap-add NET_ADMIN --mount type=volume,source=g7probe-ac-data,target=/layerfs/data --publish 127.0.0.1:55451:7070
--label layerfs.role=g7probe debian:trixie-slim sh -c "trap 'exit 0' TERM; sleep infinity & wait"` (`ac-create.log`).

Guest facts (`ac-probe.log`, `ac-inspect.json`, `ac-network-inspect.json`):
```
IP 192.168.64.10/24; /proc/net/route default gateway 0140A8C0 = 192.168.64.1
/etc/hosts: 127.0.0.1 localhost / 192.168.64.10 g7probe-ac       (no host.* entry)
/etc/resolv.conf: nameserver 192.168.64.1
getent hosts host.container.internal / host.docker.internal / host.containers.internal / host.lima.internal: no answer
container inspect: networks[0].ipv4Gateway = "192.168.64.1", network "default", mode nat, plugin container-network-vmnet
host side: bridge100 inet 192.168.64.1
```
Reach:
```
PG SSLRequest 192.168.64.1:55432 -> Connection refused   (127.0.0.1-only PostgreSQL)
PG SSLRequest 192.168.64.1:55433 -> N
host-native listener 192.168.64.1:55442 -> Connection refused   (127.0.0.1-only)
host-native listener 192.168.64.1:55443 -> G7OK 0.0.0.0:55443
PG startup 192.168.64.1:55432 -> Connection refused
PG startup 192.168.64.1:55433 -> R........SCRAM-SHA-256..
```
The host saw the 55443 connection from `192.168.64.10` (the guest's own address), so `pg_hba` must admit
192.168.64.0/24 for a server reached this way.

NOT_RUN: `container system dns create <domain> --localhost <ip>` (the CLI's own mechanism to redirect an IP to the
host's localhost) — "must run as an administrator"; this lane does not use sudo or change system settings.

**Conclusion:** no host alias exists by default. The gateway `192.168.64.1` (read from `container inspect`
`networks[0].ipv4Gateway`, not hard-coded: it is the vmnet network's gateway) reaches host servers bound to
0.0.0.0 / the bridge, but a server bound only to 127.0.0.1 is NOT reachable. `reach` options for the owner:
(a) keep refusing loopback URLs on apple-container with a reason naming this fact; or (b) rewrite loopback → the
inspected `ipv4Gateway`, letting the reach check refuse when the server listens on loopback only (refusal should say
the server must listen on the vmnet bridge address and admit 192.168.64.0/24). Option (c), an admin-created
`--localhost` DNS domain, is untested here.

## microsandbox — alias `host.microsandbox.internal`; host blocked by msb's default egress policy

`msb` 0.7.11 was present at `~/.microsandbox/bin/msb` (not on PATH), so this ran.

### As LayerFS creates it today (no network flags) — host NOT reachable
`msb create --name g7probe-msb --cpus 2 --memory 1024M --mount-named g7probe-msb-data:/layerfs/data:kind=disk,size=1024M
-p 127.0.0.1:55452:7070 --label layerfs.role=g7probe debian:trixie-slim` (`msb-create.log`).
```
eth0 172.16.0.6/30; default gateway 050010AC = 172.16.0.5
/etc/hosts: 172.16.0.5	host.microsandbox.internal ; resolv.conf nameserver 172.16.0.5
host.microsandbox.internal:55432/55433/55442/55443 -> Connection refused (all four)
PG startup 172.16.0.5:55433 / 192.168.2.123:55433 (host LAN IP) -> Connection refused
public control: 1.1.1.1:53 connect OK
```
`msb create --help`: "--net-default-egress … Default: deny (with an implicit allow@public rule when no other rules are
present)". So the host (and private addresses) are denied by policy, not unreachable by routing.

### With `--net public,host` — loopback-only reachable
`msb create --name g7probe-msb2 … --net public,host -p 127.0.0.1:55453:7070 … debian:trixie-slim` (`msb2-*.log`):
```
getent hosts host.microsandbox.internal: 172.16.0.9      (per-sandbox /30 gateway; differs per sandbox)
PG SSLRequest host.microsandbox.internal:55432 -> N      (127.0.0.1-only PostgreSQL)
PG SSLRequest host.microsandbox.internal:55433 -> N
host-native listener host.microsandbox.internal:55442 -> G7OK 127.0.0.1:55442
host-native listener host.microsandbox.internal:55443 -> G7OK 0.0.0.0:55443
PG startup host.microsandbox.internal:55432 -> R........SCRAM-SHA-256..
```
Host saw peers as `127.0.0.1` (10:27:14). Inspected policy (`msb2-inspect.json`): default_egress deny; allow
group `host` (all ports), allow group `public`, allow `host` 53 udp/tcp.

### With a per-port rule — only that port
`msb create --name g7probe-msb3 … --net-rule allow@public --net-rule allow@host:tcp:55432 …` (`msb3-*.log`):
`host.microsandbox.internal:55432 -> R…SCRAM-SHA-256`, `:55433 -> Connection refused`, public 1.1.1.1:53 OK.

`msb modify` has no network options (only cpus/memory/disk compaction), so an existing microsandbox machine cannot
gain host access without being re-created.

**Conclusion:** alias `host.microsandbox.internal` (use the name: the address is a per-sandbox /30 gateway).
A 127.0.0.1-only host server IS reachable through it **only if the sandbox is created with host egress allowed**;
with LayerFS's current create flags it is not reachable at all. `reach`: loopback → `host.microsandbox.internal`,
plus `microsandbox/create.rs` must add `--net public,host` (a per-port `allow@host:tcp:<port>` rule is narrower but is
fixed at create, while G3 lets any Store be used later). `--net public,host` opens every host port to the guest:
an owner call. Machines created before that change need re-creation.

## firecracker — NOT_RUN (simulated L3 path only)

NOT_RUN: no Firecracker binary, kernel or rootfs on this host (`~/.layerfs/firecracker` empty; no `vmlinux`/
`rootfs.ext4` under the LayerFS worktrees). Building them needs downloads and a nested Lima host; skipped.

Simulation (`fcsim.sh`, `fcsim-run.log`; **not Firecracker**): a fresh Lima VM `g7probe-fcsim` (same template), a
network namespace standing in for the microVM, a veth for its tap, addressed exactly as `firecracker/scripts.rs` does
for n=1 (host 172.30.1.1/30, guest 172.30.1.2, default via 172.30.1.1).
```
ip_forward before: 0
A. as built today (no NAT):  172.30.1.1:55432 refused; 192.168.5.2:55432/55433/55442 timeout; 1.1.1.1:53 timeout
B. ip_forward=1 + iptables -t nat -A POSTROUTING -s 172.30.0.0/16 ! -d 172.30.0.0/16 -j MASQUERADE:
   172.30.1.1:55432 refused; 192.168.5.2:55432 N; 192.168.5.2:55433 N; 192.168.5.2:55442 G7OK 127.0.0.1:55442; 1.1.1.1:53 connect OK
C. B + DNAT -i g7fc-h -d 172.30.1.1 --dport 55432 -> 192.168.5.2:55432:  172.30.1.1:55432 N
```
**Finding (simulated):** `172.30.<n>.1` is the Lima host's own tap address. A loopback URL rewritten to it reaches a
server running *on the Lima host*, not the Mac's loopback, even with the planned masquerade. With masquerade +
ip_forward, the microVM reaches the Mac's loopback at `192.168.5.2` (Lima's `host.lima.internal` address; the microVM's
rootfs is not expected to carry that hosts entry — unverified) and the public internet. So either `reach` for
firecracker rewrites loopback → `192.168.5.2` (the Lima host's resolved `host.lima.internal`) after NAT, or the NAT
step also needs a DNAT per server port on `172.30.<n>.1`. A real Firecracker receipt is still required for G7.

## Cleanup

Removed: Lima instances `g7probe-lima`, `g7probe-fcsim` and disks `g7probe-lima-data`, `g7probe-fcsim-data`;
apple container `g7probe-ac` and volume `g7probe-ac-data`; microsandboxes `g7probe-msb`, `g7probe-msb2`,
`g7probe-msb3` and volume `g7probe-msb-data`; Docker container `g7probe-pg`; the Python listener (pid 18376).
Verified (`final-cleanup.log`): `limactl list` shows only `lfc-linux`, `lfc-ssh`, `lfc-vzkvm` (Stopped, untouched);
`limactl disk list` only `lfcssh-store`; `container list --all` and `container volume list` empty;
`msb list` / `msb volume list` empty; no `g7probe` Docker container; no listener on 55432/55433/55442/55443.

**Left behind (1 item):** `docker rm -f g7probe-pg` did not remove the image's anonymous data volume
`ae17ce8d6d4f5beec46e7c0ecf85b8d314294d29a131220e00f2729f18d03588` (created 2026-11-03T02:20:42Z, the
container's start second; label `com.docker.volume.anonymous`; holds only the throwaway PostgreSQL data). Removing it
was blocked by the session's permission classifier; remove with `docker volume rm ae17ce8d…88`.
Existing VMs, containers and sessions' resources were not touched (`layerfs-postgres` on 5432 left running).
