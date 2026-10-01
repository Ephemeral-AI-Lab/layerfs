"""Acquire the exact native community MinIO bottle once; no global installation."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import urllib.request

from common import write_json

SHA = 'f399aa93691336acf3ef6f79c08f97f2d43784ecf14d1f535c77bd3813082ee1'
VERSION = 'RELEASE.2025-10-15T17-29-55Z'
URL = 'https://ghcr.io/v2/homebrew/core/minio/blobs/sha256:' + SHA


def acquire(root):
    root = Path(root).resolve()
    root.mkdir(parents=True, exist_ok=True)
    archive = root / 'minio-bottle.tar.gz'
    if not archive.exists():
        token_url = 'https://ghcr.io/token?service=ghcr.io&scope=repository:homebrew/core/minio:pull'
        with urllib.request.urlopen(token_url, timeout=30) as response:
            token = json.load(response)['token']
        request = urllib.request.Request(URL, headers={'Authorization': 'Bearer ' + token})
        # Keep incomplete acquisitions visible. An existing file is never overwritten.
        with urllib.request.urlopen(request, timeout=30) as response, archive.open('xb') as out:
            shutil.copyfileobj(response, out, length=65536)
    with archive.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    if digest != SHA:
        raise RuntimeError('bottle hash mismatch/incomplete; retained file, no automatic retry')
    binary = root / 'minio'
    if not binary.exists():
        with tarfile.open(archive) as bottle:
            members = [m for m in bottle.getmembers() if m.isfile() and m.name.endswith('/bin/minio')]
            if len(members) != 1:
                raise RuntimeError('expected exactly one MinIO binary')
            with bottle.extractfile(members[0]) as source, binary.open('xb') as out:
                shutil.copyfileobj(source, out, length=65536)
        binary.chmod(0o755)
    version = subprocess.check_output([str(binary), '--version'], text=True).strip()
    if VERSION not in version:
        raise RuntimeError('provider version differs from frozen selection')
    with binary.open('rb') as source:
        binary_sha = hashlib.file_digest(source, 'sha256').hexdigest()
    seal = {'version': version, 'binary_sha256': binary_sha, 'bottle_sha256': SHA,
            'source': URL, 'distribution': 'Homebrew arm64_tahoe community bottle'}
    target = root / 'provider.json'
    if target.exists() and json.loads(target.read_text()) != seal:
        raise RuntimeError('existing provider seal differs; no replacement')
    if not target.exists():
        write_json(target, seal)
    print('checksum-verified native community MinIO ready')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', default='benchmark-results/storage-probes/provider')
    acquire(parser.parse_args().root)
