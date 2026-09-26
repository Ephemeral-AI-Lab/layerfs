#!/usr/bin/env python3
"""One v0.1.6 legacy SDK route diagnostic; never repeats a measured attempt."""
import datetime
import hashlib
import json
import platform
import secrets
import shutil
import subprocess
from pathlib import Path

repo = Path(__file__).resolve().parents[2]
probe = Path(__file__).resolve().parent
binary = probe / 'build/debug/v016-sdk-route-probe'
image = subprocess.check_output(['docker', 'image', 'inspect', 'layerfs-bench-infra:fdbd6273ee1ab63a', '--format', '{{.Id}}'], text=True).strip()
source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
status = subprocess.check_output(['git', 'status', '--porcelain'], cwd=repo, text=True).strip()
assert source == '44cf748486863ab7c21ca47e731bd88e2b9a7b4a' and not status
assert image.startswith('sha256:') and len(image) == 71
run = secrets.randbits(128) or 1
now = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
out = probe / 'evidence' / f'v016-route-{now}-{run & 0xffff:04x}'
out.mkdir(parents=True)
for name in ['Cargo.toml', 'Cargo.lock', 'src/main.rs']:
    item = probe / name
    shutil.copyfile(item, out / item.name.replace('/', '-'))
command = [str(binary)]
(out / 'command.txt').write_text('LAYERFS_V016_IMAGE=' + image + ' LAYERFS_V016_OUTPUT=' + str(out) + ' LAYERFS_V016_RUN=' + str(run) + ' ' + ' '.join(command) + '\n')
(out / 'identities.json').write_text(json.dumps({
    'source_tag': 'v0.1.6', 'source_commit': source,
    'source_tree': subprocess.check_output(['git', 'rev-parse', 'HEAD^{tree}'], cwd=repo, text=True).strip(),
    'source_status': status, 'image_id': image,
    'probe_binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    'probe_source_sha256': hashlib.sha256((probe/'src/main.rs').read_bytes()).hexdigest(),
    'telemetry_source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd='/Users/yifanxu/.codex/worktrees/bb50/layerfs', text=True).strip(),
    'telemetry_run': run, 'host': platform.platform(),
    'cache_contract': 'uncontrolled OS/source cache; diagnostic only',
    'sample_count': 1, 'route': 'legacy managed container create+start+connect -> FUSE session create -> Exec+drain -> Commit -> End(Clean)',
}, indent=2, sort_keys=True) + '\n')
env = dict(__import__('os').environ, LAYERFS_V016_IMAGE=image, LAYERFS_V016_OUTPUT=str(out), LAYERFS_V016_RUN=str(run))
with (out/'raw-host.log').open('wb') as stdout, (out/'stderr.log').open('wb') as stderr:
    process = subprocess.Popen(command, env=env, cwd=repo, stdout=stdout, stderr=stderr)
    try:
        code = process.wait(timeout=15)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()
        code = 124
        subprocess.run(['docker', 'rm', '-f', f'layerfs-v016-route-{process.pid}'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
(out/'exit-status.txt').write_text(str(code) + '\n')
manifest = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.iterdir()) if p.is_file()}
(out/'manifest.json').write_text(json.dumps(manifest, indent=2, sort_keys=True) + '\n')
print(json.dumps({'output': str(out), 'exit_code': code, 'image': image}))
raise SystemExit(code)
