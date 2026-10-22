#!/bin/sh
# Starts the format study's PG 17 (local image, no download), 2 GB / 2 CPUs, no volume (container layer).
. "$(dirname "$0")/pg_env.sh"
docker run -d --name $C --memory 2g --cpus 2 --shm-size 256m -e POSTGRES_HOST_AUTH_METHOD=trust \
  postgres:17-bookworm -c fsync=off -c synchronous_commit=off -c full_page_writes=off \
  -c shared_buffers=256MB -c max_wal_size=8GB -c checkpoint_timeout=30min -c max_connections=10
