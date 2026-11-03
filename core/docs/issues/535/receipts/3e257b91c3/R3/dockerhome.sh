#!/bin/bash
# R3 Linux docker row (from L3's dockerhome.sh): one shared home whose built-in
# docker machine is started by hand (`machine start docker`, #535 D11).
# setup: import-env, image, list/get/status (stopped), start, list/get/status
# (running), the worker container's `docker inspect` (network mode, argv).
# stopped: stop, list/get/status (stopped), start again for the cells.
# teardown: stop, list/get/status, remove the worker container/volume and home.
# usage: dockerhome.sh setup|stopcheck|teardown ; as root in lfs535r3x
X=$(dirname "$(readlink -f "$0")")
B=/root/.cache/layerfs/bundle/2.0.0/linux-aarch64
H=/root/lfs535r3-dockerhome
export LAYERFS_HOME=$H
run() { echo "\$ layerfs $*"; echo "# at $(date -u +%FT%TZ)"; t0=$(date +%s.%N); "$X/bin/layerfs" "$@" 2>&1; rc=$?; echo "exit $rc  wall_s $(echo "$(date +%s.%N) - $t0" | bc)"; echo; }
three() { run machine list; run machine get docker; run machine status docker; }
cid() { python3 -c "import json;print(json.load(open('$H/layerfs.json'))['id'][:8])"; }
case "$1" in
setup)
  rm -rf $H; mkdir -m 700 $H
  echo "\$ layerfs import-env (bundle CLI, LAYERFS_DAEMON=$B/layerfs-daemon)"; LAYERFS_DAEMON=$B/layerfs-daemon $B/layerfs import-env 2>&1; echo "exit $?"; echo
  run machine update docker --image "$(cat /root/sandbox-image.id)"
  echo "## stopped state, before the first start"; three
  run machine start docker
  echo "## running state"; three
  echo "\$ docker ps"; docker ps --format '{{.Names}} {{.Image}} {{.Status}} {{.Ports}}'; echo
  c=lfs-$(cid)-docker
  echo "\$ docker inspect $c (network mode, path, args, env)"
  docker inspect -f 'NetworkMode={{.HostConfig.NetworkMode}}
Path={{json .Path}}
Args={{json .Config.Cmd}}
Entrypoint={{json .Config.Entrypoint}}
Env={{json .Config.Env}}
Mounts={{json .Mounts}}' $c
  echo
  echo "## docker run argv reconstructed from inspect (runlike-style)"
  docker inspect $c | python3 -c '
import json,sys
c=json.load(sys.stdin)[0]; h=c["HostConfig"]; cfg=c["Config"]
a=["docker","run","-d","--name",c["Name"].lstrip("/")]
if h.get("NetworkMode"): a+=["--network",h["NetworkMode"]]
for m in c.get("Mounts",[]):
  a+=["-v","%s:%s"%(m.get("Name") or m.get("Source"),m["Destination"])]
for e in cfg.get("Env") or []:
  k=e.split("=",1)[0]
  a+=["-e", e if not ("SECRET" in k or "PASSWORD" in k) else k+"=<redacted>"]
if cfg.get("Entrypoint"): a+=["--entrypoint"," ".join(cfg["Entrypoint"])]
a+=[cfg["Image"]]+(cfg.get("Cmd") or [])
print(" ".join(a))'
  echo
  echo "\$ docker logs $c (tail)"; docker logs --tail 5 $c 2>&1; echo
  ;;
stopcheck)
  run machine stop docker
  echo "## stopped state, after machine stop"; three
  echo "\$ docker ps -a (worker)"; docker ps -a --filter name=lfs-$(cid)-docker --format '{{.Names}} {{.Status}}'; echo
  run machine start docker
  echo "## running state, after the second start"; three
  ;;
teardown)
  three
  run machine stop docker
  three
  c=lfs-$(cid)-docker
  docker rm -f $c; docker volume rm $c-data
  rm -rf $H
  echo "\$ docker ps -a; docker volume ls"; docker ps -a --format '{{.Names}}'; docker volume ls -q
  ;;
esac
