"""External fail-closed observer for source-bound #273 public functional RCA.

Prepared index pages are not ACKs: only pair this report with an independently
verified public Stage result/known failure and the exact product identity.
"""
import argparse
import json
import re
from pathlib import Path

PREFIX = 'LFS_INDEX_PAGE_CAUSE v=1 '
PREFIX_V2 = 'LFS_INDEX_PAGE_CAUSE v=2 '
EXTRA = {'generic_split_events', 'generic_split_pages', 'direct_carries'}
FIELDS = {'generation', 'revision', 'update_keys', 'selected_height', 'new_height',
          'captured', 'frozen_revisions', 'direct', 'changed_leaf', 'changed_parent',
          'direct_leaf', 'direct_parent', 'admission_no_key', 'normalization_no_key',
          'connection_no_key', 'height_pages', 'index_pages', 'directory_pages',
          'created_total', 'replaced', 'hot_before', 'hot_after'}
ROLES = {'changed_leaf', 'changed_parent', 'direct_leaf', 'direct_parent',
         'admission_no_key', 'normalization_no_key', 'connection_no_key', 'height_pages'}


def parse_rows(text):
    rows = []
    for line in text.splitlines():
        prefix = PREFIX_V2 if line.startswith(PREFIX_V2) else PREFIX
        if not line.startswith(prefix):
            if line.startswith('LFS_INDEX_PAGE_CAUSE '):
                raise ValueError('unknown page-cause version')
            continue
        expected = FIELDS | (EXTRA if prefix == PREFIX_V2 else set())
        parts = {}
        for piece in line[len(prefix):].split():
            key, separator, value = piece.partition('=')
            if not separator or key in parts or key not in expected:
                raise ValueError('duplicate/unknown/missing page-cause field')
            if key == 'direct':
                if value not in ('true', 'false'):
                    raise ValueError('bad direct flag')
                parts[key] = value == 'true'
            elif value.isdecimal():
                parts[key] = int(value)
            else:
                raise ValueError('bad numeric page-cause field')
        if parts.keys() != expected:
            raise ValueError(f'incomplete page-cause line: {expected-parts.keys()}')
        if (prefix == PREFIX_V2 and
                (parts['generic_split_pages'] < 2*parts['generic_split_events'] or
                 (not parts['direct'] and parts['direct_carries']))):
            raise ValueError('invalid split/carry witness')
        if (sum(parts[role] for role in ROLES) != parts['index_pages'] or
                parts['created_total'] != parts['index_pages'] + parts['directory_pages'] or
                parts['selected_height'] > 7 or parts['new_height'] > 7 or
                parts['captured'] > 32 or parts['hot_before'] > 64 or
                parts['hot_after'] > 64):
            raise ValueError('invalid page-cause accounting')
        rows.append(parts)
    return rows


def summarize(stderr, stdout, expected, generation=2):
    candidates = [row for row in parse_rows(stderr) if row['generation'] == generation]
    # The first generation-2 candidate may be the prior base Commit's local
    # C5 publication. Its capture is held; the subsequent WRITE group begins
    # only after that pin has been released. Never credit it to a WRITE.
    prefix, rows = candidates[:-expected], candidates[-expected:]
    if (expected <= 0 or len(rows) != expected or
            (prefix and (len(prefix) != 1 or prefix[0]['captured'] == 0)) or
            len({row['revision'] for row in rows}) != expected):
        raise ValueError(f'expected {expected} distinct prepared revisions; got {len(candidates)}')
    revisions = sorted(row['revision'] for row in rows)
    if revisions != list(range(revisions[0], revisions[0]+expected)):
        raise ValueError('missing/duplicate index revision')
    cause = re.search(r'GENERIC_WRITE_CAUSE writes=(\d+) .*? '
                      r'representation_only_pages=(\d+) seeks=(\d+) '
                      r'index_fetches=(\d+) index_writes=(\d+) pack_writes=(\d+)', stdout)
    if not cause or int(cause[1]) != expected:
        raise ValueError('missing public WRITE sample')
    counted = ROLES | {'index_pages', 'directory_pages'}
    if all('generic_split_events' in row for row in rows):
        counted |= EXTRA
    elif any('generic_split_events' in row for row in rows):
        raise ValueError('mixed observer versions')
    totals = {name: sum(row[name] for row in rows) for name in counted}
    # StoreStatus.index_page_writes counts HotDirectory too; the disjoint
    # page-cause buckets intentionally classify only leaf/branch versions.
    if (totals['index_pages'] + totals['directory_pages'] != int(cause[5]) or
            sum(totals[name] for name in ('admission_no_key', 'normalization_no_key', 'connection_no_key')) != int(cause[2])):
        raise ValueError('mutation role totals disagree with public WRITE counters')
    return {'writes': expected, 'generation': generation,
            'first_revision': revisions[0], 'last_revision': revisions[-1],
            'totals': totals,
            'excluded_prior_c5_revisions': [row['revision'] for row in prefix],
            'rows': rows, 'status': 'SOURCE_COUNT_ONLY', 'admission_eligible': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--stderr', type=Path, required=True)
    parser.add_argument('--stdout', type=Path, required=True)
    parser.add_argument('--expected-writes', type=int, required=True)
    parser.add_argument('--generation', type=int, default=2)
    args = parser.parse_args()
    result = summarize(args.stderr.read_text(), args.stdout.read_text(),
                       args.expected_writes, args.generation)
    print(json.dumps(result, sort_keys=True))
