#!/usr/bin/env python3
"""Append-only public SDK fixed-16-MiB read Budget functional proof.

Builds only a static external POSIX workload; immutable daemon/product limits,
worker and SDK call boundaries remain unchanged. No numeric admission claim.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[5]
SOURCE = Path(__file__).resolve()
WORKLOAD = SOURCE.with_name('budget_writer.c')
TEST = SOURCE.with_name('workspace_view.rs')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--image', required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    assert str(output).startswith(str((ROOT / 'core/target').resolve()) + os.sep)
    assert subprocess.check_output(['docker', 'image', 'inspect', args.image,
                                    '--format', '{{.Id}}'], text=True).strip() == args.image
    dirty = subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).splitlines()
    mode = 'diagnostic' if '-diagnostic-' in output.name else 'functional'
    if mode == 'functional':
        assert not dirty, 'functional SDK proof requires a clean source seal'
    compiler = shutil.which('zig')
    assert compiler, 'build static external POSIX helper with zig cc'
    helper = output / 'budget-writer'
    build = [compiler, 'cc', '-target', 'aarch64-linux-musl', '-static', '-O2',
             '-o', str(helper), str(WORKLOAD)]
    env = os.environ.copy()
    env.update(LAYERFS_BUDGET_WRITER=str(helper), LAYERFS_TEST_IMAGE=args.image,
               LAYERFS_CONSTRUCTION_WORKERS='1')
    command = ['cargo', '+1.85.1', 'test', '--manifest-path', 'core/Cargo.toml', '--locked',
               '-p', 'layerfs-sdk', '--test', 'workspace_view', '--', '--exact',
               'view_lease_read_refuses_exhausted_response_budget_without_partial_entry',
               '--nocapture', '--test-threads=1']
    result = {'status': 'FAIL', 'mode': mode, 'dirty': dirty,
              'source': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT,
                                                text=True).strip(),
              'image_id': args.image, 'driver_sha256': sha(SOURCE),
              'test_sha256': sha(TEST), 'workload_source_sha256': sha(WORKLOAD),
              'build_command': build, 'command': command, 'budget_bytes': 16777216,
              'read_bytes': 131072, 'performance_claim': False, 'numeric_admission': False}
    start = time.monotonic()
    try:
        build_run = subprocess.run(build, cwd=ROOT, capture_output=True, timeout=30)
        (output / 'build.stdout').write_bytes(build_run.stdout)
        (output / 'build.stderr').write_bytes(build_run.stderr)
        result['build_exit'] = build_run.returncode
        if build_run.returncode == 0:
            result['helper_sha256'] = sha(helper)
            try:
                test = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, timeout=150)
                (output / 'test.stdout').write_bytes(test.stdout)
                (output / 'test.stderr').write_bytes(test.stderr)
                result['test_exit'] = test.returncode
            except subprocess.TimeoutExpired as error:
                (output / 'test.stdout').write_bytes(error.stdout or b'')
                (output / 'test.stderr').write_bytes(error.stderr or b'')
                result['timeout'] = True
            passed = (result.get('test_exit') == 0
                      and b'VIEW_BUDGET fixed=16777216 response=131072 capacity_before_bytes=true no_partial_entry=true'
                      in (output / 'test.stdout').read_bytes()
                      and b'VIEW_BUDGET_LOOKUP_CAPACITY' in (output / 'test.stderr').read_bytes()
                      and b'VIEW_BUDGET_RESPONSE' in (output / 'test.stderr').read_bytes())
            result['status'] = ('DIAGNOSTIC_PASS' if passed and mode == 'diagnostic'
                                else 'PASS' if passed else 'FAIL')
    finally:
        result['wall_seconds'] = time.monotonic() - start
        (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        (output / 'SHA256SUMS').write_text(''.join(
            sha(p) + '  ' + str(p.relative_to(output)) + '\n'
            for p in sorted(output.rglob('*')) if p.is_file() and p.name != 'SHA256SUMS'))
        print(json.dumps({'status': result['status'], 'source': result['source'],
                          'output': str(output)}))


if __name__ == '__main__':
    main()
