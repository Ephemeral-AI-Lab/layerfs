#!/bin/sh
# Starts or stops a MinIO for the ignored MinIO tests (tests/minio.rs and
# layerfs-store-service's remote_minio). `start NAME [NETWORK]` runs the
# pinned image with fresh generated test credentials, creates the bucket
# `layerfs-test` and prints `export LAYERFS_REMOTE_*=…` lines to eval.
# `stop NAME` removes the container and its data.
#
# The MinIO community repository is archived (2026-04); this pinned image
# (RELEASE.2026-09-22T19-25-18Z, Chainguard build) is used only for tests.
set -eu
IMAGE=cgr.dev/chainguard/minio@sha256:4692462f35d97d7e82c30371d82f057703c5d9489bcae726010594c812f2d285
action=${1:?start NAME [NETWORK] | stop NAME}
name=${2:?container name}
case "$action" in
start)
    user=lfs$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')
    secret=$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n')
    network=${3:-}
    docker run -d --pull never --name "$name" ${network:+--network "$network"} \
        -p 127.0.0.1::9000 -e MINIO_ROOT_USER="$user" -e MINIO_ROOT_PASSWORD="$secret" \
        "$IMAGE" server /data >/dev/null
    port=$(docker port "$name" 9000/tcp | head -n 1 | sed 's/.*://')
    # Readiness wait, bounded at 30 s; not a retry of a product request.
    waited=0
    until docker exec "$name" mc alias set local http://127.0.0.1:9000 "$user" "$secret" >/dev/null 2>&1; do
        waited=$((waited + 1))
        [ "$waited" -lt 30 ] || { echo "MinIO not ready after 30 s" >&2; exit 1; }
        sleep 1
    done
    docker exec "$name" mc mb local/layerfs-test >/dev/null
    echo "export LAYERFS_REMOTE_ENDPOINT=http://127.0.0.1:$port"
    echo "export LAYERFS_REMOTE_BUCKET=layerfs-test"
    echo "export LAYERFS_REMOTE_ACCESS_KEY=$user"
    echo "export LAYERFS_REMOTE_SECRET_KEY=$secret"
    ;;
stop)
    docker rm -f -v "$name" >/dev/null
    ;;
*)
    echo "unknown action $action" >&2
    exit 2
    ;;
esac
