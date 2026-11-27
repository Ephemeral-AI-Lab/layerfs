#!/usr/bin/env python3
"""Decode a pcap of PostgreSQL traffic captured inside the server container's
network namespace (server port 5432) into per-connection exchanges, split by
probe case using the observer's pg_stat_statements snapshot queries.

Exchange = one client send phase followed by the server send phase(s) up to
ReadyForQuery. Timestamps are the container's eth0: inbound at arrival,
outbound after the egress qdisc (netem), so reply_first - req_last includes
the netem delay.
"""
import struct
import sys
import json
from collections import defaultdict

SERVER_PORT = 5432
GAPS = [0]


def packets(path):
    with open(path, "rb") as f:
        gh = f.read(24)
        magic = struct.unpack("<I", gh[:4])[0]
        if magic == 0xA1B2C3D4:
            endian, nano = "<", False
        elif magic == 0xA1B23C4D:
            endian, nano = "<", True
        else:
            raise SystemExit(f"bad magic {magic:x}")
        linktype = struct.unpack(endian + "I", gh[20:24])[0]
        while True:
            h = f.read(16)
            if len(h) < 16:
                return
            sec, frac, incl, orig = struct.unpack(endian + "IIII", h)
            data = f.read(incl)
            ts = sec + frac / (1e9 if nano else 1e6)
            if linktype == 1:
                eth_type = struct.unpack(">H", data[12:14])[0]
                off = 14
            elif linktype == 113:  # Linux cooked
                eth_type = struct.unpack(">H", data[14:16])[0]
                off = 16
            else:
                raise SystemExit(f"linktype {linktype}")
            if eth_type != 0x0800:
                continue
            ip = data[off:]
            ihl = (ip[0] & 0xF) * 4
            total = struct.unpack(">H", ip[2:4])[0]
            if ip[9] != 6:
                continue
            tcp = ip[ihl:total] if total else ip[ihl:]
            sport, dport, seq, ack = struct.unpack(">HHII", tcp[:12])
            doff = (tcp[12] >> 4) * 4
            flags = tcp[13]
            payload = tcp[doff:]
            yield ts, sport, dport, seq, flags, payload


class Stream:
    def __init__(self):
        self.next_seq = None
        self.buf = bytearray()
        self.buf_ts = []  # (offset_in_total, ts) for each appended chunk
        self.consumed = 0
        self.total = 0
        self.started = False

    def add(self, ts, seq, flags, payload):
        if flags & 0x02:  # SYN
            self.next_seq = (seq + 1) & 0xFFFFFFFF
            return []
        if not payload:
            return []
        if self.next_seq is None:
            self.next_seq = seq
        if seq != self.next_seq and ((seq - self.next_seq) & 0xFFFFFFFF) < 0x80000000:
            GAPS[0] += 1
            self.next_seq = seq
        if seq != self.next_seq:
            # retransmission / overlap: keep only new bytes
            d = (self.next_seq - seq) & 0xFFFFFFFF
            if d < len(payload):
                payload = payload[d:]
            else:
                return []
        self.next_seq = (self.next_seq + len(payload)) & 0xFFFFFFFF
        self.buf_ts.append((self.total, ts))
        self.buf += payload
        self.total += len(payload)
        return [len(payload)]

    def ts_at(self, absolute):
        t = None
        for off, ts in self.buf_ts:
            if off <= absolute:
                t = ts
            else:
                break
        return t


def parse_messages(stream, frontend):
    """Yield (type, body, first_ts, last_ts) for complete messages."""
    out = []
    while True:
        b = stream.buf
        if frontend and not stream.started:
            if len(b) < 8:
                break
            ln = struct.unpack(">I", b[:4])[0]
            if len(b) < ln:
                break
            code = struct.unpack(">I", b[4:8])[0]
            start = stream.consumed
            out.append(("startup" if code != 80877103 else "sslreq", bytes(b[8:ln]),
                        stream.ts_at(start), stream.ts_at(start + ln - 1)))
            del b[:ln]
            stream.consumed += ln
            if code != 80877103:
                stream.started = True
            continue
        if not frontend and not stream.started:
            # an 'N'/'S' single byte reply to SSLRequest is possible; we use sslmode=disable
            stream.started = True
        if len(b) < 5:
            break
        typ = chr(b[0])
        ln = struct.unpack(">I", b[1:5])[0]
        if len(b) < 1 + ln:
            break
        start = stream.consumed
        out.append((typ, bytes(b[5:1 + ln]), stream.ts_at(start), stream.ts_at(start + ln)))
        del b[:1 + ln]
        stream.consumed += 1 + ln
    # trim timestamp list
    stream.buf_ts = [(o, t) for (o, t) in stream.buf_ts if o >= stream.consumed - 70000] or stream.buf_ts[-1:]
    return out


def cstr(body, i=0):
    j = body.index(b"\0", i)
    return body[i:j].decode("utf-8", "replace"), j + 1


def main(path, out_json):
    streams = {}
    conn_events = defaultdict(list)  # cport -> list of (ts, dir, msg_type, info, nbytes)
    stmt_sql = defaultdict(dict)
    portal_stmt = defaultdict(dict)
    pkts = defaultdict(list)  # cport -> (ts, dir, len)
    for ts, sp, dp, seq, flags, payload in packets(path):
        if dp == SERVER_PORT:
            cport, d = sp, "C"
        elif sp == SERVER_PORT:
            cport, d = dp, "S"
        else:
            continue
        key = (cport, d)
        st = streams.setdefault(key, Stream())
        st.add(ts, seq, flags, payload)
        if payload:
            pkts[cport].append((ts, d, len(payload)))
        for typ, body, t0, t1 in parse_messages(st, d == "C"):
            info = ""
            n = len(body) + 5
            if d == "C":
                if typ == "P":
                    name, i = cstr(body)
                    q, i = cstr(body, i)
                    stmt_sql[cport][name] = q
                    info = f"{name}|{q}"
                elif typ == "B":
                    portal, i = cstr(body)
                    name, i = cstr(body, i)
                    info = f"{name}|{stmt_sql[cport].get(name, '?')}"
                elif typ == "Q":
                    q, _ = cstr(body)
                    info = q
                elif typ == "D" and False:
                    pass
            else:
                if typ == "E":
                    info = body.decode("utf-8", "replace").replace("\0", " ")[:200]
                elif typ == "C":
                    info = body.decode("utf-8", "replace").rstrip("\0")
            conn_events[cport].append((t0, t1, d, typ, info, n))
    # Build exchanges per connection
    exchanges = []
    for cport, evs in conn_events.items():
        cur = None
        for t0, t1, d, typ, info, n in evs:
            if d == "C":
                if cur is None or cur.get("reply_first") is not None:
                    if cur is not None:
                        exchanges.append(cur)
                    cur = {"conn": cport, "req_first": t0, "req_last": t1, "msgs": [], "sql": [],
                           "req_bytes": 0, "reply_first": None, "reply_last": None, "reply_bytes": 0,
                           "rows": 0, "z": 0, "client_sends_after_reply": 0, "errors": []}
                elif cur.get("reply_first") is not None:
                    # client sent more while a reply was partially received (not yet done)
                    cur["client_sends_after_reply"] += 1
                cur["req_last"] = max(cur["req_last"], t1)
                cur["msgs"].append(typ)
                cur["req_bytes"] += n
                if typ in ("P", "B", "Q") and info:
                    cur["sql"].append(typ + ":" + info)
            else:
                if cur is None:
                    continue
                if cur["reply_first"] is None:
                    cur["reply_first"] = t0
                cur["reply_last"] = t1
                cur["reply_bytes"] += n
                if typ == "D":
                    cur["rows"] += 1
                if typ == "E":
                    cur["errors"].append(info)
                if typ == "Z":
                    cur["z"] += 1
                    # done when all Syncs/Queries answered
                    syncs = cur["msgs"].count("S") + cur["msgs"].count("Q")
                    if cur["z"] >= syncs:
                        cur["done"] = True
        if cur is not None:
            exchanges.append(cur)
    # packet-level detail per exchange: count server send bursts and intra-reply gaps
    for ex in exchanges:
        ps = [p for p in pkts[ex["conn"]] if ex["req_first"] - 1e-6 <= p[0] <= (ex["reply_last"] or ex["req_last"]) + 1e-6]
        cts = [p[0] for p in ps if p[1] == "C"]
        sts = [p[0] for p in ps if p[1] == "S"]
        ex["req_pkts"] = len(cts)
        ex["reply_pkts"] = len(sts)
        ex["req_span_ms"] = (max(cts) - min(cts)) * 1e3 if cts else 0
        ex["reply_span_ms"] = (max(sts) - min(sts)) * 1e3 if sts else 0
        gaps = [(b - a) * 1e3 for a, b in zip(sts, sts[1:])]
        ex["reply_max_gap_ms"] = max(gaps) if gaps else 0
        # does the client send between server packets of this exchange?
        ex["interleaved"] = sum(1 for p in ps if p[1] == "C" and sts and min(sts) < p[0] < max(sts))
        ex["msgs"] = "".join(ex["msgs"])
    exchanges.sort(key=lambda e: e["req_first"])
    # observer connection = the one that queries pg_stat_statements
    obs = {e["conn"] for e in exchanges if any("pg_stat_statements" in s for s in e["sql"])}
    json.dump({"observer_ports": sorted(obs), "exchanges": exchanges}, open(out_json, "w"), default=str)
    print(f"gaps {GAPS[0]} exchanges {len(exchanges)} observer {sorted(obs)} conns {sorted(conn_events)}")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
