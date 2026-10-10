#!/usr/bin/env python3
"""Writes two position-dependent byte streams, interleaved, then exits.

usage: stream_source.py STDOUT_BYTES STDERR_BYTES EXIT_CODE [held SECONDS]

Each stream is a sequence of 4,096-byte blocks: an 8-byte big-endian block
index followed by one repeated byte (0x6f for stdout, 0x65 for stderr), the
last block truncated. A lost, repeated or reordered block changes the digest.
With `held`, a descendant keeps stdout open, sleeps, writes one last line
after the command itself has exited, and exits.
"""
import os
import sys
import time

BLOCK = 4096
TURN = 256 * BLOCK


def stream(fill, start, stop):
    parts = []
    for index in range(start // BLOCK, (stop + BLOCK - 1) // BLOCK):
        block = index.to_bytes(8, "big") + bytes([fill]) * (BLOCK - 8)
        parts.append(block[max(start - index * BLOCK, 0):min(stop - index * BLOCK, BLOCK)])
    return b"".join(parts)


def put(descriptor, data):
    view = memoryview(data)
    while view:
        view = view[os.write(descriptor, view):]


def main():
    totals = (int(sys.argv[1]), int(sys.argv[2]))
    sent = [0, 0]
    while sent[0] < totals[0] or sent[1] < totals[1]:
        for which, (descriptor, fill) in enumerate(((1, 0x6F), (2, 0x65))):
            stop = min(sent[which] + TURN, totals[which])
            if stop > sent[which]:
                put(descriptor, stream(fill, sent[which], stop))
                sent[which] = stop
    if len(sys.argv) > 4 and sys.argv[4] == "held":
        if os.fork() == 0:
            os.setsid()
            os.close(2)
            time.sleep(float(sys.argv[5]))
            put(1, b"late line from the descendant\n")
            os._exit(0)
    os._exit(int(sys.argv[3]))


if __name__ == "__main__":
    main()
