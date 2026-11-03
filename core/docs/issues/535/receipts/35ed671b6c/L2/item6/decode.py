"""Decodes roundtrips.log: per measured call, the guest's listeners and the
sockets that appeared during it (from /proc/net/tcp). usage: decode.py <log>"""

import re
import sys

STATES = {"01": "ESTABLISHED", "06": "TIME_WAIT", "0A": "LISTEN", "08": "CLOSE_WAIT"}


def addr(h):
    ip, port = h.split(":")
    return ".".join(str(b) for b in bytes.fromhex(ip)[::-1]) + ":" + str(int(port, 16))


text = open(sys.argv[1]).read()
for section in re.split(r"^## ", text, flags=re.M)[1:]:
    name = section.split("\n")[0]
    before = section.split("### guest sockets before\n")[1].split("###")[0].strip().split("\n")[1:]
    after = section.split("### guest sockets after\n")[1].split("###")[0].strip().split("\n")[1:]
    listen = sorted({addr(line.split()[0]) for line in after if line.split()[2] == "0A"})
    print(f"{name}: guest listeners {listen}")
    for line in after:
        if line not in before:
            local, remote, state = line.split()
            print(f"  new socket: {addr(local)} <- {addr(remote)} {STATES.get(state, state)}")
