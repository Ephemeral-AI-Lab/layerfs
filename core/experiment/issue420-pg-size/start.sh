#!/bin/sh
# Creates the size-study instance with the contention test's PG settings.
. "$(dirname "$0")/env.sh"
docker network create $NET
docker volume create $VOL
docker run -d --name $C --network $NET --shm-size 256m --memory ${MEM:-4g} --cpus 2 \
  -e POSTGRES_HOST_AUTH_METHOD=trust -v $VOL:/var/lib/postgresql/data \
  postgres:17-bookworm -c fsync=off -c synchronous_commit=off -c full_page_writes=off \
  -c shared_buffers=256MB -c max_wal_size=8GB -c checkpoint_timeout=30min -c max_connections=20
