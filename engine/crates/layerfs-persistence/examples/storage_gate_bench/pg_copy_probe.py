#!/usr/bin/env python3
"""#758 client-profile comparison arms (stdlib only; DIAGNOSTIC, one sample each).

Reads the same pack set as `pg_client_profile` with clients other than the
Rust driver, to separate the transport from the Rust client:

  host   - a minimal Python client on this host over the published port:
           trust startup, one simple-protocol `COPY ... TO STDOUT (FORMAT
           binary)`, bytes counted until ReadyForQuery.
  unix   - psql inside the server container over its Unix socket.
  tcp    - psql inside the server container over TCP 127.0.0.1.

psql arms are timed inside the container (`date +%s%N` around psql) and
reported both raw and less one `SELECT 1` psql run (process start and
connect). Usage:

  pg_copy_probe.py --schema S [--host 127.0.0.1 --port 55758 --container C]
"""

import argparse
import socket
import struct
import subprocess
import time


def receive(sock, view, count):
    got = 0
    while got < count:
        n = sock.recv_into(view[got:count])
        if n == 0:
            raise ConnectionError("closed")
        got += n


def host_copy(host, port, user, database, sql):
    sock = socket.create_connection((host, port))
    sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
    params = b"".join(
        k.encode() + b"\0" + v.encode() + b"\0"
        for k, v in (("user", user), ("database", database))
    )
    payload = struct.pack("!I", 196608) + params + b"\0"
    sock.sendall(struct.pack("!I", len(payload) + 4) + payload)
    buf = bytearray(1 << 20)
    view = memoryview(buf)
    head = bytearray(5)
    hv = memoryview(head)

    def message():
        receive(sock, hv, 5)
        kind = head[0:1]
        (length,) = struct.unpack("!I", head[1:5])
        body = length - 4
        nonlocal buf, view
        if body > len(buf):
            buf = bytearray(body)
            view = memoryview(buf)
        receive(sock, view, body)
        return kind, body

    while True:
        kind, body = message()
        if kind == b"E":
            raise ConnectionError(bytes(view[:body]))
        if kind == b"Z":
            break
    query = sql.encode() + b"\0"
    started = time.perf_counter()
    sock.sendall(b"Q" + struct.pack("!I", len(query) + 4) + query)
    data = 0
    while True:
        kind, body = message()
        if kind == b"d":
            data += body
        elif kind == b"E":
            raise ConnectionError(bytes(view[:body]))
        elif kind == b"Z":
            break
    ms = (time.perf_counter() - started) * 1e3
    sock.sendall(b"X\0\0\0\4")
    sock.close()
    return ms, data


def psql_ms(container, extra, sql):
    script = (
        's=$(date +%s%N); psql -U postgres -d postgres -X -q -At '
        + extra
        + ' -c "$1" > /dev/null; e=$(date +%s%N); echo $(( (e-s)/1000 ))'
    )
    out = subprocess.run(
        ["docker", "exec", container, "sh", "-c", script, "probe", sql],
        capture_output=True,
        text=True,
        check=True,
    )
    return int(out.stdout.strip()) / 1e3


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--schema", required=True)
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, default=55758)
    p.add_argument("--container", default="lfs-bench-758-pg")
    a = p.parse_args()
    sql = (
        f"COPY (SELECT body FROM {a.schema}.pack ORDER BY pack_id) "
        "TO STDOUT (FORMAT binary)"
    )
    body = int(
        subprocess.run(
            ["docker", "exec", a.container, "psql", "-U", "postgres", "-At", "-c",
             f"SELECT sum(length) FROM {a.schema}.pack"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
    )
    host_copy(a.host, a.port, "postgres", "postgres", sql)  # warm, unmeasured
    ms, data = host_copy(a.host, a.port, "postgres", "postgres", sql)
    print(f"arm=python_host_copy ms={ms:.2f} copy_bytes={data} body_bytes={body} "
          f"ms_per_mb={ms / (body / 1e6):.3f}")
    for arm, extra in (("psql_unix_copy", ""), ("psql_tcp_copy", "-h 127.0.0.1")):
        base = psql_ms(a.container, extra, "SELECT 1")
        raw = psql_ms(a.container, extra, sql)
        net = raw - base
        print(f"arm={arm} ms_raw={raw:.2f} ms_select1={base:.2f} ms={net:.2f} "
              f"body_bytes={body} ms_per_mb={net / (body / 1e6):.3f}")


if __name__ == "__main__":
    main()
