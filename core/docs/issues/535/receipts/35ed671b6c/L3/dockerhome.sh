#!/bin/bash
# Linux docker row: one shared home whose built-in docker machine is started by
# hand (`machine start docker`, #535 D11: nothing starts a machine on demand);
# the cells run in it with LFS533_HOME (each cell's sqlite Store pushed
# native -> docker and pulled back). Also records `machine list` vs
# `machine status docker` before and after the start (live state fix), and the
# worker container's network mode (--network host fix).
# usage: dockerhome.sh setup|teardown ; as root in lfs535l3x
X=$(dirname "$(readlink -f "$0")")
B=/root/.cache/layerfs/bundle/2.0.0/linux-aarch64
H=/root/lfs535l3-dockerhome
export LAYERFS_HOME=$H
run() { echo "\$ layerfs $*"; echo "# at $(date -u +%FT%TZ)"; "$X/bin/layerfs" "$@" 2>&1; echo "exit $?"; echo; }
if [ "$1" = setup ]; then
  rm -rf $H; mkdir -m 700 $H
  echo "\$ layerfs import-env (bundle CLI, LAYERFS_DAEMON=$B/layerfs-daemon)"; LAYERFS_DAEMON=$B/layerfs-daemon $B/layerfs import-env 2>&1; echo "exit $?"; echo
  run machine update docker --image "$(cat /root/sandbox-image.id)"
  run machine list
  run machine status docker
  run machine get docker
  run machine start docker
  run machine list
  run machine status docker
  run machine get docker
  docker ps --format '{{.Names}} {{.Image}} {{.Status}} {{.Ports}}'
  id=$(python3 -c "import json;print(json.load(open('$H/layerfs.json'))['id'][:8])")
  echo "\$ docker inspect lfs-$id-docker (network mode, args)"
  docker inspect -f '{{.HostConfig.NetworkMode}} {{json .HostConfig.ExtraHosts}} {{json .Config.Env}}' lfs-$id-docker
else
  run machine list
  run machine stop docker
  run machine list
  run machine status docker
  id=$(python3 -c "import json;print(json.load(open('$H/layerfs.json'))['id'][:8])")
  docker rm -f lfs-$id-docker; docker volume rm lfs-$id-docker-data
  rm -rf $H
  docker ps -a --format '{{.Names}}'; docker volume ls -q
fi
