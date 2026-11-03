# usage: sh sslreq.sh <host> <port>: one PostgreSQL SSLRequest, prints the one-byte answer; no auth
h=$1; p=$2
T=$(command -v timeout >/dev/null && echo "timeout 5"); $T bash -c "exec 3<>/dev/tcp/$h/$p || exit 9; printf '\x00\x00\x00\x08\x04\xd2\x16\x2f' >&3; dd bs=1 count=1 <&3 2>/dev/null" > /tmp/lfs535-ans 2>/tmp/lfs535-err; rc=$?
printf 'SSLRequest %s:%s rc=%s answer=%s %s\n' "$h" "$p" "$rc" "$(cat /tmp/lfs535-ans)" "$(head -c 200 /tmp/lfs535-err | tr '\n' ' ')"
rm -f /tmp/lfs535-ans /tmp/lfs535-err
