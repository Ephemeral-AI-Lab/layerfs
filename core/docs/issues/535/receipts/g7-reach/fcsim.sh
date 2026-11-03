#!/bin/bash
# Simulates a Firecracker microVM's L3 path on a Lima host (NOT Firecracker): a netns stands in for the microVM,
# a veth for its tap, addressed exactly as firecracker/scripts.rs does for n=1 (host 172.30.1.1/30, guest 172.30.1.2, gw 172.30.1.1).
set -u
t() { printf '   %-28s -> ' "$1:$2"; ip netns exec g7fc timeout 5 bash -c "exec 3<>/dev/tcp/$1/$2 && printf '\x00\x00\x00\x08\x04\xd2\x16\x2f' >&3 && dd bs=64 count=1 status=none <&3" 2>&1 | tr -c '[:print:]' '.'; echo " (rc=${PIPESTATUS[0]})"; }
all() { for hp in 172.30.1.1:55432 192.168.5.2:55432 192.168.5.2:55433 192.168.5.2:55442 1.1.1.1:53; do t ${hp%:*} ${hp#*:}; done; }
echo "== iptables: $(command -v iptables) $(iptables --version 2>&1)"
echo "== ip_forward before: $(sysctl -n net.ipv4.ip_forward)"
ip netns add g7fc
ip link add g7fc-h type veth peer name g7fc-g
ip link set g7fc-g netns g7fc
ip addr add 172.30.1.1/30 dev g7fc-h; ip link set g7fc-h up
ip netns exec g7fc ip addr add 172.30.1.2/30 dev g7fc-g
ip netns exec g7fc ip link set g7fc-g up; ip netns exec g7fc ip link set lo up
ip netns exec g7fc ip route add default via 172.30.1.1
echo "== simulated guest routes:"; ip netns exec g7fc ip route
echo "== A. as built today (no NAT, ip_forward as booted)"; all
echo "== B. ip_forward=1 + MASQUERADE 172.30.0.0/16 (the README's planned outbound NAT)"
sysctl -qw net.ipv4.ip_forward=1
iptables -t nat -A POSTROUTING -s 172.30.0.0/16 ! -d 172.30.0.0/16 -j MASQUERADE
all
echo "== C. B + DNAT 172.30.1.1:55432 -> 192.168.5.2:55432 (what reach=172.30.<n>.1 would additionally need)"
iptables -t nat -A PREROUTING -i g7fc-h -d 172.30.1.1 -p tcp --dport 55432 -j DNAT --to-destination 192.168.5.2:55432
all
echo "== nat table:"; iptables -t nat -S
