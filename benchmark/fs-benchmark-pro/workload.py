#!/usr/bin/env python3
"""Neutral fs-benchmark-pro file workload; uses only the Python standard library."""

import hashlib
import os
import shutil
import sys
import tempfile

PREPEND = b"PREPEND010"


def digest(path):
    result = hashlib.sha256()
    size = 0
    with open(path, "rb", buffering=0) as source:
        while block := source.read(1024 * 1024):
            size += len(block)
            result.update(block)
    return size, result.hexdigest()


def create(fixture, path):
    with open(fixture, "rb", buffering=0) as source, open(path, "wb", buffering=0) as target:
        shutil.copyfileobj(source, target, 1024 * 1024)
        os.fsync(target.fileno())


def edit(path, index, base_size):
    if base_size <= 10:
        raise ValueError("base size must exceed marker length")
    marker = f"E{index + 1:09d}".encode("ascii")
    offset = ((index + 1) * 2_654_435_761) % (base_size - len(marker))
    with open(path, "r+b", buffering=0) as target:
        target.seek(offset)
        target.write(marker)
        os.fsync(target.fileno())


def prepend(path):
    temporary = path + ".prepend.tmp"
    with open(path, "rb", buffering=0) as source, open(temporary, "wb", buffering=0) as target:
        target.write(PREPEND)
        shutil.copyfileobj(source, target, 1024 * 1024)
        os.fsync(target.fileno())
    os.replace(temporary, path)


def print_digest(path):
    size, sha256 = digest(path)
    print(f"{size}\t{sha256}")


def self_check():
    with tempfile.TemporaryDirectory(prefix="fs-benchmark-pro-") as root:
        fixture = os.path.join(root, "fixture.bin")
        payload = os.path.join(root, "payload.bin")
        with open(fixture, "wb") as target:
            target.write(bytes(range(256)) * 32)
        create(fixture, payload)
        edit(payload, 0, os.path.getsize(fixture))
        prepend(payload)
        actual = open(payload, "rb").read()
        expected = bytearray(open(fixture, "rb").read())
        marker = b"E000000001"
        offset = 2_654_435_761 % (len(expected) - len(marker))
        expected[offset : offset + len(marker)] = marker
        expected = PREPEND + expected
        assert actual == expected
        size, sha256 = digest(payload)
        assert size == len(expected)
        assert sha256 == hashlib.sha256(expected).hexdigest()
    print('{"schema":"fs-benchmark-pro-workload-self-check-v1","status":"pass"}')


def main(argv):
    if argv == ["self-check"]:
        self_check()
    elif len(argv) == 2 and argv[0] in {"digest", "read"}:
        print_digest(argv[1])
    elif len(argv) == 3 and argv[0] == "create":
        create(argv[1], argv[2])
    elif len(argv) == 4 and argv[0] == "edit":
        edit(argv[1], int(argv[2]), int(argv[3]))
    elif len(argv) == 2 and argv[0] == "prepend":
        prepend(argv[1])
    elif len(argv) == 4 and argv[0] == "verify":
        size, sha256 = digest(argv[1])
        if size != int(argv[2]) or sha256 != argv[3]:
            raise SystemExit(
                f"verification mismatch: size={size} sha256={sha256} "
                f"expected_size={argv[2]} expected_sha256={argv[3]}"
            )
        print(f"{size}\t{sha256}")
    else:
        raise SystemExit(
            "usage: workload.py self-check | digest|read PATH | create FIXTURE PATH | "
            "edit PATH INDEX BASE_SIZE | prepend PATH | verify PATH SIZE SHA256"
        )


if __name__ == "__main__":
    main(sys.argv[1:])
