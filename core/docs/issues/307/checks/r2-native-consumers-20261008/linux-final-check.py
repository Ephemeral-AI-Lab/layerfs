"""Consumer checkpoint only; actual mount/Ready/drain remain unfinished."""
from pathlib import Path
import hashlib, json, os, re, signal, subprocess, time

out = Path('core/docs/issues/307/checks/r2-native-consumers-20261008')
identity = json.loads((out / '23-final-source-identity.json').read_text())
assert all(hashlib.sha256(Path(p).read_bytes()).hexdigest() == digest for p, digest in identity.items())
records = []
def run(name, command, limit=100):
    Path('benchmark_agent_report.md').read_text()
    started = time.monotonic_ns()
    expired = False
    with (out / name).open('x') as log:
        process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try: code = process.wait(timeout=limit)
        except subprocess.TimeoutExpired:
            expired = True
            os.killpg(process.pid, signal.SIGKILL)
            code = process.wait()
    record = {'command': command, 'exit': code, 'timeout': expired,
              'wall_bound_seconds': limit, 'elapsed_ns': time.monotonic_ns()-started, 'output': name}
    if Path(command[0]).is_file():
        record['binary_sha256'] = hashlib.sha256(Path(command[0]).read_bytes()).hexdigest()
    records.append(record)
    (out / 'linux-final-results.json').write_text(json.dumps(records, indent=2) + '\n')
    print(json.dumps(record), flush=True)
    output = (out / name).read_text()
    print(output[-6000:], flush=True)
    if code: raise SystemExit(code)
    return output

run('linux-final-fuser-integrity.txt', ['python3', '-B', 'core/tools/check_fuser_integrity.py'])
base = ['--manifest-path', 'core/Cargo.toml', '--locked']
run('linux-final-fuse-build.txt', ['cargo', 'test', *base, '-p', 'layerfs-fuse', '--all-targets', '--no-run'])
output = run('linux-final-port-build.txt', ['cargo', 'test', *base, '-p', 'layerfs-daemon', '--test', 'filesystem_port', '--no-run'])
binary = re.search(r'Executable tests/filesystem_port.rs \(([^)]+)\)', output).group(1)
run('linux-final-clippy.txt', ['cargo', 'clippy', *base, '-p', 'layerfs-fuse', '-p', 'layerfs-daemon', '-p', 'layerfs-overlay', '--all-targets', '--', '-D', 'warnings'])
assert all(hashlib.sha256(Path(p).read_bytes()).hexdigest() == digest for p, digest in identity.items())
