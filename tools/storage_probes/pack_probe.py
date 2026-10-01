"""Real C1/C2 native packs over S3; independent Python physical/byte oracle."""
import argparse
import compression.zstd
import hashlib
import json
from pathlib import Path
import random
import struct
import subprocess
import time

from common import S3, phase, write_json

CASES = {}
for width in (8, 16, 32):
    for mode in ('object', 'pack'):
        CASES[f'P-{width}-{mode}-v2'] = (width * 1024, 'random', mode)
for mode in ('object', 'pack'):
    CASES[f'P-16-text-{mode}-v2'] = (16384, 'text', mode)
TOTAL = 8 * 1048576


def decode(record):
    assert len(record) >= 6 and record[0] in (0, 2)
    width = struct.unpack_from('<I', record, 1)[0]
    raw = record[5:] if record[0] == 2 else compression.zstd.decompress(record[5:])
    assert len(raw) == width and width in (8192, 16384, 32768)
    return raw


def records_in_pack(data):
    assert len(data) <= 262144 and data[:8] == b'LFPACK\0\0'
    version, groups, used, reserved = struct.unpack_from('<IIII', data, 8)
    assert version == 15 and 1 <= groups <= 256 and used == len(data) and reserved == 0
    assert data[24 + groups * 16:4120] == bytes(4096 - groups * 16)
    position, found = 4120, []
    for g in range(groups):
        start, length, decoded, flags = struct.unpack_from('<IIII', data, 24 + g * 16)
        assert start == position and length == decoded and flags == 0 and length <= 65536
        end = start + length
        assert end <= len(data)
        count = struct.unpack_from('<I', data, start)[0]
        assert 1 <= count <= 8191
        ends = struct.unpack_from('<' + 'I' * count, data, start + 4)
        record_start = start + 4 + 4 * count
        previous = 0
        for n, finish in enumerate(ends):
            assert previous < finish <= end - record_start
            found.append((g, n, record_start + previous, record_start + finish))
            previous = finish
        assert record_start + previous == end
        position = end
    assert position == len(data)
    return found


def prepare(root, binary):
    root = Path(root)
    root.mkdir(exist_ok=False, parents=True)
    raw = random.Random(2912).randbytes(TOTAL)
    (root / 'random.bin').write_bytes(raw)
    text = bytearray((b'LayerFS chunk fixture\n' * (TOTAL // 21 + 1))[:TOTAL])
    for i in range(TOTAL // 16384):
        text[i * 16384:i * 16384 + 16] = f'{i:016d}'.encode()
    (root / 'text.bin').write_bytes(text)
    inventory = {'binary_sha256': hashlib.sha256(Path(binary).read_bytes()).hexdigest(),
                 'raw_sha256': {name: hashlib.sha256((root / (name + '.bin')).read_bytes()).hexdigest()
                                for name in ('random', 'text')}, 'shapes': {}}
    for width, kind in [(8192, 'random'), (16384, 'random'), (32768, 'random'), (16384, 'text')]:
        shape = f'{kind}-{width}'
        output = root / shape
        argv = [str(Path(binary).resolve()), str(root / (kind + '.bin')), str(output), str(width)]
        started = time.monotonic_ns()
        result = subprocess.run(argv, check=True, capture_output=True, text=True)
        manifest = json.loads((output / 'manifest.json').read_text())
        reference = (root / (kind + '.bin')).read_bytes()
        all_locations = []
        for pack in sorted(output.glob('pack-*.bin')):
            data = pack.read_bytes()
            for group, record, start, end in records_in_pack(data):
                all_locations.append({'index': len(all_locations), 'pack': pack.name,
                                      'group': group, 'record': record, 'start': start, 'end': end})
        assert manifest['locators'] == all_locations
        for loc in all_locations:
            i = loc['index']
            record = (output / f'record-{i:05d}.bin').read_bytes()
            data = (output / loc['pack']).read_bytes()[loc['start']:loc['end']]
            assert data == record and decode(record) == reference[i * width:(i + 1) * width]
        inventory['shapes'][shape] = {'command': argv, 'exit': result.returncode,
            'wall_ns': time.monotonic_ns() - started, 'construction_ns': manifest['construction_ns'],
            'records': manifest['records'], 'packs': manifest['packs'],
            'stored_records': manifest['stored_records'],
            'artifact_sha256': {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                                for p in sorted(output.glob('*.bin'))}, 'proof': 'PASS'}
    write_json(root / 'fixtures.json', inventory)


def shape_info(case, fixtures):
    width, kind, mode = CASES[case]
    root = Path(fixtures)
    directory = root / f'{kind}-{width}'
    manifest = json.loads((directory / 'manifest.json').read_text())
    return width, kind, mode, directory, manifest


def save_transcript(path, rows):
    with Path(path).open('xb') as out:
        for index, body in rows:
            out.write(struct.pack('<II', index, len(body)))
            out.write(body)


def transcript(path):
    data = Path(path).read_bytes()
    position = 0
    while position < len(data):
        index, size = struct.unpack_from('<II', data, position)
        position += 8
        yield index, data[position:position + size]
        position += size
    assert position == len(data)


def execute(action, case, provider, fixtures, output):
    output = Path(output)
    width, kind, mode, directory, manifest = shape_info(case, fixtures)
    prefix = case + '/'
    paths = sorted(directory.glob(('record' if mode == 'object' else 'pack') + '-*.bin'))
    config = json.loads((Path(provider) / 'private-config.json').read_text())
    client = S3(config)
    locations = manifest['locators']
    scattered = [(37 * k) % manifest['records'] for k in range(128)]
    if action == 'perf':
        output.mkdir(exist_ok=False, parents=True)
        record = {'case': case, 'logical_bytes': TOTAL, 'mode': mode,
                  'objects': len(paths), 'chunk_bytes': width, 'records': manifest['records'],
                  'cache_verdict': 'INELIGIBLE', 'performance_claim': False,
                  'cache_contract': 'fixture file/server/OS residency UNKNOWN; reads follow PUT',
                  'construction_scope': 'shared preparation diagnostic writes both record and pack forms'}
        encoded_bytes = sum(p.stat().st_size for p in paths)
        def upload():
            for p in paths:
                body = p.read_bytes()
                client.call('PUT', prefix + p.name, size=len(body),
                            payload_hash=hashlib.sha256(body).hexdigest(), block=body)
        phase(record, 'put', len(paths), encoded_bytes, upload)
        def sequential():
            rows = []
            index = 0
            for p in paths:
                body = client.call('GET', prefix + p.name)
                if mode == 'object':
                    rows.append((index, body)); index += 1
                else:
                    for _, _, start, end in records_in_pack(body):
                        rows.append((index, body[start:end])); index += 1
            return rows
        # Pack framing/extraction is included; payload decode/hash proof is excluded.
        rows = phase(record, 'sequential_get', len(paths), encoded_bytes, sequential)
        save_transcript(output / 'sequential.bin', rows)
        def scatter(ranged):
            rows = []
            for i in scattered:
                if mode == 'object':
                    body = client.call('GET', prefix + f'record-{i:05d}.bin')
                else:
                    loc = locations[i]
                    headers = {'Range': f"bytes={loc['start']}-{loc['end'] - 1}"} if ranged else None
                    body = client.call('GET', prefix + loc['pack'], request_headers=headers)
                    if not ranged:
                        body = body[loc['start']:loc['end']]
                rows.append((i, body))
            return rows
        for label, ranged in [('scattered_whole_get', False)] + (
                [('scattered_range_get', True)] if mode == 'pack' else []):
            before_bytes, before_calls = client.received, client.calls
            rows = phase(record, label, 128, 128 * width, lambda: scatter(ranged))
            record['phases'][-1]['http_received_bytes'] = client.received - before_bytes
            record['phases'][-1]['requests'] = client.calls - before_calls
            save_transcript(output / (label + '.bin'), rows)
        record.update({'sent_bytes': client.sent, 'received_bytes': client.received,
                       'requests': client.calls, 'encoded_bytes': encoded_bytes})
        write_json(output / 'performance.json', record)
    else:
        started = time.monotonic_ns()
        reference = (Path(fixtures) / (kind + '.bin')).read_bytes()
        checked = 0
        for path in sorted(output.glob('*.bin')):
            rows = list(transcript(path))
            expected = list(range(manifest['records'])) if path.name == 'sequential.bin' else scattered
            assert [i for i, _ in rows] == expected
            for i, body in rows:
                assert decode(body) == reference[i * width:(i + 1) * width]
                checked += 1
        assert client.listing(prefix)[0] == [prefix + p.name for p in paths]
        proof_ns = time.monotonic_ns() - started
        cleanup = time.monotonic_ns()
        for p in paths:
            client.call('DELETE', prefix + p.name)
        assert client.listing(prefix)[0] == []
        write_json(output / 'verification.json', {'case': case, 'proof': 'PASS', 'cleanup': 'PASS',
            'records_checked': checked, 'proof_ns': proof_ns,
            'cleanup_ns': time.monotonic_ns() - cleanup,
            'oracle': 'original Python fixture bytes; independent physical parser and stdlib zstd'})
    client.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=['setup', 'perf', 'verify'])
    parser.add_argument('--case', choices=CASES)
    parser.add_argument('--provider')
    parser.add_argument('--fixtures', required=True)
    parser.add_argument('--output')
    parser.add_argument('--binary')
    args = parser.parse_args()
    if args.action == 'setup':
        prepare(args.fixtures, args.binary)
    else:
        execute(args.action, args.case, args.provider, args.fixtures, args.output)
