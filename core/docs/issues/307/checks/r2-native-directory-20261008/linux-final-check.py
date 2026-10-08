from pathlib import Path
import hashlib, json, os, re, signal, subprocess, sys, time

out = Path('core/docs/issues/307/checks/r2-native-directory-20261008')
identity = json.loads((out / '38-final-source-identity.json').read_text())
assert all(hashlib.sha256(Path(p).read_bytes()).hexdigest() == h for p, h in identity.items())
base = ['--manifest-path', 'core/Cargo.toml', '--locked', '-p', 'layerfs-overlay', '-p', 'layerfs-workspace', '-p', 'layerfs-daemon', '--all-targets']
records = []
command = ['cargo', 'test', *base, '--no-run']
start = time.monotonic_ns()
build = subprocess.run(command, capture_output=True, text=True, timeout=300)
buildlog = build.stdout + build.stderr
print(buildlog, flush=True)
records.append({'command': command, 'exit_code': build.returncode, 'wall_ns': time.monotonic_ns() - start})
if build.returncode:
    sys.exit(build.returncode)
binaries = {}
package = None
for line in buildlog.splitlines():
    found = re.search(r'Executable unittests src/lib.rs \([^)]*/deps/(layerfs_(?:overlay|workspace|daemon))-', line)
    if found:
        package = found[1].replace('_', '-')
    found = re.search(r'Executable tests/([^ ]+)\.rs \(([^)]+)\)', line)
    if found:
        binaries[(package, found[1])] = found[2]
selection = json.loads((out / '39-final-linux-selection.json').read_text())
for package, names in selection['selected'].items():
    for name in names:
        binary = binaries[(package, name)]
        command = [binary, '--test-threads=1', '--nocapture']
        limit = 9 if name == 'native_application' else 100
        start = time.monotonic_ns()
        expired = False
        path = out / ('linux-final-' + package + '-' + name + '.txt')
        with path.open('x') as log:
            process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            try:
                code = process.wait(timeout=limit)
            except subprocess.TimeoutExpired:
                expired = True
                os.killpg(process.pid, signal.SIGKILL)
                code = process.wait()
        record = {'command': command, 'sha256': hashlib.sha256(Path(binary).read_bytes()).hexdigest(), 'exit_code': code, 'timeout': expired, 'wall_ns': time.monotonic_ns() - start, 'timeout_seconds': limit, 'output': str(path)}
        records.append(record)
        print(json.dumps(record), flush=True)
        (out / '42-final-linux-tests.json').write_text(json.dumps(records, indent=2) + '\n')
        if code:
            print(path.read_text(), flush=True)
            sys.exit(code)
command = ['cargo', 'clippy', *base, '--', '-D', 'warnings']
start = time.monotonic_ns()
check = subprocess.run(command, timeout=300)
records.append({'command': command, 'exit_code': check.returncode, 'wall_ns': time.monotonic_ns() - start})
(out / '42-final-linux-tests.json').write_text(json.dumps(records, indent=2) + '\n')
sys.exit(check.returncode)
