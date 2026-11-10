#!/bin/zsh
# Creates a workbench: one long-running Linux container in OrbStack (Docker)
# that holds the LayerFS Stores, their daemons and the agents. Agents run as
# uid 501 (`agent`); the LayerFS control (`lfs`) and the daemons run as root.
#
# usage: tools/orbstack/lfs-vm.sh <bundle-dir> [lfs-orb|lfs-foxway]
#   <bundle-dir> holds layerfs-daemon and layerfs.node (process-e2e.sh makes it).
#   Run it in a shell proxied through the workbench's vault:
#   `agent-vault-run shared -- tools/orbstack/lfs-vm.sh <bundle-dir>` for `lfs-orb`,
#   `agent-vault-run foxway-workspace-dev -- tools/orbstack/lfs-vm.sh <bundle-dir> lfs-foxway`.
#
# Two workbenches, so Foxway files and credentials never share a container
# with personal work:
#   lfs-orb      personal repos: vault `shared`, SSH to github.com and
#                gitlab.com, the shared services, volume `lfs-data`, port 2223.
#   lfs-foxway   Foxway repos: a `foxway*` vault, SSH to ssh.dev.azure.com
#                only, no shared services, no /share, only the Foxway
#                summaries of the vault, volume `lfs-foxway-data`, port 2224. Its Sandboxes are
#                foxway-<project>-<n>, so a Sandbox path names its workbench
#                (memoryd and lfs-bridge file Foxway sessions by it).
# The rest of this text describes `lfs-orb`; `lfs-foxway` is the same except
# for those settings.
#
# Lasting state lives on the named volume `lfs-data` (made once), mounted at
# /var/lib/layerfs-data: the LayerFS home and Stores, and the agent home with
# its agent harnesses and logins. Recreating the container keeps them;
# Sandboxes (daemons, Overlays, mounts) start fresh and are reclaimed.
#
# A shared folder, read-write both ways: $LFS_SHARE (default ~/lfs-share) on
# the Mac is /share in the VM. Agents can write there, so treat its files as
# untrusted on the Mac.
#
# Parts of the fleet-memory Obsidian vault ($LFS_VAULT, default
# ~/vaults/fleet-memory), at /vaults/fleet-memory and, for agents, at
# ~/vaults as on the Mac: features, diagrams and this year's summaries
# read-write, router read-only. The generated memory pages stay out (agents
# read them with `mac mem`), and so do summaries/foxway and the rest of
# summaries: Foxway data must not reach the devin harness in the VM.
# Each year's summaries folder is its own mount: at a new year, add it and
# recreate. Removal condition: Foxway summaries move out of summaries/.
#
# The memoryd daemon runs in `lfs-orb` (memoryd ADR 0069). It reads these
# Mac folders read-only at their Mac paths, so its folder keys match the
# Mac's: the transcript roots of ~/.config/memoryd/config.json that have no
# `host`, the pull mirror ~/.local/share/memoryd/remote, and ~/repos.
# Agents can read them too, Foxway transcripts and repositories included:
# the operator accepted that on 2026-11-10. A new root needs a recreate.
#
# Agents may use DNS and HTTPS to any host, the Agent Vault proxy, the Mac
# bridge, and SSH of github.com and gitlab.com (iptables rules for uid 501). Root is not limited. vm-boot.sh applies the
# rule and starts sshd at each container start.
#
# The base is node:24-bookworm-slim; agents cannot install apt packages, so
# every package they need is listed here. Docs, man pages and translations
# are not installed (/etc/dpkg/dpkg.cfg.d/lfs-vm), nor recommended packages.
# iproute2 gives `ss`, which Herdr UI uses to list the ports agents listen on.
# rsync and zstd let memoryd on the Mac pull agent transcripts over SSH.
# clang (14 or newer) lets Bend build native binaries (`bend <file> -o <out>`).
# libsecret-1-0 is loaded by npm postinstall steps (uniwind-pro in praze).
# The base tools line holds what the full node image gave and agents use:
# Git, SSH, curl and archive tools, and the headers that native npm, Python
# and Rust builds and mise's source builds link (OpenSSL, libffi, zlib,
# SQLite, libpq, readline, libyaml, bzip2, xz, libxml2, libcurl).
# The next line holds what the active repos' builds and tests need:
# procps and build-essential (praze, yada, Rust crates), sqlite3
# (herdr-blackboard), python3-venv (dreamd, herdr-loom), postgresql-client
# (turtlr, infra-kit; Debian's 15), ffmpeg (dearday, praze-landing), and
# chromium, fonts-noto-core and xvfb for browser tests (jev-hand, effect-folio,
# pilotprospector, praze-landing). ripgrep, jq and fd-find (as `fd`) are
# the search tools agents expect.
#
# The shared services of services.compose.yml that every agent uses on
# 127.0.0.1: Postgres, Redis and RustFS. OrbStack runs them as sibling
# containers on the Docker network `lfs-orb` (services.orb.yml), and socat
# forwards each port to the workbench's loopback. No Docker runs inside
# `lfs-orb`.
#
# sshd takes key logins for `agent` only, on 127.0.0.1:$LFS_VM_SSH_PORT
# (default 2223) of the Mac. Put public keys in /home/agent/.ssh/authorized_keys;
# the host keys live on the volume, so they survive a recreate.
#
# Agents run their own Docker (ADR 0005): rootless dockerd as `agent`, which
# vm-boot.sh starts, with its data in the agent home. Its containers see the
# Workspaces and publish ports on the workbench's loopback, and root in them
# is uid 501 outside. It needs /dev/net/tun, a writable /proc/sys and the
# seccomp profile seccomp.json (seccomp.sh writes it).
#
# The Herdr plugin joshka.lfs-bridge (herdr-plugins) makes this container the
# Herdr machine `lfs orb` and installs the agent harnesses.
set -eu
repo=${0:A:h:h:h}
bundle=${1:?usage: lfs-vm.sh <bundle-dir>}
sfw_package=$(dirname "$(dirname "$(readlink -f "$(whence -p sfw)")")")
name=${2:-lfs-orb}
vault=${LFS_VAULT:-$HOME/vaults/fleet-memory}
year=$(date +%Y)
# OrbStack's VM has 16 GB unless its memory_mib setting says otherwise.
case $name in
  lfs-orb)
    [[ ${AGENT_VAULT_VAULT:-} != foxway* ]] || { echo "lfs-vm: lfs-orb never uses a Foxway vault" >&2; exit 64; }
    volume=lfs-data ssh_port=${LFS_VM_SSH_PORT:-2223} memory=${LFS_VM_MEMORY:-12G}
    git_ssh_hosts="github.com gitlab.com" sandbox_prefix=layerfs-
    service_forwards="5432:postgres:5432 6379:redis:6379 9000:rustfs:9000"
    mkdir -p "$vault/summaries/$year"
    share=${LFS_SHARE:-$HOME/lfs-share}
    mkdir -p "$share"
    vault_mounts=(-v "$share":/share -v "$vault/features":/vaults/fleet-memory/features -v "$vault/diagrams":/vaults/fleet-memory/diagrams
      -v "$vault/summaries/$year":"/vaults/fleet-memory/summaries/$year"
      -v "$vault/router":/vaults/fleet-memory/router:ro)
    memoryd_config=${MEMORYD_CONFIG:-$HOME/.config/memoryd/config.json}
    [[ -f $memoryd_config ]] || { echo "lfs-vm: lfs-orb needs the memoryd config $memoryd_config" >&2; exit 64; }
    memoryd_dirs=(${(f)"$(python3 -I -c 'import json, sys
for r in json.load(open(sys.argv[1]))["roots"]:
    if "host" not in r: print(r["path"])' "$memoryd_config")"} "$HOME/.local/share/memoryd/remote" "$HOME/repos")
    memoryd_mounts=()
    for dir in $memoryd_dirs; do
      [[ -d $dir ]] && memoryd_mounts+=(-v "${dir}:${dir}:ro")
    done ;;
  lfs-foxway)
    [[ ${AGENT_VAULT_VAULT:-} == foxway* ]] || { echo "lfs-vm: lfs-foxway needs a Foxway vault (agent-vault-run foxway-workspace-dev)" >&2; exit 64; }
    volume=lfs-foxway-data ssh_port=${LFS_VM_SSH_PORT:-2224} memory=${LFS_VM_MEMORY:-8G}
    git_ssh_hosts="ssh.dev.azure.com" sandbox_prefix=foxway-
    service_forwards=""
    mkdir -p "$vault/summaries/foxway"
    vault_mounts=(-v "$vault/summaries/foxway":/vaults/fleet-memory/summaries/foxway)
    memoryd_mounts=() ;;
  *) echo "usage: lfs-vm.sh <bundle-dir> [lfs-orb|lfs-foxway]" >&2; exit 64 ;;
esac

# The docker commands must not inherit an agent's proxy settings. Keep only
# the proxy's host and port.
proxy_url=${HTTPS_PROXY:?the Agent Vault proxy (HTTPS_PROXY) is not set}
proxy=${proxy_url#*@}
proxy_host=${proxy%%:*}
proxy_port=${${proxy#*:}%%/*}
unset HTTP_PROXY HTTPS_PROXY http_proxy https_proxy NO_PROXY no_proxy ALL_PROXY all_proxy
proxy_ip=$(dscacheutil -q host -a name "$proxy_host" | awk '/ip_address/ {print $2; exit}')
bridge_port=${LFS_BRIDGE_PORT:-47111}
docker volume inspect $volume >/dev/null 2>&1 || docker volume create $volume >/dev/null
docker network inspect $name >/dev/null 2>&1 || docker network create $name >/dev/null
# Where the workbench reaches the Mac (the bridge): OrbStack's
# host.docker.internal, the Mac's loopback.
gateway=0.250.250.254
# The proxy's tailnet address, in the hosts file Docker keeps across restarts.
# LayerFS Sandboxes need a real filesystem: on OrbStack's overlay root, a
# commit finds no changes. An anonymous volume holds them; `docker rm -v`
# removes it, so a recreate starts them fresh.
flags=(--device /dev/fuse --device /dev/net/tun --security-opt systempaths=unconfined
  --security-opt "seccomp=$repo/tools/orbstack/seccomp.json" --network $name --restart unless-stopped --add-host "$proxy_host:$proxy_ip"
  -v /var/lib/layerfs)

docker run -d --init --name $name --cpus 10 --memory $memory --cap-add CAP_SYS_ADMIN --cap-add CAP_NET_ADMIN --cap-add CAP_SYS_PTRACE \
  $flags -p "127.0.0.1:$ssh_port:22" -v $volume:/var/lib/layerfs-data \
  -v "$bundle":/opt/layerfs/bundle:ro -v "$repo/sdk/js":/opt/layerfs/sdk:ro \
  -v "$repo/tools/orbstack":/opt/layerfs/tools:ro $vault_mounts $memoryd_mounts \
  node:24-bookworm-slim /opt/layerfs/tools/vm-boot.sh run

# The Agent Vault proxy: its CA and its tailnet address (the VM's DNS has no
# MagicDNS). The proxy URL itself goes to each agent (lfs-bridge agent).
docker exec -i $name sh -c 'cat > /opt/agent-vault-ca.pem && chmod 644 /opt/agent-vault-ca.pem' \
  < "${CURL_CA_BUNDLE:?the Agent Vault CA (CURL_CA_BUNDLE) is not set}"
# sfw for agents' package installs (it caches its firewall binary beside itself).
docker exec $name mkdir -p /opt/sfw/dist /opt/sfw/.sfw-cache
docker exec -i $name sh -c 'cat > /opt/sfw/dist/sfw.mjs' < "$sfw_package/dist/sfw.mjs"
docker exec -i $name sh -c 'cat > /opt/sfw/package.json' < "$sfw_package/package.json"

docker exec -i -e PROXY_IP="$proxy_ip" -e PROXY_PORT="$proxy_port" \
  -e GATEWAY="$gateway" -e BRIDGE_PORT="$bridge_port" -e GIT_SSH_HOSTS="$git_ssh_hosts" \
  -e SERVICE_FORWARDS="$service_forwards" -e SANDBOX_PREFIX="$sandbox_prefix" $name bash -s <<'SETUP'
set -eu
# OrbStack's docker exec starts with umask 0000.
umask 022
data=/var/lib/layerfs-data
mkdir -p "$data/home" "$data/stores" "$data/agent-home" "$data/ssh-host-keys" /etc/lfs-vm
chmod 700 "$data/ssh-host-keys"
# The sibling services, forwarded to loopback (<port>:<host>:<port>).
printf 'PROXY_IP=%s\nPROXY_PORT=%s\nGATEWAY=%s\nBRIDGE_PORT=%s\nGIT_SSH_HOSTS="%s"\nSERVICE_FORWARDS="%s"\n' \
  "$PROXY_IP" "$PROXY_PORT" "$GATEWAY" "$BRIDGE_PORT" "$GIT_SSH_HOSTS" "$SERVICE_FORWARDS" > /etc/lfs-vm/boot.env
chmod 700 "$data/home"
ln -sfn "$data/agent-home" /home/agent
# The Mac's ~/vaults/wiki/summaries is a link to the fleet-memory summaries.
mkdir -p /vaults/wiki
ln -sfn /vaults/fleet-memory/summaries /vaults/wiki/summaries
ln -sfn /vaults /home/agent/vaults
id agent >/dev/null 2>&1 || useradd -u 501 -g 20 -M -d /home/agent -s /bin/bash agent
chown 501:20 "$data/agent-home" /opt/sfw/.sfw-cache
printf '#!/bin/sh\nexec node /opt/sfw/dist/sfw.mjs "$@"\n' > /usr/local/bin/sfw
cat > /usr/local/bin/lfs <<'LFS'
#!/bin/sh
export LAYERFS_RUNTIME=process LAYERFS_HOME=/var/lib/layerfs-data/home \
  LAYERFS_DAEMON=/opt/layerfs/bundle/layerfs-daemon LAYERFS_NATIVE=/opt/layerfs/bundle/layerfs.node \
  LAYERFS_STORE_ROOT=/var/lib/layerfs-data/stores LAYERFS_CONTAINER_PREFIX=@SANDBOX_PREFIX@
exec node /opt/layerfs/tools/lfs.mjs "$@"
LFS
sed -i "s|@SANDBOX_PREFIX@|$SANDBOX_PREFIX|" /usr/local/bin/lfs
chmod 755 /usr/local/bin/sfw /usr/local/bin/lfs
cat > /etc/dpkg/dpkg.cfg.d/lfs-vm <<'DPKG'
path-exclude /usr/share/doc/*
path-include /usr/share/doc/*/copyright
path-exclude /usr/share/man/*
path-exclude /usr/share/info/*
path-exclude /usr/share/locale/*
path-include /usr/share/locale/locale.alias
DPKG
DEBIAN_FRONTEND=noninteractive apt-get -qq update >/dev/null
DEBIAN_FRONTEND=noninteractive apt-get -qq install -y --no-install-recommends git openssh-client curl wget \
  ca-certificates gnupg netbase unzip xz-utils bzip2 file patch less pkg-config autoconf automake libtool \
  libssl-dev libffi-dev zlib1g-dev libsqlite3-dev libpq-dev libreadline-dev libyaml-dev libbz2-dev \
  liblzma-dev libxml2-dev libcurl4-openssl-dev >/dev/null
DEBIAN_FRONTEND=noninteractive apt-get -qq install -y --no-install-recommends iptables openssh-server \
  iproute2 rsync zstd clang libsecret-1-0 >/dev/null
DEBIAN_FRONTEND=noninteractive apt-get -qq install -y --no-install-recommends procps build-essential \
  sqlite3 python3-venv postgresql-client ffmpeg chromium fonts-noto-core xvfb >/dev/null
DEBIAN_FRONTEND=noninteractive apt-get -qq install -y --no-install-recommends socat >/dev/null
DEBIAN_FRONTEND=noninteractive apt-get -qq install -y --no-install-recommends ripgrep jq fd-find >/dev/null
ln -sfn /usr/bin/fdfind /usr/local/bin/fd
# Rootless Docker from Docker's Debian repository, its key checked by
# fingerprint, at versions two weeks old or more.
install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc
[ "$(gpg --show-keys --with-colons /etc/apt/keyrings/docker.asc 2>/dev/null | awk -F: '/^fpr/ { print $10; exit }')" \
  = 9DC858229FC7DD38854AE2D88D81803C0EBFCD88 ] || { echo "lfs-vm: Docker's apt key has an unknown fingerprint" >&2; exit 1; }
chmod a+r /etc/apt/keyrings/docker.asc
echo "deb [arch=arm64 signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian bookworm stable" \
  > /etc/apt/sources.list.d/docker.list
DEBIAN_FRONTEND=noninteractive apt-get -qq update >/dev/null
docker_version=5:29.9.1-1~debian.12~bookworm
DEBIAN_FRONTEND=noninteractive apt-get -qq install -y --no-install-recommends uidmap slirp4netns \
  docker-ce=$docker_version docker-ce-cli=$docker_version docker-ce-rootless-extras=$docker_version \
  docker-compose-plugin=5.6.0-1~debian.12~bookworm >/dev/null
apt-get -qq clean
rm -rf /var/lib/apt/lists/*
# System CAs (ca-certificates, installed above) plus the Agent Vault CA: proxied hosts are re-signed by it,
# passed-through ones keep their public certificates.
cat /etc/ssl/certs/ca-certificates.crt /opt/agent-vault-ca.pem > /opt/ca-bundle.pem
grep -q '^agent:' /etc/subuid || echo agent:100000:65536 >> /etc/subuid
grep -q '^agent:' /etc/subgid || echo agent:100000:65536 >> /etc/subgid
# Host keys from the volume, or keep the new ones there.
if ls "$data"/ssh-host-keys/ssh_host_* >/dev/null 2>&1; then
  cp "$data"/ssh-host-keys/ssh_host_* /etc/ssh/
else
  cp /etc/ssh/ssh_host_* "$data/ssh-host-keys/"
fi
cat > /etc/ssh/sshd_config.d/lfs-vm.conf <<'SSHD'
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitRootLogin no
AllowUsers agent
SSHD
usermod -p '*' agent
pkill -x sshd || true
/opt/layerfs/tools/vm-boot.sh apply
echo LFS_VM_READY
SETUP

# OrbStack runs the shared services next to the personal workbench.
[ -z "$service_forwards" ] || docker compose -f "$repo/tools/orbstack/services.compose.yml" \
  -f "$repo/tools/orbstack/services.orb.yml" up -d --quiet-pull

# The proxy for every agent shell: ~/.lfs/profile.sh loads it (and the
# bridge session the Herdr plugin writes). Through stdin only.
{
  printf "export HTTPS_PROXY='%s' HTTP_PROXY='%s' https_proxy='%s' http_proxy='%s'\n" \
    "$proxy_url" "$proxy_url" "$proxy_url" "$proxy_url"
  # The bridge on the gateway is direct: the proxy cannot reach it. Package
  # installs are direct too (the firewall allows port 443): the registry and
  # Socket Firewall need no key, and one install opens more connections than
  # the shared proxy's rate limit allows every agent together.
  direct="localhost,127.0.0.1,$proxy_host,$gateway,registry.npmjs.org,.socket.dev"
  printf "export NO_PROXY='%s' no_proxy='%s'\n" "$direct" "$direct"
  printf "export SSL_CERT_FILE=/opt/ca-bundle.pem CURL_CA_BUNDLE=/opt/ca-bundle.pem GIT_SSL_CAINFO=/opt/ca-bundle.pem\n"
  printf "export REQUESTS_CA_BUNDLE=/opt/ca-bundle.pem NODE_EXTRA_CA_CERTS=/opt/agent-vault-ca.pem\n"
  # gh and glab need a token to start; the proxy replaces these placeholders
  # with the vault's GitHub and GitLab keys. Foxway vaults have neither.
  [ $name != lfs-orb ] || printf "export GH_TOKEN=agent-vault GITLAB_TOKEN=agent-vault\n"
  # Node's fetch ignores the proxy variables without this; agents have no DNS.
  printf "export NODE_USE_ENV_PROXY=1\n"
  # The Mac's time zone, so summaries and notes carry the operator's local time.
  printf "export TZ='%s'\n" "$(readlink /etc/localtime | sed 's|.*/zoneinfo/||')"
} | docker exec -i -u 501:20 $name sh -c 'umask 077; mkdir -p /home/agent/.lfs; cat > /home/agent/.lfs/proxy.env'
docker exec -i -u 501:20 $name sh -s <<'PROFILE'
umask 022
cat > /home/agent/.lfs/profile.sh <<'SH'
# mise shims give each project the tool versions its .tool-versions pins.
export PATH="$HOME/.local/share/mise/shims:$HOME/.local/bin:$PATH"
export LANG=C.UTF-8 LC_ALL=C.UTF-8
# The agent's own rootless Docker (vm-boot.sh starts it).
export DOCKER_HOST=unix:///run/user/501/docker.sock
# A missing file must not fail the profile: callers run with `set -e`.
for f in "$HOME/.lfs/proxy.env" "$HOME/.lfs/bridge.env"; do [ ! -f "$f" ] || . "$f"; done
SH
for rc in /home/agent/.bashrc /home/agent/.profile; do
  grep -qs '.lfs/profile.sh' "$rc" || echo '. "$HOME/.lfs/profile.sh"' >> "$rc"
done
PROFILE
