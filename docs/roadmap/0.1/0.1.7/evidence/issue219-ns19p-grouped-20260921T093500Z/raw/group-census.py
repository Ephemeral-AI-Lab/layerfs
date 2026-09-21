"""Object groups per lane in a receipt's Store, and what whole-file grouping would make.

Count-driven, over the receipt already on disk: no arm is sampled. One seal is one
`INSERT` statement and one `write_pack` call, so the group count is the population
both of those counters count, and `pipeline.statements` is the check.

The projection replays the owner's own seal rule - a group is sealed when the
framed length the *next* record would produce passes `GROUP_TARGET`
(`cas/selection.rs`, `pack::assemble::framed_group_length`) - over the whole-file
lane's records in the order the plan emits them.

Run: python3 group-census.py <sample.sqlite>
"""
import sqlite3, struct, sys
from collections import Counter, defaultdict

VERSIONS = {9: 'ordinary', 10: 'native', 11: 'whole', 12: 'pooled', 13: 'singleton',
            14: 'whole', 15: 'native', 16: 'singleton'}
GROUP_TARGET = 48 * 1024
RECORD_COUNT_LIMIT = 8191


def main():
    con = sqlite3.connect('file:%s?mode=ro' % sys.argv[1], uri=True)
    lane_of = {}
    for pid, data in con.execute('select pack_id, data from object_packs'):
        version = struct.unpack_from('<I', data, 8)[0]
        lane_of[pid] = VERSIONS.get(version, 'v%d' % version)
    groups = defaultdict(set)
    records = Counter()
    whole = []
    for pid, gn, rn, cl in con.execute(
            'select pack_id, group_number, record_number, canonical_length from objects'):
        lane = lane_of[pid]
        groups[lane].add((pid, gn))
        records[lane] += 1
        if lane == 'whole':
            whole.append((gn, rn, cl))
    total_groups = 0
    for lane in sorted(records):
        print('%-10s records %6d groups %6d' % (lane, records[lane], len(groups[lane])))
        total_groups += len(groups[lane])
    print('%-10s                    %6d' % ('TOTAL', total_groups))

    # Replay the seal rule over the whole-file lane in locator order.
    whole.sort(key=lambda row: (row[0], row[1]))
    projected, count, payload = 0, 0, 0
    for _, _, cl in whole:
        width = cl - 23 + 9  # the pending compact record: tag + two lengths + payload
        if count and (4 + 4 * (count + 1) + payload + width > GROUP_TARGET
                      or count >= RECORD_COUNT_LIMIT):
            projected += 1
            count, payload = 0, 0
        count += 1
        payload += width
    if count:
        projected += 1
    print('whole-file groups if grouped at %d B: %d (from %d)'
          % (GROUP_TARGET, projected, records['whole']))
    print('object groups after: %d (from %d), seals removed: %d'
          % (total_groups - records['whole'] + projected, total_groups,
             records['whole'] - projected))
    print('group framing added: %d bytes' % (records['whole'] * 4))


main()
