"""Every payload record of the control row's Store, measured frame against payload.

Count-driven and byte-driven, over the receipt already on disk: no arm is
sampled. Reports, per payload lane, how many stored frames are *not* narrower than
the payload they describe - the population the post-hoc rule of this round stores
verbatim at no codec cost - and the width distribution of the difference.

The native lane's record carries its own raw length (`[tag][raw_len u32][frame]`),
so that lane needs no locator join. The compact whole-file lane drops its two
length fields at assembly, so its payload length comes from the locator row, which
is unambiguous there: one record per group.

Run: python3 frame-widths.py <sample.sqlite>
"""
import sqlite3, struct, sys, statistics

WHOLE_FILE, NATIVE = 11, 10
HEADER = 24
WHOLE_FILE_PAYLOAD_OVERHEAD = 23


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


def main():
    con = sqlite3.connect('file:%s?mode=ro' % sys.argv[1], uri=True)
    canonical = {}
    for pid, g, cl in con.execute('select pack_id, group_number, canonical_length from objects'):
        canonical[(pid, g)] = cl
    widths = {'whole_file': [], 'native': []}
    tag_tally = {'whole_file': {}, 'native': {}}
    shorter = {'whole_file': 0, 'native': 0}
    for pack_id, data in con.execute('select pack_id, data from object_packs order by pack_id'):
        version = struct.unpack_from('<I', data, 8)[0]
        if version not in (WHOLE_FILE, NATIVE):
            continue
        lane = 'whole_file' if version == WHOLE_FILE else 'native'
        for ordinal, (s, e) in enumerate(groups(data, version)):
            if version == WHOLE_FILE:
                tag = data[s]
                tag_tally[lane][tag] = tag_tally[lane].get(tag, 0) + 1
                if tag != 0:
                    continue
                payload = canonical[(pack_id, ordinal)] - WHOLE_FILE_PAYLOAD_OVERHEAD
                frame = e - s - 1
            else:
                count = struct.unpack_from('<I', data, s)[0]
                ends = [struct.unpack_from('<I', data, s + 4 + 4 * i)[0] for i in range(count)]
                area = s + 4 + 4 * count
                prev = 0
                for end in ends:
                    blob = data[area + prev:area + end]
                    prev = end
                    tag = blob[0]
                    tag_tally[lane][tag] = tag_tally[lane].get(tag, 0) + 1
                    if tag != 0:
                        continue
                    payload = struct.unpack_from('<I', blob, 1)[0]
                    frame = len(blob) - 5
                    widths[lane].append(frame - payload)
                    if frame >= payload:
                        shorter[lane] += 1
                continue
            widths[lane].append(frame - payload)
            if frame >= payload:
                shorter[lane] += 1
    for lane in ('whole_file', 'native'):
        w = widths[lane]
        print('%-11s records %6d  frame-payload: min %5d  median %5d  max %6d  '
              'frame >= payload: %6d' % (
                  lane, len(w), min(w), int(statistics.median(w)), max(w), shorter[lane]))
        print('%-11s tags %s' % ('', tag_tally[lane]))
    total = sum(shorter.values())
    print('records the post-hoc rule stores verbatim (frame >= payload): %d' % total)


main()
