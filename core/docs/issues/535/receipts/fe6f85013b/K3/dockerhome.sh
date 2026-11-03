#!/bin/bash
# Linux docker row, attempt 2 (after the D11 diagnosis): one shared home whose
# built-in docker machine is started by hand (`machine start docker`, as #535
# D11 requires: nothing starts a machine on demand), then the cells run in it
# with LFS533_HOME (each cell's Store pushed native -> docker and pulled back).
# usage: dockerhome.sh setup|teardown ; as root in lfs535k3x
X=$(dirname "$(readlink -f "$0")")
B=/root/.cache/layerfs/bundle/2.0.0/linux-aarch64
H=/root/lfs535k3-dockerhome
export LAYERFS_HOME=$H
run() { echo "\$ layerfs $*"; echo "# at $(date -u +%FT%TZ)"; "$X/bin/layerfs" "$@" 2>&1; echo "exit $?"; echo; }
if [ "$1" = setup ]; then
  rm -rf $H; mkdir -m 700 $H
  echo "\$ layerfs import-env (bundle CLI, LAYERFS_DAEMON=$B/layerfs-daemon)"; LAYERFS_DAEMON=$B/layerfs-daemon $B/layerfs import-env 2>&1; echo "exit $?"; echo
  run machine update docker --image "$(cat /root/sandbox-image.id)"
  run machine status docker
  run machine start docker
  run machine status docker
  docker ps --format '{{.Names}} {{.Image}} {{.Status}}'
else
  run machine list
  run machine stop docker
  id=$(python3 -c "import json;print(json.load(open('$H/layerfs.json'))['id'][:8])")
  docker rm -f lfs-$id-docker; docker volume rm lfs-$id-docker-data
  rm -rf $H
  docker ps -a --format '{{.Names}}'; docker volume ls -q
fi
