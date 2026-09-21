"""Diagnostic: does a 1024-byte prefix of this fixture's payloads compress?

Reads the *existing* receipt's Store (no new sample of any arm), extracts payload
frames straight out of the pack rows, and asks the pinned zstd CLI what a
1024-byte prefix of each one costs. Count-driven (bytes in, bytes out), never a
timing sample. Run:  python3 probe-classification.py <sample.sqlite> [limit]
"""
import sqlite3, subprocess, struct, sys, statistics

WHOLE_FILE, NATIVE = 11, 10
DIR_ENTRY = {WHOLE_FILE: 4, NATIVE: 16}
DIR_CAP = {WHOLE_FILE: 4 * 256, NATIVE: 16 * 256}
HEADER = 24


def groups(data, version):
    cap = DIR_CAP[version]
    body0 = HEADER + cap
    count = struct.unpack_from('<I', data, 12)[0]
    if version == WHOLE_FILE:
        starts = [struct.unpack_from('<I', data, HEADER + 4 * i)[0] for i in range(count)]
        starts.append(len(data))
        return [(starts[i], starts[i + 1]) for i in range(count)]
    out = []
    for i in range(count):
        off = HEADER + 16 * i
        start, encoded, decoded, codec = struct.unpack_from('<IIIB', data, off)
        if codec != 0 or encoded != decoded:
            raise SystemExit('native group is not stored raw')
        out.append((start, start + encoded))
    return out


def records(data, version, start, end):
    if version == WHOLE_FILE:
        # compact: [tag][frame...]; the two length fields are dropped at assembly
        return [(data[start], data[start + 1:end])]
    count = struct.unpack_from('<I', data, start)[0]
    ends = [struct.unpack_from('<I', data, start + 4 + 4 * i)[0] for i in range(count)]
    area = start + 4 + 4 * count
    out, prev = [], 0
    for e in ends:
        blob = data[area + prev:area + e]
        tag = blob[0]
        if tag == 0:
            frame = blob[5:]
        elif tag == 1:
            frame = blob[5 + 32:]
        else:
            raise SystemExit('unknown tag %d' % tag)
        out.append((tag, frame))
        prev = e
    return out


def zstd_len(payload):
    p = subprocess.run(['zstd', '-3', '-q', '-c', '--no-check'], input=payload,
                       stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    return len(p.stdout)


def main():
    path = sys.argv[1]
    limit = int(sys.argv[2]) if len(sys.argv) > 2 else 400
    con = sqlite3.connect('file:%s?mode=ro' % path, uri=True)
    seen = {'whole_file': 0, 'native': 0}
    ratios = []
    samples = 0
    for pack_id, data in con.execute('select pack_id, data from object_packs order by pack_id'):
        version = struct.unpack_from('<I', data, 8)[0]
        if version not in (WHOLE_FILE, NATIVE):
            continue
        key = 'whole_file' if version == WHOLE_FILE else 'native'
        for (s, e) in groups(data, version):
            for tag, frame in records(data, version, s, e):
                seen[key] += 1
                if samples >= limit or len(frame) < 2048:
                    continue
                samples += 1
                head = frame[:1024]
                probe = zstd_len(head)
                ratios.append((probe, len(head), len(frame)))
    print('records seen:', seen)
    print('payloads probed:', samples)
    if ratios:
        over = [r for r in ratios if r[0] >= r[1]]
        print('probe >= sample (would store raw): %d of %d' % (len(over), len(ratios)))
        print('probe/sample  min %.4f  median %.4f  max %.4f' % (
            min(r[0] / r[1] for r in ratios),
            statistics.median(r[0] / r[1] for r in ratios),
            max(r[0] / r[1] for r in ratios)))
        print('full/sample   min %.4f  median %.4f  max %.4f' % (
            min(r[2] / r[1] for r in ratios),
            statistics.median(r[2] / r[1] for r in ratios),
            max(r[2] / r[1] for r in ratios)))


main()
