#!/usr/bin/env python3
"""Owned local Phase 7 PostgreSQL/MinIO development services; never a benchmark."""

import argparse
from datetime import datetime, timezone
import hashlib
import hmac
import json
from pathlib import Path
import secrets
import shlex
import subprocess
import time
import urllib.error
import urllib.request
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[2]
STATE = ROOT / 'target' / 'phase7-services' / 'settings.json'
POSTGRES_IMAGE = 'postgres@sha256:639ab7ceb90e13123085b741fb31ef493fba25463002f6da665352e7b534b652'
MINIO_IMAGE = 'cgr.dev/chainguard/minio@sha256:4692462f35d97d7e82c30371d82f057703c5d9489bcae726010594c812f2d285'
OWNER_LABEL = 'dev.layerfs.phase7.worktree'
NAMES = {'postgres': 'layerfs-postgres', 'minio': 'layerfs-minio'}
NETWORK = 'layerfs-phase7-cluster1'
VOLUMES = {'postgres': 'layerfs-phase7-postgres', 'minio': 'layerfs-phase7-minio'}


def docker(*args, input_text=None, check=True):
    result = subprocess.run(['docker', *args], input=input_text, text=True,
                            capture_output=True, timeout=60)
    if check and result.returncode:
        raise RuntimeError(f'docker {args[0]} failed: {result.stderr.strip()}')
    return result


def inspect(kind, name):
    result = docker(kind, 'inspect', name, check=False) if kind in ('volume', 'network') else docker('inspect', name, check=False)
    if result.returncode:
        missing = ('No such object:', 'No such container:', 'no such volume', 'No such network:')
        if result.returncode == 1 and (any(marker.casefold() in result.stderr.casefold() for marker in missing) or (kind == 'network' and f'network {name} not found' in result.stderr.casefold())):
            return None
        raise RuntimeError(f'cannot inspect {kind} {name}: {result.stderr.strip()}')
    return json.loads(result.stdout)[0]


def owned(kind, name):
    item = inspect(kind, name)
    if item is not None:
        labels = item.get('Config', {}).get('Labels', {}) if kind == 'container' else item.get('Labels', {})
        if (labels or {}).get(OWNER_LABEL) != str(ROOT):
            raise RuntimeError(f'refusing foreign {kind}: {name}')
    return item


def ownership_inventory():
    """Validate all resources before removing or creating any of them."""
    return {kind: {name: owned(kind, name) for name in names}
            for kind, names in [('container', NAMES.values()), ('volume', VOLUMES.values()), ('network', [NETWORK])]}


def save(settings):
    STATE.parent.mkdir(parents=True, exist_ok=True)
    STATE.write_text(json.dumps(settings, indent=2) + '\n')
    STATE.chmod(0o600)
    exports = STATE.with_name('services.env')
    exports.write_text(''.join(f'export {key}={shlex.quote(value)}\n' for key, value in settings['environment'].items()))
    exports.chmod(0o600)


def load():
    if not STATE.exists():
        raise RuntimeError('no owned settings; run up first')
    settings = json.loads(STATE.read_text())
    if settings['owner'] != str(ROOT):
        raise RuntimeError('settings belong to another worktree')
    return settings


def fresh_settings():
    return {'owner': str(ROOT), 'complete': False, 'epoch': secrets.token_hex(12), 'environment': {
        'LAYERFS_PG_HOST': '127.0.0.1', 'LAYERFS_PG_PORT': '5432',
        'LAYERFS_PG_DATABASE': 'layerfs', 'LAYERFS_PG_SCHEMA': 'layerfs',
        'LAYERFS_PG_USER': 'layerfs', 'LAYERFS_PG_PASSWORD': secrets.token_hex(24),
        'LAYERFS_PG_TLS': 'disabled',
        'LAYERFS_S3_ENDPOINT': 'http://127.0.0.1:9000', 'LAYERFS_S3_BUCKET': 'layerfs',
        'LAYERFS_S3_PREFIX': 'cluster1', 'LAYERFS_S3_REGION': 'us-east-1',
        'LAYERFS_S3_ACCESS_KEY': 'layerfs' + secrets.token_hex(8),
        'LAYERFS_S3_SECRET_KEY': secrets.token_hex(24),
    }, 'images': {'postgres': POSTGRES_IMAGE, 'minio': MINIO_IMAGE}}


def signed_bucket_request(environment, now=None):
    """Only the development bucket bootstrap; product requests live in layerfs-s3."""
    now = now or datetime.now(timezone.utc)
    date = now.strftime('%Y%m%d'); stamp = now.strftime('%Y%m%dT%H%M%SZ')
    endpoint = environment['LAYERFS_S3_ENDPOINT']; host = urlparse(endpoint).netloc
    path = '/' + environment['LAYERFS_S3_BUCKET']; body_hash = hashlib.sha256(b'').hexdigest()
    headers = f'host:{host}\nx-amz-content-sha256:{body_hash}\nx-amz-date:{stamp}\n'
    names = 'host;x-amz-content-sha256;x-amz-date'
    canonical = f'PUT\n{path}\n\n{headers}\n{names}\n{body_hash}'
    region = environment['LAYERFS_S3_REGION']; scope = f'{date}/{region}/s3/aws4_request'
    to_sign = f'AWS4-HMAC-SHA256\n{stamp}\n{scope}\n{hashlib.sha256(canonical.encode()).hexdigest()}'
    key = ('AWS4' + environment['LAYERFS_S3_SECRET_KEY']).encode()
    for value in (date, region, 's3', 'aws4_request'):
        key = hmac.new(key, value.encode(), hashlib.sha256).digest()
    signature = hmac.new(key, to_sign.encode(), hashlib.sha256).hexdigest()
    authorization = f'AWS4-HMAC-SHA256 Credential={environment["LAYERFS_S3_ACCESS_KEY"]}/{scope}, SignedHeaders={names}, Signature={signature}'
    return urllib.request.Request(endpoint + path, data=b'', method='PUT', headers={
        'Host': host, 'x-amz-content-sha256': body_hash, 'x-amz-date': stamp, 'Authorization': authorization})


def wait_ready(settings):
    """Readiness polling is setup, not a product operation or timed sample."""
    deadline = time.monotonic() + 30
    pg = minio = False
    while time.monotonic() < deadline:
        if not pg:
            pg = docker('exec', NAMES['postgres'], 'pg_isready', '-h', '127.0.0.1', '-U', 'layerfs', '-d', 'layerfs', check=False).returncode == 0
        if not minio:
            try:
                with urllib.request.urlopen('http://127.0.0.1:9000/minio/health/live', timeout=1) as response:
                    minio = response.status == 200
            except (urllib.error.URLError, TimeoutError, ConnectionError):
                pass
        if pg and minio:
            return
        for name in NAMES.values():
            item = owned('container', name)
            if item is None or not item['State']['Running']:
                raise RuntimeError(f'service exited during readiness: {name}')
        time.sleep(0.1)
    raise RuntimeError('service readiness failed within declared setup budget')


def postgres_sql(sql):
    return docker('exec', '-i', NAMES['postgres'], 'psql', '-X', '-v', 'ON_ERROR_STOP=1',
                  '-U', 'layerfs', '-d', 'layerfs', '-At', input_text=sql).stdout


def bootstrap(settings):
    postgres_sql('CREATE SCHEMA layerfs;\nCHECKPOINT;\n')
    with urllib.request.urlopen(signed_bucket_request(settings['environment']), timeout=5) as response:
        if response.status != 200:
            raise RuntimeError('bucket create was not acknowledged')
    # Runtime tables are frozen in steps 6/7 and bootstrapped explicitly there.
    settings['schema_bootstrap'] = 'namespace-only; C2/C5 runtime tables enter in steps 6/7'


def up():
    inventory = ownership_inventory()
    existing = [item for item in inventory['container'].values() if item is not None]
    if existing:
        settings = load()
        if len(existing) != 2 or not settings['complete']:
            raise RuntimeError('partial owned setup; inspect evidence and use explicit reset')
        validate(settings)
        return status(settings, dependency_reuse='owned-services-and-volumes')
    if any(inventory['volume'].values()) or any(inventory['network'].values()):
        raise RuntimeError('orphaned owned setup; explicit reset required')
    settings = fresh_settings(); save(settings)
    for key, image in settings['images'].items():
        image_info = json.loads(docker('image', 'inspect', image).stdout)[0]
        settings.setdefault('image_ids', {})[key] = image_info['Id']
    save(settings)
    label = f'{OWNER_LABEL}={ROOT}'
    docker('network', 'create', '--label', label, NETWORK)
    for volume in VOLUMES.values():
        docker('volume', 'create', '--label', label, volume)
    e = settings['environment']
    common = ['--detach', '--label', label, '--network', NETWORK, '--cpus', '2',
              '--memory', '512m', '--memory-swap', '512m', '--pids-limit', '256']
    # Pass secrets in private environment files, never in printed argv/evidence.
    pg_env = STATE.with_name('postgres.env'); minio_env = STATE.with_name('minio.env')
    pg_env.write_text(f'POSTGRES_USER=layerfs\nPOSTGRES_DB=layerfs\nPOSTGRES_PASSWORD={e["LAYERFS_PG_PASSWORD"]}\nTZ=UTC\n')
    minio_env.write_text(f'MINIO_ROOT_USER={e["LAYERFS_S3_ACCESS_KEY"]}\nMINIO_ROOT_PASSWORD={e["LAYERFS_S3_SECRET_KEY"]}\nMINIO_COMPRESS_ENABLE=off\nMINIO_KMS_AUTO_ENCRYPTION=off\nMINIO_BROWSER=off\n')
    pg_env.chmod(0o600); minio_env.chmod(0o600)
    docker('run', *common, '--name', NAMES['postgres'], '--publish', '127.0.0.1:5432:5432',
           '--env-file', str(pg_env), '--volume', VOLUMES['postgres'] + ':/var/lib/postgresql/data', POSTGRES_IMAGE)
    # The minimal image's named data volume is root-owned; this profile states UID 0.
    docker('run', *common, '--name', NAMES['minio'], '--user', '0:0', '--publish', '127.0.0.1:9000:9000',
           '--env-file', str(minio_env), '--volume', VOLUMES['minio'] + ':/data', MINIO_IMAGE, 'server', '/data')
    wait_ready(settings); bootstrap(settings)
    settings['complete'] = True; save(settings)
    validate(settings)
    return status(settings, dependency_reuse='pinned-local-images')


def validate(settings):
    for service, name in NAMES.items():
        item = owned('container', name)
        if item is None or not item['State']['Running'] or item['Image'] != settings['image_ids'][service]:
            raise RuntimeError(f'wrong image or unavailable owned service: {name}')
        port = '5432/tcp' if service == 'postgres' else '9000/tcp'
        binding = item['NetworkSettings']['Ports'].get(port)
        if binding != [{'HostIp': '127.0.0.1', 'HostPort': port.split('/')[0]}]:
            raise RuntimeError(f'wrong published endpoint: {name}')
        limits = item['HostConfig']
        if limits['Memory'] != 512 * 1024 * 1024 or limits['MemorySwap'] != limits['Memory'] or limits['NanoCpus'] != 2_000_000_000:
            raise RuntimeError(f'wrong declared resource profile: {name}')


def status(settings=None, dependency_reuse=None):
    settings = settings or load(); validate(settings)
    pg_settings = postgres_sql("SELECT name || '=' || setting FROM pg_settings WHERE name IN ('server_version','server_encoding','fsync','synchronous_commit','full_page_writes','wal_level','shared_buffers','default_transaction_isolation','statement_timeout','TimeZone') ORDER BY name;\n")
    return {'status': 'PASS', 'owner': str(ROOT), 'epoch': settings['epoch'],
            'images': settings['images'], 'image_ids': settings['image_ids'],
            'postgres_settings': pg_settings.splitlines(),
            'postgres_settings_sha256': hashlib.sha256(pg_settings.encode()).hexdigest(),
            'minio_version': docker('exec', NAMES['minio'], '/usr/bin/minio', '--version').stdout.splitlines()[0],
            'schema_bootstrap': settings.get('schema_bootstrap'), 'dependency_reuse': dependency_reuse,
            'environment_file': str(STATE.with_name('services.env')),
            'resource_profile': {'cpus_per_service': 2, 'memory_bytes_per_service': 512 * 1024 * 1024, 'swap': 'disabled', 'minio_uid': 0}}


def down():
    inventory = ownership_inventory()
    for name, item in inventory['container'].items():
        if item is not None:
            docker('rm', '--force', name)
    for name, item in inventory['volume'].items():
        if item is not None:
            docker('volume', 'rm', name)
    for name, item in inventory['network'].items():
        if item is not None:
            docker('network', 'rm', name)
    if STATE.exists():
        settings = load(); settings['complete'] = False; save(settings)
    return {'status': 'PASS', 'cleanup': 'owned-containers-volumes-network-removed'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['up', 'reset', 'down', 'status'])
    args = parser.parse_args()
    try:
        if args.command == 'reset':
            down(); result = up()
        else:
            result = {'up': up, 'down': down, 'status': status}[args.command]()
        print(json.dumps(result, indent=2))
        return 0
    except (RuntimeError, subprocess.TimeoutExpired, urllib.error.URLError, OSError) as error:
        print(json.dumps({'status': 'FAIL', 'reason': str(error)}))
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
