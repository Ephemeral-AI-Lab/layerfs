#!/usr/bin/env python3
"""Run the public payload contract against owned Linux ext4 storage.

The normal input uses a named Docker volume; a separately created 48 MiB ext4
loop filesystem supplies real ENOSPC. A failed run retains its container/volume
for inspection and writes a non-passing receipt. No performance claim is made.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shlex
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[4]
CHECKS = ['unaligned-segment-ranges', 'reader-pins-and-admission',
          'empty-deadline-and-quota', 'failed-input-retention', 'native-short-write-retention',
          'corruption-and-explicit-cleanup', 'local-progress-and-window-headroom',
          'large-stream-and-residency',
          'clean-close', 'exact-block-quota-reuse', 'unowned-collider-retention',
          'stale-path-and-filesystem-refusal', 'native-enospc-retention']


def command(args, **kwargs):
    return subprocess.run(args, check=True, text=True, capture_output=True, **kwargs)


def product_inputs():
    paths = [ROOT / '.cargo/config.toml', ROOT / 'core/Cargo.toml', ROOT / 'core/Cargo.lock']
    for package in (ROOT / 'core/crates').iterdir():
        paths.append(package / 'Cargo.toml')
        for group in ('src', 'sql'):
            paths.extend(path for path in (package / group).rglob('*') if path.is_file())
    digest = hashlib.sha256()
    for path in sorted(paths):
        digest.update(str(path.relative_to(ROOT)).encode() + b'\0')
        digest.update(hashlib.sha256(path.read_bytes()).digest())
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--test-binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--image', default='rust:1.85.1-bookworm')
    args = parser.parse_args()
    args.output = args.output.resolve(); args.output.mkdir(parents=True, exist_ok=False)
    binary = args.test_binary.resolve().relative_to(ROOT)
    name = 'layerfs-payload-' + uuid.uuid4().hex[:16]
    volume = name + '-data'
    report = {'status': 'FAIL', 'mode': 'functional-owned-payload',
              'checks': [{'id': item, 'status': 'NOT_RUN'} for item in CHECKS],
              'performance_claim': False, 'cache_claim': 'only observed private-inode residency',
              'not_run': ['mounted writes', 'metadata index', 'snapshot G/live successor',
                          'Commit/lowering', 'npm', 'R6', 'RSS/cgroup hard bound']}
    report.update(source=command(['git', 'rev-parse', 'HEAD'], cwd=ROOT).stdout.strip(),
                  product_inputs_sha256=product_inputs(),
                  test_binary_sha256=hashlib.sha256(args.test_binary.read_bytes()).hexdigest(),
                  image=command(['docker', 'image', 'inspect', args.image, '--format', '{{.Id}}']).stdout.strip(),
                  driver_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  oracle_sha256=hashlib.sha256(Path(__file__).with_name('payload_residency.py').read_bytes()).hexdigest(),
                  test_source_sha256=hashlib.sha256(Path(__file__).with_name('payload.rs').read_bytes()).hexdigest())
    started = time.monotonic(); created = False; made_volume = False
    def remaining():
        return max(0.01, 60 - (time.monotonic() - started))
    try:
        command(['docker', 'volume', 'create', volume], timeout=remaining()); made_volume = True
        command(['docker', 'run', '-d', '--privileged', '--name', name,
                 '--mount', f'type=bind,src={ROOT},dst=/work,readonly',
                 '--mount', f'type=volume,src={volume},dst=/payload',
                 args.image, 'sleep', 'infinity'], timeout=remaining()); created = True
        report['kernel'] = command(['docker', 'exec', name, 'uname', '-srmo'], timeout=remaining()).stdout.strip()
        setup = '''set -eu
chmod 700 /payload
truncate -s 48M /tmp/layerfs-enospc.img
mkfs.ext4 -q -F -b 4096 /tmp/layerfs-enospc.img
mkdir -m 700 /enospc
mount -o loop /tmp/layerfs-enospc.img /enospc
chmod 700 /enospc
findmnt -n -o FSTYPE,TARGET -T /payload
findmnt -n -o FSTYPE,TARGET -T /enospc
stat -f -c '%s %a' /enospc
'''
        report['setup'] = command(['docker', 'exec', name, 'sh', '-c', setup], timeout=remaining()).stdout
        assert report['setup'].splitlines()[0].split()[0] == 'ext4', report['setup']
        invocation = ['docker', 'exec', '-e', 'LAYERFS_PAYLOAD_TEST_ROOT=/payload',
                      '-e', 'LAYERFS_PAYLOAD_ENOSPC_ROOT=/enospc',
                      '-e', 'LAYERFS_PAYLOAD_RESIDENCY_ORACLE=/work/core/crates/layerfs-workspace/tests/payload_residency.py',
                      '-e', 'LAYERFS_CONSTRUCTION_WORKERS=1', name,
                      '/work/' + str(binary), '--ignored', '--nocapture', '--test-threads=1']
        report['invocation'] = shlex.join(invocation)
        run = subprocess.run(invocation, text=True, capture_output=True, timeout=remaining())
        (args.output / 'test.stdout').write_text(run.stdout)
        (args.output / 'test.stderr').write_text(run.stderr)
        for row in report['checks']:
            if f"PAYLOAD_CHECK {row['id']} PASS" in run.stdout:
                row['status'] = 'PASS'
        report['test_exit'] = run.returncode
        assert run.returncode == 0 and all(row['status'] == 'PASS' for row in report['checks']), run.stderr[-2000:]
        report['observations'] = [line for line in run.stdout.splitlines()
                                  if line.startswith(('PAYLOAD_RESOURCE ', 'PAYLOAD_RESIDENCY ', 'PAYLOAD_ENOSPC ', 'PAYLOAD_SHORT_WRITE '))]
        command(['docker', 'exec', name, 'umount', '/enospc'], timeout=remaining())
        command(['docker', 'rm', '-f', name], timeout=remaining()); created = False
        command(['docker', 'volume', 'rm', volume], timeout=remaining()); made_volume = False
        report['status'] = 'PASS'; report['cleanup'] = 'PASS'
    except BaseException as error:
        report['failure'] = repr(error)
        report['cleanup'] = 'RETAINED_FOR_INSPECTION'
        if created:
            report['retained_container'] = name
        if made_volume:
            report['retained_volume'] = volume
        raise
    finally:
        report['command_wall_seconds'] = time.monotonic() - started
        report['hard_budget_seconds'] = 60
        (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
