#!/usr/bin/env python3
"""Phased real Sandbox image proof; never runs builds or an active preview.

Prepare: --work ABS --prepare --cli PIN --native PIN --bundle ABS [--os-image ABS]
Run each once with --work ABS --phase lima-machine (or docker-machine), then:
  PROVIDER-project, PROVIDER-import, PROVIDER-exec for each selected provider;
  source-delete; PROVIDER-reuse, PROVIDER-reuse-exec;
  PROVIDER-cleanup-sandboxes, PROVIDER-cleanup-store, PROVIDER-cleanup-machine;
  docker-cleanup-resources additionally retires only its stopped container/volume.
Each child phase has a hard 110s deadline. A timeout is UNKNOWN, never retried.
Only these exact resources in the generated private home are touched. Evidence
is append-only; the home, immutable plan, source and receipts remain at handoff.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import time
import uuid


def append(work, value):
    with (work / 'receipts.ndjson').open('a') as stream:
        stream.write(json.dumps({'at': time.time(), **value}) + '\n')


def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for part in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(part)
    return digest.hexdigest()


def prepare(args, work):
    work.mkdir(parents=True, exist_ok=False)
    append(work, {'phase': 'prepare', 'status': 'STARTED'})
    bundle = Path(args.bundle).resolve(strict=True)
    archive = bundle / 'sandbox-image.tar'
    output = work / 'custom-image.tar'
    identity = uuid.uuid4().hex[:16]
    with tarfile.open(archive, 'r:') as source:
        member = source.getmember('manifest.json')
        if member.size > 1024 * 1024:
            raise ValueError('manifest exceeds bounded metadata size')
        manifest = json.load(source.extractfile(member))
        if len(manifest) != 1:
            raise ValueError('fixture preparation requires one bundled image')
        config_name = manifest[0]['Config']
        member = source.getmember(config_name)
        if member.size > 1024 * 1024:
            raise ValueError('config exceeds bounded metadata size')
        config = json.load(source.extractfile(member))
        config.setdefault('config', {}).setdefault('Labels', {})['org.layerfs.image-proof'] = identity
        layer_io = io.BytesIO()
        with tarfile.open(fileobj=layer_io, mode='w', format=tarfile.USTAR_FORMAT) as layer:
            body = identity.encode() + b'\n'
            marker = tarfile.TarInfo('.layerfs-image-proof')
            marker.mode = 0o644
            marker.size = len(body)
            layer.addfile(marker, io.BytesIO(body))
        proof_layer = layer_io.getvalue()
        manifest[0]['Layers'] = manifest[0]['Layers'][:2]
        config['rootfs']['diff_ids'] = config['rootfs']['diff_ids'][:2]
        config['history'] = [entry for entry in config.get('history', []) if not entry.get('empty_layer')][:2]
        config['rootfs']['diff_ids'].append('sha256:' + hashlib.sha256(proof_layer).hexdigest())
        config.setdefault('history', []).append({'created_by': 'LayerFS custom-image live proof'})
        manifest[0]['Layers'].append('proof-layer.tar')
        encoded = json.dumps(config, separators=(',', ':'), sort_keys=True).encode()
        digest = hashlib.sha256(encoded).hexdigest()
        manifest[0]['Config'] = digest + '.json'
        manifest[0]['RepoTags'] = ['layerfs-proof:' + identity]
        with tarfile.open(output, 'w', format=tarfile.USTAR_FORMAT) as target:
            for member in source:
                if member.name not in manifest[0]['Layers'][:-1]:
                    continue
                target.addfile(member, source.extractfile(member) if member.isfile() else None)
            for name, body in [('manifest.json', json.dumps(manifest).encode()), (digest + '.json', encoded), ('proof-layer.tar', proof_layer)]:
                member = tarfile.TarInfo(name)
                member.mode = 0o644
                member.size = len(body)
                target.addfile(member, io.BytesIO(body))
    source_dir = work / 'source'
    source_dir.mkdir()
    (source_dir / 'proof.txt').write_text('custom-image-proof\n')
    cli = Path(args.cli).resolve(strict=True)
    native = Path(args.native).resolve(strict=True)
    root = Path(__file__).resolve().parent.parent
    plan = {'version': 1, 'identity': identity, 'home': str(work / 'home'),
            'fixture_treatment': 'first-two-original-layers-plus-sentinel', 'archive': str(output), 'digest': 'sha256:' + digest, 'source': str(source_dir),
            'cli': str(cli), 'cli_sha256': sha(cli), 'native': str(native), 'native_sha256': sha(native),
            'bundle': str(bundle), 'bundle_manifest': json.loads((bundle / 'bundle.json').read_text()),
            'sdk': str(root / 'api/node/src/index.js'), 'osImage': args.os_image,
            'runner_sha256': sha(Path(__file__).resolve()), 'worker_script_sha256': sha(Path(__file__).with_suffix('.mjs')),
            'source_commit': subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip(),
            'providers': {'lima': {'machine': 'proof-' + identity}, 'docker': {'machine': 'docker'}}}
    with (work / 'plan.json').open('x') as stream:
        json.dump(plan, stream, indent=2)
    append(work, {'phase': 'prepare', 'status': 'PASS', 'digest': plan['digest'], 'archive_bytes': output.stat().st_size})
    print(json.dumps(plan, indent=2))


def run(args, work):
    plan = json.loads((work / 'plan.json').read_text())
    phase = args.phase
    previous = [json.loads(line) for line in (work / 'receipts.ndjson').read_text().splitlines()]
    if any(row.get('phase') == phase and row.get('status') == 'STARTED' for row in previous):
        raise ValueError('phase already attempted; inspect receipts, do not replay')
    append(work, {'phase': phase, 'status': 'STARTED'})
    if phase == 'source-delete':
        Path(plan['archive']).unlink()
        append(work, {'phase': phase, 'status': 'PASS'})
        return
    if phase == 'docker-cleanup-resources':
        if not any(row.get('phase') == 'docker-cleanup-machine' and row.get('status') == 'PASS' for row in previous):
            raise ValueError('stop the owned Docker worker successfully before releasing resources')
        home = Path(plan['home'])
        home_id = json.loads((home / 'layerfs.json').read_text())['id']
        record = json.loads((home / 'machine/docker/record.json').read_text())
        name = record.get('vm_name', 'lfs-' + home_id[:8] + '-docker')
        expected = 'lfs-' + home_id[:8] + '-docker'
        if name != expected:
            raise ValueError('worker name differs from exact private-home identity')
        docker = ['docker', '-H', 'unix://' + record.get('docker', '/var/run/docker.sock')]
        with (work / (phase + '.log')).open('xb') as log:
            inspected = subprocess.run(docker + ['inspect', name], capture_output=True, timeout=20, check=True)
            value = json.loads(inspected.stdout)[0]
            if value['State']['Running'] or value['Config']['Labels'].get('layerfs.role') != 'machine-worker':
                raise ValueError('refusing to release a running or unrelated worker')
            volume = expected + '-data'
            if not any(m.get('Name') == volume and m.get('Destination') == '/layerfs/data' for m in value['Mounts']):
                raise ValueError('worker data volume does not match its exact private-home identity')
            subprocess.run(docker + ['rm', name], stdout=log, stderr=subprocess.STDOUT, timeout=20, check=True)
            subprocess.run(docker + ['volume', 'rm', volume], stdout=log, stderr=subprocess.STDOUT, timeout=20, check=True)
        append(work, {'phase': phase, 'status': 'PASS', 'released': [name, volume]})
        return
    for key in ('cli', 'native'):
        if sha(Path(plan[key])) != plan[key + '_sha256']:
            raise ValueError('pinned ' + key + ' changed after prepare')
    env = {**os.environ, 'LAYERFS_HOME': plan['home'], 'LAYERFS_NATIVE': plan['native'],
           'LAYERFS_BUNDLE': plan['bundle']}
    script = Path(__file__).with_suffix('.mjs')
    with (work / (phase + '.log')).open('xb') as log:
        try:
            result = subprocess.run([args.node, str(script), str(work), phase], env=env,
                                    stdout=log, stderr=subprocess.STDOUT, timeout=110, check=False)
        except subprocess.TimeoutExpired:
            append(work, {'phase': phase, 'status': 'UNKNOWN', 'reason': '110s deadline; operation may have run; no replay'})
            raise
    append(work, {'phase': phase, 'status': 'PASS' if result.returncode == 0 else 'FAIL', 'exit_code': result.returncode})
    if result.returncode:
        raise SystemExit(result.returncode)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--prepare', action='store_true')
    parser.add_argument('--phase')
    parser.add_argument('--cli')
    parser.add_argument('--native')
    parser.add_argument('--bundle')
    parser.add_argument('--os-image')
    parser.add_argument('--node', default='node')
    args = parser.parse_args()
    work = args.work.absolute()
    if args.prepare:
        if not all((args.cli, args.native, args.bundle)):
            parser.error('--prepare requires --cli, --native, --bundle')
        prepare(args, work)
    elif args.phase:
        run(args, work)
    else:
        parser.error('choose --prepare or --phase')
