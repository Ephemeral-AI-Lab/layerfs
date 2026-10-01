#!/usr/bin/env python3
"""One real-provider correctness diagnostic; no performance admission."""
from pathlib import Path
import argparse, fcntl, hashlib, json, os, secrets, shutil, socket, subprocess, time, urllib.request

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def free_port():
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]

def run():
    p = argparse.ArgumentParser()
    p.add_argument('--output', required=True)
    p.add_argument('--minio', required=True)
    p.add_argument('--image', required=True)
    p.add_argument('--driver', required=True)
    p.add_argument('--case-file')
    p.add_argument('--purpose', default='real-provider correctness diagnostic')
    args = p.parse_args()
    out = Path(args.output).resolve()
    out.mkdir(parents=True, exist_ok=False)
    root = Path(__file__).resolve().parents[3]
    status = subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=normal'], cwd=root, text=True)
    if status:
        raise RuntimeError('clean published source required; diagnostic source not sealed')
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    binary = out / 'minio'
    shutil.copyfile(args.minio, binary)
    binary.chmod(0o700)
    port, console = free_port(), free_port()
    if port == console:
        raise RuntimeError('port allocation collision; no retry')
    access, secret = 'p6-' + secrets.token_hex(8), secrets.token_hex(32)
    bucket = 'phase6-live-' + secrets.token_hex(8)
    private = out / 'private-provider.txt'
    fd = os.open(private, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, 'w') as file:
        file.write(f'127.0.0.1:{port}\n{bucket}\n{access}\n{secret}\n')
    env = dict(os.environ, MINIO_ROOT_USER=access, MINIO_ROOT_PASSWORD=secret,
               MINIO_BROWSER='off', MINIO_UPDATE='off', MINIO_COMPRESSION_ENABLE='off',
               LAYERFS_CONSTRUCTION_WORKERS='1')
    identity = {'source': source, 'image': args.image, 'driver_sha256': sha(args.driver),
                'minio_sha256': sha(binary), 'harness_sha256': sha(__file__),
                'dependency_lock_sha256': sha(root / 'core/benchmark/phase6-live/Cargo.lock'),
                'armv8_build_config_sha256': sha(root / '.cargo/config.toml'),
                'workload_source_sha256': sha(root / 'core/benchmark/phase6-live/src/driver.rs'),
                'cache': 'INELIGIBLE: OS/page cache unknown',
                'purpose': args.purpose,
                'complete_child_limit_seconds': 15, 'separate_proof_limit_seconds': 9.5}
    if args.case_file:
        identity['case_file_sha256'] = sha(args.case_file)
        identity['scenario_module_sha256'] = sha(root / 'core/benchmark/phase6-live/src/scenario.rs')
    (out / 'identity.json').write_text(json.dumps(identity, indent=2) + '\n')
    with open(out / 'minio-private.log', 'xb') as provider_log:
        os.chmod(out / 'minio-private.log', 0o600)
        process = subprocess.Popen([str(binary), 'server', str(out / 'data'),
                                    '--address', f'127.0.0.1:{port}', '--console-address', f'127.0.0.1:{console}'],
                                   env=env, stdout=provider_log, stderr=subprocess.STDOUT)
    (out / 'provider-owner.json').write_text(json.dumps({'pid': process.pid, 'binary': str(binary)}) + '\n')
    try:
        deadline = time.monotonic() + 15
        while True:
            if process.poll() is not None:
                raise RuntimeError('owned MinIO exited; inspect private log')
            try:
                with urllib.request.urlopen(f'http://127.0.0.1:{port}/minio/health/ready', timeout=1) as response:
                    if response.status == 200:
                        break
            except OSError:
                if time.monotonic() >= deadline:
                    raise RuntimeError('provider readiness deadline')
                time.sleep(.1)
        command = [str(Path(args.driver).resolve()), 'smoke', str(out / 'result'), str(private), args.image]
        if args.case_file:
            command[1] = 'scenario'
            command.append(str(Path(args.case_file).resolve()))
        start = time.monotonic()
        with open(out / 'driver.stdout', 'xb') as stdout, open(out / 'driver.stderr', 'xb') as stderr:
            try:
                child = subprocess.run(command, env=env, stdout=stdout, stderr=stderr, timeout=15)
                result = {'exit': child.returncode, 'wall_seconds': time.monotonic()-start}
            except subprocess.TimeoutExpired:
                result = {'exit': None, 'status': 'TIMEOUT', 'wall_seconds': time.monotonic()-start,
                          'custody': 'owned container/volume and accepted writes retained; inspect exact owner'}
        (out / 'invocation.json').write_text(json.dumps(result, indent=2) + '\n')
        if result['exit'] != 0:
            raise RuntimeError('integration diagnostic failed; raw evidence retained')
        statistics = []
        for line in (out / 'result/daemon.stderr').read_text().splitlines():
            if line.startswith('P6_METADATA_STATS '):
                statistics.append(json.loads(line[len('P6_METADATA_STATS '):]))
        (out / 'transport-statistics.json').write_text(json.dumps(statistics, indent=2) + '\n')
        proof_command = [str(Path(args.driver).resolve()), 'verify', str(out / 'result'), str(private)]
        if args.case_file:
            proof_command.append(str(Path(args.case_file).resolve()))
        proof_start = time.monotonic()
        with open(out / 'proof.stdout', 'xb') as stdout, open(out / 'proof.stderr', 'xb') as stderr:
            try:
                proof_child = subprocess.run(proof_command, env=env, stdout=stdout, stderr=stderr, timeout=9.5)
                proof_result = {'exit': proof_child.returncode, 'wall_seconds': time.monotonic()-proof_start}
            except subprocess.TimeoutExpired:
                proof_result = {'exit': None, 'status': 'TIMEOUT', 'wall_seconds': time.monotonic()-proof_start}
        proof_result['plan_sha256'] = sha(out / 'result/proof-plan.bin')
        (out / 'proof-invocation.json').write_text(json.dumps(proof_result, indent=2)+'\n')
        if proof_result['exit'] != 0:
            raise RuntimeError('separate proof failed; actual performance and data retained')
        performance = json.loads((out / 'result/performance.json').read_text())
        proof = json.loads((out / 'result/proof.json').read_text())
        if proof['proof_ms'] >= 9500:
            raise RuntimeError('proof bound exceeded; not an admission PASS')
        receipt = dict(performance, semantic_proof=proof['status'], proof_ms=proof['proof_ms'],
                       performance_child_wall_seconds=result['wall_seconds'],
                       proof_child_wall_seconds=proof_result['wall_seconds'], proof_plan_sha256=proof_result['plan_sha256'])
        (out / 'result/receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')
        print(json.dumps(result))
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=5)
        (out / 'provider-drain.json').write_text(json.dumps({'pid': process.pid, 'exit': process.returncode,
                                                           'process_drained': True, 'data_retained': True})+'\n')

def main():
    root = Path(__file__).resolve().parents[3]
    lock_path = root / 'benchmark-results/phase6-integration/.live-run.lock'
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    with lock_path.open('a+b') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        run()

if __name__ == '__main__':
    main()
