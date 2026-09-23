#!/usr/bin/env python3
"""Run one source-pinned functional LayerFS SDK telemetry diagnostic."""
import hashlib
import json
import os
import platform
import secrets
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

repo = Path(__file__).resolve().parents[3]
assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=repo).strip()
source_commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
assert source_commit == '0edc58ce8d4f21115a1eb27e2964290426f418bc'
image = 'sha256:e08b4efc6d4ea1be3e8df2fbeaccd60581ecaeb0551c1a440c9fd538ecf2e961'
assert subprocess.check_output(['docker', 'image', 'inspect', image, '--format', '{{.Id}}'], text=True).strip() == image
run = secrets.randbits(128) or 1
output = Path(tempfile.mkdtemp(prefix='sdk-route-one-control-', dir=repo/'core/target/sdk-proof/evidence'))
command = ['cargo', '+1.85.1', 'test', '--manifest-path', 'core/Cargo.toml', '--locked', '-p', 'layerfs-sdk', '--test', 'agent_route', '--', '--nocapture']
test_binary = repo/'core/target/debug/deps/agent_route-0ed57b7eb94132f2'
sha256 = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
identity = {
    'source_commit': source_commit,
    'source_tree': subprocess.check_output(['git', 'rev-parse', 'HEAD^{tree}'], cwd=repo, text=True).strip(),
    'source_status': '', 'image_id': image,
    'image_base_digest': 'sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8',
    'sdk_test_binary_sha256': sha256(test_binary),
    'harness_source_sha256': sha256(repo/'core/crates/layerfs-api/sdk/tests/agent_route.rs'),
    'reporter_sha256': sha256(repo/'core/tools/sdk_telemetry_report.py'),
    'telemetry_run': run, 'sample_count': 1,
    'cache_contract': 'uncontrolled OS/source cache; diagnostic only',
    'host': platform.platform(),
}
(output/'identities.json').write_text(json.dumps(identity, indent=2, sort_keys=True)+'\n')
(output/'command.txt').write_text('LAYERFS_TEST_IMAGE='+image+' LAYERFS_TEST_TELEMETRY_RUN='+str(run)+' LAYERFS_TEST_TELEMETRY_OUTPUT='+str(output)+' '+' '.join(command)+'\n')
shutil.copyfile(__file__, output/'runner.py')
env = dict(os.environ, LAYERFS_TEST_IMAGE=image, LAYERFS_TEST_TELEMETRY_RUN=str(run), LAYERFS_TEST_TELEMETRY_OUTPUT=str(output))
started = time.monotonic_ns()
with (output/'raw-host.log').open('wb') as log:
    try:
        result = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=15)
        code = result.returncode
    except subprocess.TimeoutExpired:
        code = 124
elapsed = time.monotonic_ns()-started
(output/'exit-status.txt').write_text(str(code)+'\n')
(output/'external-wall.json').write_text(json.dumps({'complete_command_wall_ns':elapsed,'timeout_seconds':15},indent=2)+'\n')
if code == 0:
    report = subprocess.run(['python3', 'core/tools/sdk_telemetry_report.py', str(output)], cwd=repo, capture_output=True, text=True)
    print(report.stdout, end='')
    if report.returncode:
        print(report.stderr, end='')
        code = report.returncode
print(json.dumps({'output':str(output),'exit_code':code,'external_wall_ns':elapsed}))
raise SystemExit(code)
