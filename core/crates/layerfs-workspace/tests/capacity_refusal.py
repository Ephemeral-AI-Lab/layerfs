"""Strict external parser of opt-in product capacity lines; never infer missing zeroes."""
import argparse
import json
import re
from pathlib import Path

REFUSAL = re.compile(
    r'^LFS_CAPACITY_REFUSAL v=1 domain=(memory|physical) '
    r'operation=(reserve|metadata_reserve|payload_acquire) '
    r'site=([^\s]+) (.*)$'
)
CONTEXT = re.compile(r'^LFS_C5_FAILURE_CONTEXT v=1 known=(true|false) '
                     r'installed_revision=(NA|\d+) class=(capacity|backing|other) '
                     r'budget_used=(\d+) allocated=(NA|\d+) reserved=(NA|\d+) '
                     r'quota=(NA|\d+) active_revision=(NA|\d+) pages=(NA|\d+) pins=(NA|\d+)$')


def parse(text):
    refusals = []
    contexts = []
    for line in text.splitlines():
        match = REFUSAL.fullmatch(line)
        if match:
            domain, operation, site, rest = match.groups()
            expected = ({'used', 'request', 'limit'} if domain == 'memory' else
                        {'allocated', 'reserved', 'request', 'limit'})
            if (domain == 'memory') != (operation == 'reserve'):
                raise ValueError('inconsistent domain and operation')
            fields = {}
            for item in rest.split():
                key, sep, value = item.partition('=')
                if not sep or key in fields or key not in expected or not value.isdecimal():
                    raise ValueError('invalid refusal field')
                fields[key] = int(value)
            if fields.keys() != expected:
                raise ValueError('incomplete refusal')
            if (fields['used'] if domain == 'memory' else
                fields['allocated'] + fields['reserved']) + fields['request'] <= fields['limit']:
                raise ValueError('not a capacity refusal')
            refusals.append(dict(domain=domain, operation=operation, site=site, **fields))
        elif line.startswith('LFS_CAPACITY_REFUSAL '):
            raise ValueError('malformed refusal')
        else:
            match = CONTEXT.fullmatch(line)
            if match:
                contexts.append(dict(known=match[1] == 'true',
                                     installed_revision=match[2], failure_class=match[3],
                                     budget_used=int(match[4]), allocated=match[5],
                                     reserved=match[6], quota=match[7],
                                     active_revision=match[8], pages=match[9], pins=match[10]))
            elif line.startswith('LFS_C5_FAILURE_CONTEXT '):
                raise ValueError('malformed C5 context')
    return dict(refusals=refusals, c5_contexts=contexts)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--log', type=Path, required=True)
    parser.add_argument('--domain', choices=['memory', 'physical'], required=True)
    parser.add_argument('--c5-known-uninstalled', action='store_true')
    args = parser.parse_args()
    result = parse(args.log.read_text())
    if not any(item['domain'] == args.domain for item in result['refusals']):
        parser.error('required refusal domain absent (missing is not zero)')
    if args.c5_known_uninstalled and not any(
        item['known'] and item['installed_revision'] == 'NA'
        for item in result['c5_contexts']
    ):
        parser.error('required known-but-uninstalled context absent')
    print(json.dumps(result, sort_keys=True))
