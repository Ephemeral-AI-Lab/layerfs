#!/bin/bash
# PostgreSQL v3 StartupMessage (user=postgres database=g7probe); prints the server's first reply, printable bytes only.
# A reachable server answers 'R' + auth request (SCRAM-SHA-256 for this throwaway server).
for hp in "$@"; do h=${hp%:*}; p=${hp##*:}
  printf '== PG startup %s:%s -> ' "$h" "$p"
  timeout 5 bash -c "exec 3<>/dev/tcp/$h/$p && printf '\x00\x00\x00\x28\x00\x03\x00\x00user\x00postgres\x00database\x00g7probe\x00\x00' >&3 && dd bs=256 count=1 status=none <&3" 2>&1 | tr -c '[:print:]' '.'; echo " (rc=${PIPESTATUS[0]})"
done
