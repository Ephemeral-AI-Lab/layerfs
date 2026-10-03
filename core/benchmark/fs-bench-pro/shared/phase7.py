"""Phase 7 preconditions and strict joint gate; observations never warm input."""
import importlib.util
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[4]
BACKEND = ROOT / 'core/benchmark/fs-bench-pro-storage-content/shared'
sys.path.insert(0, str(BACKEND))
import residency

spec = importlib.util.spec_from_file_location('phase7_services', ROOT / 'core/tools/phase7_services.py')
services = importlib.util.module_from_spec(spec)
spec.loader.exec_module(services)
CACHE = 'phase7-fresh-services-v1'
C2 = {'store_policy', 'pack', 'object', 'metadata_value_group', 'content_signature'}
C5 = {'history_meta', 'layer_stack', 'layer', 'commit', 'branch', 'workspace_stage', 'scope_allocator'}


def dewarm_tree(root):
    """Invalidate all regular input files, then attest the whole input again."""
    root = Path(root)
    if root.is_symlink() or not root.is_dir():
        raise ValueError('input must be an ordinary directory')
    paths = []
    for path in sorted(root.rglob('*')):
        if path.is_symlink():
            raise ValueError('symlink in measured input')
        if path.is_file():
            paths.append(path)
    if not paths:
        raise ValueError('empty input inventory')
    first = after = pages = length = invalidations = 0
    for path in paths:
        report = residency.de_warm(path)
        first += report.resident_first
        invalidations += report.invalidated
    # Separate whole-input pass catches residency introduced during invalidation.
    for path in paths:
        report = residency.residency(path)
        after += report.resident_pages
        pages += report.total_pages
        length += report.length_bytes
    return {'status': 'PASS' if after == 0 else 'INELIGIBLE', 'files': len(paths),
            'length_bytes': length, 'resident_first': first, 'resident_after': after,
            'total_pages': pages, 'invalidated_files': invalidations,
            'method': 'msync-invalidate-then-whole-input-mincore; no payload reads'}


def strict_gate(candidate, baseline, ceiling):
    """Both axes, cache, proof and budgets must pass; equality never passes."""
    for row in (candidate, baseline):
        if row.get('cache_status') != 'PASS':
            return 'INELIGIBLE'
        if row.get('status') != 'COMPLETE' or row.get('verification_status') != 'PASS':
            return 'INCOMPLETE'
        if row.get('command_wall_ns', float('inf')) > row['command_budget_ns']:
            return 'FAIL'
        if row.get('verification_wall_ns', float('inf')) > row['verification_budget_ns']:
            return 'FAIL'
        if type(row.get('comparison_ns')) is not int or row['comparison_ns'] <= 0:
            return 'INCOMPLETE'
    if candidate['comparison_ns'] >= baseline['comparison_ns']:
        return 'FAIL'
    total = candidate.get('storage_bytes')
    if type(total) is not int or total < 0:
        return 'INCOMPLETE'
    if ceiling is None:
        # Init speed can pass, but its undeclared numeric storage gate remains open.
        return 'INCOMPLETE'
    return 'PASS' if total < ceiling else 'FAIL'


def empty_services():
    tables = services.postgres_sql("SELECT count(*) FROM pg_tables WHERE schemaname='layerfs';")
    if tables.strip() != '0':
        raise ValueError('fresh service schema is populated')
    result = services.docker('exec', services.NAMES['minio'], 'du', '-s', '-B1', '/data/layerfs')
    # Bucket creation alone is allowed; no pack path may exist before the operation.
    exists = services.docker('exec', services.NAMES['minio'], 'sh', '-c',
                            'test ! -e /data/layerfs/cluster1', check=False)
    if exists.returncode != 0:
        raise ValueError('fresh bucket contains the measured prefix')
    return {'status': 'PASS', 'tables': 0, 'bucket_setup_allocation': int(result.stdout.split()[0])}


def collect():
    """Untimed checkpoint; allocated data+metadata, relations incl. TOAST/indexes."""
    settings = services.load()
    services.validate(settings)
    services.postgres_sql('CHECKPOINT;')
    raw = services.postgres_sql("""
SELECT json_build_object('relations', (SELECT json_agg(json_build_object(
 'name',c.relname,'kind',c.relkind,'bytes',pg_total_relation_size(c.oid)))
 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
 WHERE n.nspname='layerfs' AND c.relkind IN ('r','S')),
 'database_bytes', pg_database_size(current_database()),
 'wal_bytes', (SELECT COALESCE(sum(size),0) FROM pg_ls_waldir()),
 'payload_objects', (SELECT count(*) FROM layerfs.pack WHERE domain=1),
 'payload_lengths', (SELECT COALESCE(sum(length),0) FROM layerfs.pack WHERE domain=1));
""")
    pg = json.loads(raw)
    relations = pg['relations'] or []
    names = {item['name'] for item in relations}
    if names != C2 | C5 | {'pack_id'}:
        raise ValueError('unaccounted or missing LayerFS relations')
    pg['C2_bytes'] = sum(item['bytes'] for item in relations if item['name'] in C2 | {'pack_id'})
    pg['C5_bytes'] = sum(item['bytes'] for item in relations if item['name'] in C5)
    script = '''walk() {
 for p in "$1"/* "$1"/.[!.]*; do
  [ -e "$p" ] || continue
  [ ! -L "$p" ] || exit 3
  if [ -d "$p" ]; then walk "$p"; elif [ -f "$p" ]; then stat -c '%b|%s|%n' "$p"; else exit 4; fi
 done
}
walk /data/layerfs'''
    raw = services.docker('exec', services.NAMES['minio'], 'sh', '-c', script).stdout
    files = []
    for line in raw.splitlines():
        blocks, length, path = line.split('|', 2)
        files.append({'path': path, 'allocated_bytes': int(blocks)*512, 'length_bytes': int(length)})
    minio = {'files': files, 'allocated_bytes': sum(item['allocated_bytes'] for item in files),
             'objects': pg['payload_objects'], 'object_lengths': pg['payload_lengths'],
             'system_directory_bytes': int(services.docker('exec', services.NAMES['minio'],
                 'du', '-s', '-B1', '/data/.minio.sys').stdout.split()[0]),
             'system_directory_excluded': True}
    return {'status': 'PASS', 'epoch': settings['epoch'], 'postgres': pg, 'minio': minio,
            'total_bytes': pg['C2_bytes'] + pg['C5_bytes'] + minio['allocated_bytes'],
            'method': 'every bucket regular file st_blocks*512 + LayerFS heap/index/TOAST/sequence allocation'}
