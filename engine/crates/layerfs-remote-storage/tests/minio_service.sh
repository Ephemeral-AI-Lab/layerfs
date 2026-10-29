#!/bin/sh
# Starts or stops a MinIO for the ignored MinIO tests (tests/minio.rs and
# layerfs-store-service's remote_minio). `start NAME [NETWORK]` runs the
# pinned image with fresh generated test credentials, creates the bucket
# `layerfs-test` and prints `export LAYERFS_REMOTE_*=…` lines to eval.
# `stop NAME` removes the container and its data.
#
# With MINIO_TLS_DIR set, MinIO serves HTTPS with a fresh self-signed
# certificate written there (for localhost, 127.0.0.1 and NAME), and the
# exports add SSL_CERT_FILE, which OpenSSL (native-tls on Linux) trusts. Run
# the tests with `--features tls`.
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
    tls=${MINIO_TLS_DIR:-}
    scheme=http
    insecure=
    if [ -n "$tls" ]; then
        mkdir -p "$tls"
        openssl req -x509 -newkey rsa:2048 -nodes -days 2 -subj "/CN=$name" \
            -addext "subjectAltName=DNS:localhost,DNS:$name,IP:127.0.0.1" \
            -keyout "$tls/private.key" -out "$tls/public.crt" 2>/dev/null
        # the test key of a throwaway server, readable by MinIO's user
        chmod 644 "$tls/private.key" "$tls/public.crt"
        scheme=https
        insecure=--insecure
    fi
    docker run -d --pull never --name "$name" ${network:+--network "$network"} \
        -p 127.0.0.1::9000 -e MINIO_ROOT_USER="$user" -e MINIO_ROOT_PASSWORD="$secret" \
        ${tls:+-v "$tls:/certs:ro"} "$IMAGE" server /data ${tls:+--certs-dir /certs} >/dev/null
    port=$(docker port "$name" 9000/tcp | head -n 1 | sed 's/.*://')
    # Readiness wait, bounded at 30 s; not a retry of a product request.
    waited=0
    until docker exec "$name" mc $insecure alias set local "$scheme://127.0.0.1:9000" "$user" "$secret" >/dev/null 2>&1; do
        waited=$((waited + 1))
        [ "$waited" -lt 30 ] || { echo "MinIO not ready after 30 s" >&2; exit 1; }
        sleep 1
    done
    docker exec "$name" mc $insecure mb local/layerfs-test >/dev/null
    # From this host: the published loopback port. From a container on
    # NETWORK (such as the macOS worker), use the container name instead.
    if [ -n "$network" ]; then
        echo "# on network $network: export LAYERFS_REMOTE_ENDPOINT=$scheme://$name:9000"
    fi
    echo "export LAYERFS_REMOTE_ENDPOINT=$scheme://127.0.0.1:$port"
    if [ -n "$tls" ]; then echo "export SSL_CERT_FILE=$tls/public.crt"; fi
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
