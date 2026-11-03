#!/bin/bash
# G7 guest probe. Args: candidate hosts. Tests 55432 (pg, host 127.0.0.1 only), 55433 (pg, host 0.0.0.0),
# 55442 (host-native listener, 127.0.0.1 only), 55443 (host-native listener, 0.0.0.0).
echo "== uname"; uname -a
echo "== /etc/os-release"; grep PRETTY /etc/os-release
echo "== ip addr"; (ip -4 addr 2>/dev/null || cat /proc/net/fib_trie | head -20)
echo "== ip route"; (ip route 2>/dev/null || cat /proc/net/route)
echo "== /etc/hosts"; cat /etc/hosts
echo "== /etc/resolv.conf"; cat /etc/resolv.conf
for n in host.lima.internal host.containers.internal host.docker.internal host.container.internal host.microsandbox.internal gateway.internal; do
  printf '== getent hosts %s: ' "$n"; getent hosts "$n" || echo "(no answer, rc=$?)"
done
GW=$(ip route 2>/dev/null | awk '/^default/{print $3; exit}')
echo "== default gateway: ${GW:-none}"
pgcheck() { # PostgreSQL SSLRequest: a PG server answers one byte, 'N' or 'S'
  timeout 5 bash -c "exec 3<>/dev/tcp/$1/$2 && printf '\x00\x00\x00\x08\x04\xd2\x16\x2f' >&3 && head -c1 <&3" 2>&1; echo " (rc=$?)"; }
lncheck() { timeout 5 bash -c "exec 3<>/dev/tcp/$1/$2 && head -n1 <&3" 2>&1; echo " (rc=$?)"; }
for h in "$@" $GW; do
  for p in 55432 55433; do printf '== PG SSLRequest %s:%s -> ' "$h" "$p"; pgcheck "$h" "$p"; done
  for p in 55442 55443; do printf '== host-native listener %s:%s -> ' "$h" "$p"; lncheck "$h" "$p"; done
done
if command -v psql >/dev/null; then
  for h in "$@" $GW; do for p in 55432 55433; do
    echo "== psql $h:$p"; PGPASSWORD=g7probe PGCONNECT_TIMEOUT=5 psql "host=$h port=$p user=postgres dbname=g7probe sslmode=disable" -Atc "select inet_server_addr(), inet_client_addr(), (select count(*) from pg_namespace where nspname='g7probe_schema')" 2>&1
  done; done
else echo "== psql not installed in guest"; fi
