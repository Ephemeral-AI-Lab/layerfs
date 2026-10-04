"""Independent closed-owner census for supported-profile history qualification.

Call only after the measured child exits. This and reference metadata export are
part of the separate 9.5-second proof command, never untimed admission setup.
No expected result is imported into a measured producer.
"""
import hashlib
import json
from pathlib import Path
import sqlite3
from urllib.parse import quote

LITE_POLICY = 'all-state-structure-five-anchor-bounded-content-v1'
BASE = '7edddbdb8e8512627aed0ed42533ef099d802384'
CANONICAL = {'history-stride10': (51689, 380559460),
             'history-stride3': (73447, 589480854),
             'history-stride1': (104618, 871337620)}
C2 = {'store_policy', 'saves', 'object_packs', 'metadata_value_groups',
      'objects', 'content_signatures'}
C5 = {'history_meta', 'layer_stacks', 'layers', 'commits', 'branches',
      'workspace_stages', 'scope_allocator'}
COMBINED = {'store_policy', 'pack', 'object_location', 'metadata_value_group',
            'content_signature', 'history_meta', 'layer_stack', 'layer',
            'commit', 'branch', 'workspace_stage', 'scope_allocator'}


def digest(path):
    value = hashlib.sha256()
    with Path(path).open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            value.update(block)
    return value.hexdigest()


def owner(path, tables, application, version):
    path = Path(path)
    info = path.lstat()
    if path.is_symlink() or not path.is_file() or info.st_nlink != 1:
        raise ValueError('history proof requires an exclusive regular closed owner')
    if any(Path(str(path) + s).exists() for s in ('-wal', '-shm', '-journal')):
        raise ValueError('history proof refuses outstanding database sidecars')
    before = digest(path)
    # Immutable read is safe only for this exclusively owned, exited-child file
    # with no outstanding sidecars. It cannot create WAL read-side sidecars.
    db = sqlite3.connect('file:' + quote(str(path.resolve()), safe='/') + '?mode=ro&immutable=1', uri=True)
    try:
        if db.execute('PRAGMA application_id').fetchone()[0] != application or db.execute('PRAGMA user_version').fetchone()[0] != version:
            raise ValueError('history closed-owner schema identity mismatch')
        actual = {row[0] for row in db.execute("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'")}
        if actual != tables:
            raise ValueError('history closed-owner table set mismatch')
        if db.execute('PRAGMA quick_check').fetchall() != [('ok',)] or db.execute('PRAGMA foreign_key_check').fetchall():
            raise ValueError('history closed-owner integrity mismatch')
        rows = {t: db.execute(f'SELECT COUNT(*) FROM "{t}"').fetchone()[0] for t in sorted(tables)}
        return db, {'path': str(path), 'sha256_before': before, 'allocated_bytes': info.st_blocks * 512,
                    'length_bytes': info.st_size, 'rows': rows, 'application_id': application,
                    'user_version': version, 'immutable_read': True}
    except BaseException:
        db.close()
        raise


def unfinished_saves(db):
    return db.execute('SELECT COUNT(*) FROM saves WHERE active_slot IS NOT NULL OR publication IS NULL').fetchone()[0]


def collect(db_path, arm, child, row, metadata_output=None):
    """Authenticate counts/roots against actual C2/C5 rows; preserve original files.

    The namespace/content oracle is separate. A census does not prove that every
    canonical object decodes or every file byte matches the source corpus.
    """
    if arm not in ('baseline', 'candidate') or row not in CANONICAL:
        raise ValueError('explicit history census arm and row required')
    n = {'history-stride10': 17, 'history-stride3': 53, 'history-stride1': 157}[row]
    if child.get('states') != n or child.get('selected_states') != n or len(child.get('roots', [])) != n:
        raise ValueError('complete history census state count mismatch')
    paths = [Path(db_path)] if arm == 'candidate' else [Path(db_path), Path(str(db_path) + '.history.sqlite')]
    opened = []
    records = []
    try:
        for p, tables, app, version in ([(paths[0], COMBINED, 1279677264, 1)] if arm == 'candidate' else
                                      [(paths[0], C2, 1279677261, 10), (paths[1], C5, 1279677256, 1)]):
            connection, record = owner(p, tables, app, version)
            opened.append(connection); records.append(record)
        c2, c5 = opened[0], opened[-1]
        if arm == 'candidate':
            query = 'SELECT object_id, role, canonical_length FROM object_location ORDER BY object_id'
            counts = {'history_meta': 1, 'layer_stack': 1, 'branch': n, 'layer': n,
                      'commit': n - 1, 'workspace_stage': 0, 'scope_allocator': 0}
            root_query = 'SELECT hex(root_id) FROM layer'
            pack_bytes = c2.execute('SELECT COALESCE(SUM(length(body)),0) FROM pack').fetchone()[0]
        else:
            inconsistent = c2.execute('SELECT 1 FROM objects GROUP BY object_id HAVING MIN(object_role)!=MAX(object_role) OR MIN(canonical_length)!=MAX(canonical_length) LIMIT 1').fetchone()
            unfinished = unfinished_saves(c2)
            if inconsistent or unfinished or records[0]['rows']['saves'] != n:
                raise ValueError('reference has inconsistent identities or unfinished saves')
            query = 'SELECT object_id, MIN(object_role), MIN(canonical_length) FROM objects GROUP BY object_id ORDER BY object_id'
            counts = {'history_meta': 1, 'layer_stacks': 1, 'branches': n, 'layers': n,
                      'commits': n - 1, 'workspace_stages': 0, 'scope_allocator': 0}
            root_query = 'SELECT hex(root_id) FROM layers'
            pack_bytes = c2.execute('SELECT COALESCE(SUM(length(data)),0) FROM object_packs').fetchone()[0]
        if any(records[-1]['rows'][t] != expected for t, expected in counts.items()):
            raise ValueError('closed C5 retained-state row count mismatch')
        if sorted(r[0].lower() for r in c5.execute(root_query)) != sorted(child['roots']):
            raise ValueError('closed C5 retained roots disagree with producer')
        if metadata_output is not None:
            query = 'SELECT object_id, MIN(object_role), MIN(canonical_length), MIN(pack_id) FROM objects GROUP BY object_id ORDER BY object_id'
        value = hashlib.sha256(); objects = total = 0
        output = Path(metadata_output).open('x') if metadata_output is not None else None
        try:
            for values in c2.execute(query):
                oid, role, length = values[:3]
                if not isinstance(oid, bytes) or len(oid) != 32 or not 1 <= role <= 13 or not 1 <= length <= 16777216:
                    raise ValueError('closed C2 canonical identity malformed')
                line = f'{oid.hex()}\t{role}\t{length}\n'
                value.update(oid); value.update(bytes([role])); value.update(length.to_bytes(8, "little")); objects += 1; total += length
                if output is not None:
                    pack = values[3]
                    if type(pack) is not int or pack <= 0: raise ValueError('invalid reference physical pack hint')
                    output.write(line.removesuffix('\n') + f'\t{pack}\n')
        finally:
            if output is not None: output.close()
        if value.hexdigest() != child.get('canonical_inventory_sha256'):
            raise ValueError('closed canonical ID/role/length inventory disagrees with producer')
        if (objects, total) != CANONICAL[row] or (objects, total) != (child.get('canonical_objects'), child.get('canonical_bytes')):
            raise ValueError('closed canonical census disagrees with frozen selection/producer')
        result = {'status': 'CHECKED', 'scope': 'closed-owner schema/integrity, canonical IDs/roles/lengths and C5 roots/counts; namespace oracle separate',
                  'canonical_objects': objects, 'canonical_bytes': total, 'inventory_sha256': value.hexdigest(),
                  'pack_bodies_bytes': pack_bytes, 'owners': records,
                  'storage_bytes': sum(r['allocated_bytes'] for r in records)}
    finally:
        for connection in opened: connection.close()
    for path, record in zip(paths, records):
        after = digest(path)
        if after != record['sha256_before'] or any(Path(str(path) + s).exists() for s in ('-wal', '-shm', '-journal')):
            raise ValueError('history proof changed original owner or produced sidecars')
        record['sha256_after'] = after
    return result


def root_pins(receipt, census, proof, receipt_path, census_path, proof_path):
    """Create pins only from completed, cold, verified, budgeted reference evidence.

    Caller must seal and retain these three evidence files in the run manifest.
    Candidate runner must recheck file hashes and matched harness/vehicle identities.
    """
    if receipt.get('arm') != 'baseline' or receipt.get('measured_source_commit') != BASE:
        raise ValueError('unqualified reference source')
    if receipt.get('status') != 'COMPLETE' or receipt.get('cache_status') != 'PASS' or receipt.get('cleanup', {}).get('status') != 'PASS':
        raise ValueError('reference lifecycle/cold/cleanup incomplete')
    if receipt.get('sample_count') != 1 or receipt.get('verification_status') != 'PASS':
        raise ValueError('reference requires one arm and independent proof')
    lite = receipt.get('proof_policy') == LITE_POLICY
    allowed = 12_000_000_000 if lite else 9_500_000_000
    if lite and (proof.get('sample_policy') != LITE_POLICY or proof.get('authenticated_bytes', 8*1024*1024+1) > 8*1024*1024
                 or proof.get('acquired_content_bytes', 32*1024*1024+1) > 32*1024*1024):
        raise ValueError('bounded content proof scope/bytes mismatch')
    if receipt.get('verification_budget_ns', allowed) != allowed:
        raise ValueError('reference declared proof budget mismatch')
    if receipt['command_wall_ns'] > receipt['command_budget_ns'] or receipt['verification_wall_ns'] > allowed:
        raise ValueError('reference command/proof budget exceeded')
    child = receipt['performance']['child']
    if child.get('status') != 'COMPLETE' or child.get('profile_identity') != 'phase4.5-memory-off':
        raise ValueError('reference producer status/profile mismatch')
    if not receipt.get('identity') or not receipt.get('build', {}).get('binaries'):
        raise ValueError('reference source/harness/binary identities missing')
    if census.get('status') != 'CHECKED' or proof.get('status') != 'CHECKED' or proof.get('states') != child['states'] or proof.get('custody_states') != child['states']:
        raise ValueError('reference closed census/namespace/custody incomplete')
    if (census.get('canonical_objects'), census.get('canonical_bytes')) != CANONICAL[receipt['workload_row']]:
        raise ValueError('reference canonical census mismatch')
    evidence = {}
    for label, path, expected in [('receipt', receipt_path, receipt), ('census', census_path, census), ('proof', proof_path, proof)]:
        if json.loads(Path(path).read_text()) != json.loads(json.dumps(expected)):
            raise ValueError('reference evidence file disagrees with qualified record')
        evidence[label] = {'path': str(Path(path).resolve()), 'sha256': digest(path)}
    return {'kind': 'matched-phase4.5-root-pins-v1', 'source_commit': BASE,
            'row': receipt['workload_row'], 'roots': child['roots'], 'evidence': evidence,
            'identity': receipt['identity'], 'profile': 'phase4.5-memory-off'}


def validate_pins(pins, matched_identity):
    """Re-derive pins from hash-checked qualified reference evidence before use."""
    evidence = pins.get('evidence', {})
    if set(evidence) != {'receipt', 'census', 'proof'}:
        raise ValueError('root pins lack independent reference evidence')
    records = {}
    paths = {}
    for label, seal in evidence.items():
        path = Path(seal['path'])
        if digest(path) != seal['sha256']:
            raise ValueError('independent reference evidence hash mismatch')
        records[label] = json.loads(path.read_text()); paths[label] = path
    derived = root_pins(records['receipt'], records['census'], records['proof'],
                        paths['receipt'], paths['census'], paths['proof'])
    if pins != derived or pins['identity'] != matched_identity:
        raise ValueError('root pins do not match qualified reference/paired harness identity')
    return records


def native_environment(request, out):
    """Load the pinned observer in the native verifier, not the Python census.

    Keep native output separate from performance files and the parent process.
    A lite proof cannot run with missing or substituted acquisition counters.
    """
    import os
    env = os.environ.copy()
    if request.get('proof_policy') == LITE_POLICY:
        observer = request.get('observer', {})
        path = observer.get('path')
        if not path or digest(path) != observer.get('sha256'):
            raise ValueError('lite proof requires the pinned acquisition observer')
        env.update(DYLD_INSERT_LIBRARIES=path,
                   LAYERFS_SQLITE_WORK_OUTPUT=str(out/'proof-sql-work.json'),
                   LAYERFS_CAUSE_VFS_LOG=str(out/'proof-vfs.json'),
                   LAYERFS_CLOSE_OBSERVER_OUTPUT=str(out/'proof-close.json'))
    return env


def main():
    """One bounded proof child owns census/export, native proof and preservation."""
    import argparse
    import subprocess
    parser = argparse.ArgumentParser()
    parser.add_argument('--request', required=True)
    args = parser.parse_args()
    request = json.loads(Path(args.request).read_text())
    out = Path(request['out'])
    child = request['producer']
    native_env = native_environment(request, out)
    if request['arm'] == 'candidate':
        validate_pins(json.loads(Path(request['pins']).read_text()), request['identity'])
    metadata = out/'reference-metadata.tsv' if request['arm'] == 'baseline' else None
    census = collect(request['db'], request['arm'], child, request['row'], metadata)
    (out/'census.json').write_text(json.dumps(census, sort_keys=True, indent=2)+'\n')
    receipt = out/'producer-proof-input.json'
    receipt.write_text(json.dumps({'run': {'child': child}})+'\n')
    command = [request['verifier'], request['corpus'], request['db'], str(receipt), request['row'], 'complete']
    command += ['reference', 'independent-reference', str(metadata)] if request['arm'] == 'baseline' else [request['profile'], request['pins']]
    # Stream to exclusively created files so a bounded parent timeout retains
    # native progress instead of losing a captured pipe when the group is killed.
    with (out/'namespace-proof.stdout').open('x') as stdout, (out/'namespace-proof.stderr').open('x') as stderr:
        result = subprocess.run(command, stdout=stdout, stderr=stderr, check=False, env=native_env)
    if result.returncode: raise ValueError('independent namespace/custody proof failed; see retained output')
    native = json.loads((out/'namespace-proof.stdout').read_text())
    if native.get('status') != 'CHECKED' or native.get('states') != child['states'] or native.get('custody_states') != child['states']:
        raise ValueError('independent namespace/custody proof incomplete')
    if request.get('proof_policy') == LITE_POLICY:
        if native.get('sample_policy') != LITE_POLICY or native.get('authenticated_bytes', 8*1024*1024+1) > 8*1024*1024 or native.get('acquired_content_bytes', 32*1024*1024+1) > 32*1024*1024:
            raise ValueError('native bounded content proof scope/byte limit mismatch')
    if request['arm'] == 'candidate' and native.get('independent_root_pins') != 'CHECKED':
        raise ValueError('candidate independent roots not checked')
    for owner_record in census['owners']:
        if digest(owner_record['path']) != owner_record['sha256_before'] or any(Path(owner_record['path']+s).exists() for s in ('-wal','-shm','-journal')):
            raise ValueError('namespace proof changed original owner')
    print(json.dumps(native, sort_keys=True))


if __name__ == '__main__':
    main()
