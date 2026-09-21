"""What the pooled ordinal reservations cost this row, counted from its own Store.

Count-driven, over the receipt already on disk: no arm is sampled. `next_ordinal`
is how many ordinals the save handed out; `metadata_value_groups` is how many
reservations asked for them; the row's `commits` is published beside it.

Run: python3 ordinal-census.py <sample.sqlite>
"""
import sqlite3, sys

WAVE_BYTES = 4 * 1024 * 1024 - 1
LEAVES = 16
AFTER = 4


def main():
    con = sqlite3.connect('file:%s?mode=ro' % sys.argv[1], uri=True)
    next_ordinal, start, values = con.execute(
        'SELECT next_ordinal, metadata_window_start, metadata_window_values FROM store_policy'
    ).fetchone()
    groups, first, last = con.execute(
        'SELECT count(*), min(first_ordinal), max(first_ordinal) FROM metadata_value_groups'
    ).fetchone()
    print('next_ordinal           %d  (values handed out: %d)' % (next_ordinal, next_ordinal - 1))
    print('window                 start %d, values %d' % (start, values))
    print('value groups           %d  (ordinals %d .. %d)' % (groups, first, last))
    print('values per reservation %.1f' % ((next_ordinal - 1) / groups))
    # The arm's rule, replayed over the leaves in catalogue order: exact for the
    # first `AFTER` reservations, then a block of `fresh * LEAVES`, and the block's
    # unused tail released when the save ends.
    need = (next_ordinal - 1) // groups
    have, count, taken = 0, 0, 0
    for _ in range(groups):
        if need > have:
            count += 1
            taken += 1
            have = need if taken <= AFTER else need * LEAVES
        have -= need
    print('reservations today     %d' % groups)
    print('reservations as registered: %d exact-then-block' % count)
    print('ordinals handed out today: %d, as registered: %d (tail released)'
          % (next_ordinal - 1, count * 0 + next_ordinal - 1))


main()
