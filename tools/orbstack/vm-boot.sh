#!/bin/bash
# The command of the workbench container (lfs-orb), and the step that applies
# its boot state after setup. A container restart loses the iptables rules
# and sshd, so this script brings them back.
#   vm-boot.sh apply   apply the agent firewall, start sshd, the
#                      forwards to the shared services and the agent's
#                      rootless Docker
#   vm-boot.sh run     apply, then trim the disks hourly (the container command)
# The container runs with --init, which reaps the processes sshd leaves.
set -eu
config=/etc/lfs-vm/boot.env

apply() {
  [ -f "$config" ] || return 0
  . "$config"
  if command -v iptables >/dev/null; then
    # Agents (uid 501) may use the proxy, the bridge, and (direct_egress) DNS, HTTPS (port 443) to any
    # host, and SSH (port 22) of the Git hosts and the operator's servers. Direct HTTPS is there for
    # Socket Firewall: its npm wrapper connects to registries itself. Each Git
    # host is pinned in /etc/hosts and allowed by address. The Agent Vault
    # proxy expects TLS in every tunnel, so it cannot carry SSH.
    for tool in iptables ip6tables; do
      $tool -F OUTPUT
      $tool -A OUTPUT -m owner --uid-owner 501 -o lo -j ACCEPT
      $tool -A OUTPUT -m owner --uid-owner 501 -m conntrack --ctstate ESTABLISHED -j ACCEPT
    done
    iptables -A OUTPUT -m owner --uid-owner 501 -d "$PROXY_IP" -p tcp --dport "$PROXY_PORT" -j ACCEPT
    iptables -A OUTPUT -m owner --uid-owner 501 -d "$GATEWAY" -p tcp --dport "$BRIDGE_PORT" -j ACCEPT
    if [ "${EGRESS:-}" = gateway ]; then
      gateway_egress
    else
      direct_egress
    fi
    iptables -A OUTPUT -m owner --uid-owner 501 -j REJECT
    ip6tables -A OUTPUT -m owner --uid-owner 501 -j REJECT
  fi
  if [ -x /usr/sbin/sshd ] && ! pgrep -x sshd >/dev/null; then
    mkdir -p /run/sshd
    /usr/sbin/sshd
  fi
  # The workbench must keep running without its services.
  services || echo "vm-boot: shared services did not start" >&2
  agent_docker || echo "vm-boot: the agent's Docker did not start" >&2
}

# The default: the host NATs the box, and the firewall allows its direct connections.
direct_egress() {
  for proto in udp tcp; do
    iptables -A OUTPUT -m owner --uid-owner 501 -p "$proto" --dport 53 -j ACCEPT
  done
  iptables -A OUTPUT -m owner --uid-owner 501 -p tcp --dport 443 -j ACCEPT
  for host in ${GIT_SSH_HOSTS:-}; do
    # /etc/hosts is a bind mount: rewrite it in place, never replace it.
    grep -v " $host\$" /etc/hosts > /etc/hosts.lfs || true
    cat /etc/hosts.lfs > /etc/hosts && rm -f /etc/hosts.lfs
    ip=$(getent ahostsv4 "$host" | awk 'NR == 1 { print $1 }')
    [ -n "$ip" ] || continue
    echo "$ip $host" >> /etc/hosts
    iptables -A OUTPUT -m owner --uid-owner 501 -d "$ip" -p tcp --dport 22 -j ACCEPT
  done
  # The operator's servers (dev box, VPS) by address, from lfs-bridge install-vm.
  for ip in ${SSH_SERVER_IPS:-}; do
    iptables -A OUTPUT -m owner --uid-owner 501 -d "$ip" -p tcp --dport 22 -j ACCEPT
  done
}

# EGRESS=gateway: no host NAT (sophiai ADR 0024, Apple container). The Mac listens on GATEWAY and
# opens each connection itself, so a VPN or pf change on the Mac cannot cut the box off. DNS and
# HTTPS of every user go there (nat OUTPUT); the Mac picks the HTTPS host from the TLS SNI. SSH goes
# to one gateway port per host (the box's ssh_config). uid 501 may reach nothing else: not even
# port 22 of the Mac. boot.env: GATEWAY_DNS_PORT, GATEWAY_TLS_PORT, GATEWAY_SSH_PORTS.
gateway_egress() {
  : "${GATEWAY_DNS_PORT:?boot.env must set it with EGRESS=gateway}" "${GATEWAY_TLS_PORT:?boot.env must set it with EGRESS=gateway}"
  # resolv.conf may be a bind mount: write it in place.
  printf 'nameserver %s\n' "$GATEWAY" > /etc/resolv.conf
  iptables -t nat -F OUTPUT
  iptables -t nat -A OUTPUT -d 127.0.0.0/8 -j RETURN
  for proto in udp tcp; do
    iptables -t nat -A OUTPUT -p "$proto" --dport 53 -j DNAT --to-destination "$GATEWAY:$GATEWAY_DNS_PORT"
    iptables -A OUTPUT -m owner --uid-owner 501 -d "$GATEWAY" -p "$proto" --dport "$GATEWAY_DNS_PORT" -j ACCEPT
  done
  iptables -t nat -A OUTPUT -p tcp --dport 443 ! -d "$GATEWAY" -j DNAT --to-destination "$GATEWAY:$GATEWAY_TLS_PORT"
  for port in "$GATEWAY_TLS_PORT" ${GATEWAY_SSH_PORTS:-}; do
    iptables -A OUTPUT -m owner --uid-owner 501 -d "$GATEWAY" -p tcp --dport "$port" -j ACCEPT
  done
}

# The agent's rootless Docker (ADR 0005): dockerd as uid 501, its data in the
# agent home on lfs-data, its socket in /run/user/501. The data root is the
# real path: /home/agent is a link, and runc refuses a rootfs path with one. The firewall above
# applies to it and its containers, as to every process of the agent.
agent_docker() {
  command -v dockerd-rootless.sh >/dev/null || return 0
  pgrep -u 501 -f rootlesskit >/dev/null && return 0
  install -d -o 501 -g 20 -m 0700 /run/user/501
  # rootlesskit makes dockerd's mount namespace once. With / shared, the
  # Workspaces mounted later reach it, so compose files can bind-mount their files.
  mount --make-rshared /
  install -d -o 501 -g 20 /home/agent/.local /home/agent/.local/state
  setpriv --reuid 501 --regid 20 --init-groups env -i HOME=/home/agent XDG_RUNTIME_DIR=/run/user/501 \
    PATH=/usr/bin:/usr/sbin:/bin:/sbin sh -c 'exec dockerd-rootless.sh --data-root "$1" \
      >> "$HOME/.local/state/dockerd.log" 2>&1' sh /var/lib/layerfs-data/agent-home/.local/share/docker &
}

# The shared services (services.compose.yml) are sibling containers of
# OrbStack. Forward each to loopback (SERVICE_FORWARDS).
services() {
  for forward in $SERVICE_FORWARDS; do
    port=${forward%%:*}
    pgrep -f "socat TCP-LISTEN:$port," >/dev/null && continue
    socat "TCP-LISTEN:$port,bind=127.0.0.1,fork,reuseaddr" "TCP:${forward#*:}" >/dev/null 2>&1 &
  done
}

case "${1:-}" in
  apply) apply ;;
  # The Mac gets space back from the disk images only after a trim: deleted
  # files (a discarded Workspace's Overlay) stay allocated until then.
  # The sleep runs in the background, so the TERM trap runs at once.
  run)
    trap 'exit 0' TERM
    apply
    while :; do sleep 3600 & wait $!; fstrim -a >/dev/null 2>&1 || true; done ;;
  *) echo "usage: $0 apply|run" >&2; exit 2 ;;
esac
