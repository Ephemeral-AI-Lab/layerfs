"""External fail-closed observer for source-bound #273 public functional RCA.

Prepared index pages are not ACKs: only pair this report with an independently
verified public Stage result/known failure and the exact product identity.
"""
import argparse
import json
import re
from pathlib import Path

PREFIX = 'LFS_INDEX_PAGE_CAUSE v=1 '
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
        if not line.startswith(PREFIX):
            if line.startswith('LFS_INDEX_PAGE_CAUSE '):
                raise ValueError('unknown page-cause version')
            continue
        parts = {}
        for piece in line[len(PREFIX):].split():
            key, separator, value = piece.partition('=')
            if not separator or key in parts or key not in FIELDS:
                raise ValueError('duplicate/unknown/missing page-cause field')
            if key == 'direct':
                if value not in ('true', 'false'):
                    raise ValueError('bad direct flag')
                parts[key] = value == 'true'
            elif value.isdecimal():
                parts[key] = int(value)
            else:
                raise ValueError('bad numeric page-cause field')
        if parts.keys() != FIELDS:
            raise ValueError(f'incomplete page-cause line: {FIELDS-parts.keys()}')
        if (sum(parts[role] for role in ROLES) != parts['index_pages'] or
                parts['created_total'] != parts['index_pages'] + parts['directory_pages'] or
                parts['selected_height'] > 7 or parts['new_height'] > 7 or
                parts['captured'] > 32 or parts['hot_before'] > 64 or
                parts['hot_after'] > 64):
            raise ValueError('invalid page-cause accounting')
        rows.append(parts)
    return rows


def summarize(stderr, stdout, expected, generation=2):
    rows = [row for row in parse_rows(stderr) if row['generation'] == generation]
    if len(rows) != expected or len({row['revision'] for row in rows}) != expected:
        raise ValueError(f'expected {expected} distinct prepared revisions; got {len(rows)}')
    revisions = sorted(row['revision'] for row in rows)
    if revisions != list(range(revisions[0], revisions[0]+expected)):
        raise ValueError('missing/duplicate index revision')
    cause = re.search(r'GENERIC_WRITE_CAUSE writes=(\d+) .*? '
                      r'representation_only_pages=(\d+) seeks=(\d+) '
                      r'index_fetches=(\d+) index_writes=(\d+) pack_writes=(\d+)', stdout)
    if not cause or int(cause[1]) != expected:
        raise ValueError('missing public WRITE sample')
    totals = {name: sum(row[name] for row in rows) for name in ROLES | {'index_pages', 'directory_pages'}}
    if (totals['index_pages'] != int(cause[5]) or
            sum(totals[name] for name in ('admission_no_key', 'normalization_no_key', 'connection_no_key')) != int(cause[2])):
        raise ValueError('mutation role totals disagree with public WRITE counters')
    return {'writes': expected, 'generation': generation,
            'first_revision': revisions[0], 'last_revision': revisions[-1],
            'totals': totals,
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
