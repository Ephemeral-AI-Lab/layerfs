#!/bin/bash
# D-3 diagnosis (as root in lfs535l3x): what the readiness check reads inside
# the host-network worker container, and whether that container reaches a
# container port published on 127.0.0.1 (the Sandbox daemon route).
c=lfs-51678042-docker; img=$(cat /root/sandbox-image.id)
echo "## LISTEN sockets in the worker container's /proc/net/tcp (local port hex; 7070=1B9E, 44845=AF2D)"
docker exec $c sh -c 'awk "\$4==\"0A\" {print \$2}" /proc/net/tcp /proc/net/tcp6' | sort -u
echo "## worker.pid / worker.log the readiness check reads"
docker exec $c sh -c 'ls -la /layerfs/state/; cat /layerfs/state/worker.pid 2>&1; tail -n 8 /layerfs/state/worker.log 2>&1'
echo "## a throwaway container publishing 127.0.0.1::8080 (stands in for a Sandbox daemon)"
docker run -d --rm --name lfs535l3-diag -p 127.0.0.1::8080 --entrypoint node $img -e "require('net').createServer(s=>s.end('hi\n')).listen(8080)" >/dev/null
sleep 1; p=$(docker port lfs535l3-diag 8080/tcp | head -1); echo "published: $p"; port=${p##*:}
echo "## dial 127.0.0.1:$port from inside the host-network worker container"
docker exec $c node -e "const s=require('net').connect($port,'127.0.0.1');s.on('data',d=>{console.log('answered:',String(d).trim());process.exit(0)});s.on('error',e=>{console.log('error:',e.message);process.exit(1)});setTimeout(()=>{console.log('timeout');process.exit(2)},3000)"
echo "exit $?"
docker rm -f lfs535l3-diag >/dev/null
