#!/usr/bin/env python3
"""Freeze #241 functional positions; never select an offset from test outcomes."""
import argparse
import hashlib
from pathlib import Path

SIZES = (
    ("1mib", 1_048_576, "d7dfe3d2828aceb85177e6efbeb600f23672a326c902e525e401c1545bb05bdc", "8fafdf06fac9dbdffb7ccb6b1bde3b2460c387ef1abc55717dee8be401ff6078", 54),
    ("10mib", 10_485_760, "29c89128c748e4404f31b0147d447bd524d7b75afc98d56ac4debac762ee4b79", "dd79a6666e83927d787c8a7679b06f4c98ca5f80b6abd48d94b5e8f84aad1c85", 544),
    ("100mib", 104_857_600, "1bb2d79d54f72ae15eb0bb76ad715b9aafeba8ff8f9aa4f47bad3e3f101885bd", "bbee7155df021324495d88954be4db125eca49442b50aadc16439f61f6c32efe", 5394),
    ("500mib-capped", 524_283_904, "f1b6c61d9c126beba89dd2a310f727fd63cbbf131b793a78fe21247238c98c1f", "6c74b4ba6ad67f352a0bd85879a2f16a77511286bf9a73883d5c8858d2eded8f", 26994),
)
OPS = ("insert", "overwrite", "delete")
HEADER = "case_id\tsize_label\tpristine_bytes\top\tkind\toffset\tdelete_len\tinsert_len\tpayload\n"


def row(label, size, op, kind, offset):
    delete_len = 0 if op == "insert" else 4096
    insert_len = 0 if op == "delete" else 4096
    payload = "-" if op == "delete" else "/layerfs-bench/payloads/insert-middle-4k.bin"
    case_id = f"{label}-{op}-{kind}"
    return f"{case_id}\t{label}\t{size}\t{op}\t{kind}\t{offset}\t{delete_len}\t{insert_len}\t{payload}\n"


def bands():
    for label, size, _, _, _ in SIZES:
        for op in OPS:
            maximum = size if op == "insert" else size - 4096
            width = maximum + 1
            for band in range(16):
                lo = band * width // 16
                hi = (band + 1) * width // 16 - 1
                key = f"issue241-position-v1|{op}|{size}|{band}".encode()
                offset = lo + int.from_bytes(hashlib.sha256(key).digest(), "big") % (hi - lo + 1)
                yield row(label, size, op, f"band{band:02}", offset)


def boundaries(path):
    selected = {}
    lines = Path(path).read_text().splitlines()
    if not lines or lines[0] != "size_label\tpristine_bytes\tsha256\tcanonical_root\tcanonical_count\tboundary":
        raise ValueError("boundary receipt header")
    for line in lines[1:]:
        label, size, digest, root, count, boundary = line.split("\t")
        selected[label] = (int(size), digest, root, int(count), int(boundary))
    if set(selected) != {item[0] for item in SIZES}:
        raise ValueError("boundary receipt size set")
    for label, size, digest, root, count in SIZES:
        actual = selected[label]
        if actual[:4] != (size, digest, root, count) or not 0 < actual[4] < size:
            raise ValueError(f"unvalidated boundary receipt: {label}")
    return selected


def edges(selected):
    for label, size, _, _, _ in SIZES:
        boundary = selected[label][4]
        for op in OPS:
            maximum = size if op == "insert" else size - 4096
            choices = (("head", 0), ("midpoint", maximum // 2), ("last", maximum),
                       ("chunk-before", boundary - 1), ("chunk-at", boundary),
                       ("chunk-after", boundary + 1))
            for kind, offset in choices:
                if not 0 <= offset <= maximum:
                    raise ValueError(f"illegal {label} {op} {kind}")
                yield row(label, size, op, kind, offset)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--boundaries", type=Path,
                        help="validated Core FastCdc receipt; omit only for a bands-only precursor")
    args = parser.parse_args()
    rows = list(bands())
    assert len(rows) == 192 and len({line.split("\t", 1)[0] for line in rows}) == 192
    if args.boundaries:
        rows.extend(edges(boundaries(args.boundaries)))
        assert len(rows) == 264
    data = (HEADER + "".join(rows)).encode()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open("xb") as output:
        output.write(data)
    print(f"{args.out}: {len(rows)} cases sha256={hashlib.sha256(data).hexdigest()}")


if __name__ == "__main__":
    main()
