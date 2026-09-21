"""Every payload on this fixture, classified the way the treatment classifies it.

Count-driven and byte-driven: reads the *existing* receipt's Store, extracts each
payload frame from the pack rows exactly as the reader does (the last whole-file
group ends at the pack's declared `used` length, not at the BLOB's capacity), and
asks the pinned zstd CLI for the size of a 1024-byte prefix of it. No arm is
sampled; the script visits every payload record of both payload lanes.

Run: python3 probe-coverage.py <sample.sqlite>
"""
import sqlite3, struct, subprocess, sys, collections

WHOLE_FILE, NATIVE = 11, 10
HEADER = 24
DIR_CAP = {WHOLE_FILE: 4 * 256, NATIVE: 16 * 256}
MAGIC = bytes.fromhex('28b52ffd')


def used_length(data):
    return struct.unpack_from('<I', data, 16)[0]


def groups(data, version):
    count = struct.unpack_from('<I', data, 12)[0]
    if version == WHOLE_FILE:
        starts = [struct.unpack_from('<I', data, HEADER + 4 * i)[0] for i in range(count)]
        starts.append(used_length(data))
        return [(starts[i], starts[i + 1]) for i in range(count)]
    out = []
    for i in range(count):
        start, encoded, decoded, codec = struct.unpack_from('<IIIB', data, HEADER + 16 * i)
        if codec != 0 or encoded != decoded:
            raise SystemExit('a native group is not stored raw')
        out.append((start, start + encoded))
    return out


def records(data, version, start, end):
    if version == WHOLE_FILE:
        return [(data[start], data[start + 1:end])]
    count = struct.unpack_from('<I', data, start)[0]
    ends = [struct.unpack_from('<I', data, start + 4 + 4 * i)[0] for i in range(count)]
    area = start + 4 + 4 * count
    out, prev = [], 0
    for e in ends:
        blob = data[area + prev:area + e]
        tag = blob[0]
        frame = blob[5:] if tag == 0 else blob[5 + 32:]
        out.append((tag, frame))
        prev = e
    return out


def content_size(frame):
    """The frame content size a whole-file payload frame declares, or None."""
    fhd = frame[4]
    fcs_flag = fhd >> 6
    single = (fhd >> 5) & 1
    dict_flag = fhd & 3
    at = 5
    if not single:
        at += 1
    at += [0, 1, 2, 4][dict_flag]
    size = [1, 2, 4, 8][fcs_flag] if fcs_flag else (1 if single else 0)
    if not size:
        return None
    raw = frame[at:at + size]
    return int.from_bytes(raw, 'little')


def probe(payload):
    p = subprocess.run(['zstd', '-3', '-q', '-c', '--no-check'], input=payload,
                       stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    return len(p.stdout)


def main():
    con = sqlite3.connect('file:%s?mode=ro' % sys.argv[1], uri=True)
    declared = {}
    for pid, g, cl in con.execute('select pack_id, group_number, canonical_length from objects'):
        declared[(pid, g)] = cl
    tally = collections.Counter()
    sizes = collections.Counter()
    overhead = {WHOLE_FILE: 23, NATIVE: 21}
    mismatched = 0
    checked = 0
    verified = 0
    for pack_id, data in con.execute('select pack_id, data from object_packs order by pack_id'):
        version = struct.unpack_from('<I', data, 8)[0]
        if version not in (WHOLE_FILE, NATIVE):
            continue
        lane = 'whole_file' if version == WHOLE_FILE else 'native'
        for ordinal, (s, e) in enumerate(groups(data, version)):
            for tag, frame in records(data, version, s, e):
                tally[lane + '/records'] += 1
                if tag != 0:
                    tally[lane + '/tag%d' % tag] += 1
                    continue
                if frame[:4] != MAGIC:
                    tally[lane + '/not_a_frame'] += 1
                    continue
                expect = declared.get((pack_id, ordinal))
                if verified < 200 and expect is not None:
                    verified += 1
                    out = subprocess.run(['zstd', '-d', '-q', '-c'], input=frame,
                                         stdout=subprocess.PIPE, stderr=subprocess.DEVNULL).stdout
                    checked += 1
                    if version != WHOLE_FILE and len(out) == 0:
                        mismatched += 1
                if len(frame) <= 1024:
                    tally[lane + '/payload_le_1024'] += 1
                    continue
                got = probe(frame[:1024])
                sizes[(lane, got)] += 1
                if got * 32 >= 1024 * 31:
                    tally[lane + '/probe_says_store'] += 1
                else:
                    tally[lane + '/probe_says_compress'] += 1
    print('frames decoded with the pinned CLI as a parsing self-check: %d checked, %d refused'
          % (checked, mismatched))
    for key in sorted(tally):
        print('%-34s %d' % (key, tally[key]))
    print('probe output sizes on noise (payload > 1024):', dict(sizes))
    print('predicted stored records:',
          tally['whole_file/probe_says_store'] + tally['native/probe_says_store']
          + tally['whole_file/payload_le_1024'] + tally['native/payload_le_1024'])


main()
