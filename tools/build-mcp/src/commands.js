import { spawn } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repository = fileURLToPath(new URL('../../', import.meta.url));
const windowsPrefix = ['-d', 'Ubuntu', '-u', 'root', '--cd', '/mnt/e/layerfs-connect/layerfs'];
const namePattern = /^[A-Za-z0-9][A-Za-z0-9_.-]*$/;

async function spawnRunner(command, args, { timeout }) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: repository,
      shell: false,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    const stdout = [];
    const stderr = [];
    child.stdout.on('data', (chunk) => stdout.push(chunk));
    child.stderr.on('data', (chunk) => stderr.push(chunk));
    child.once('error', reject);
    const timer = setTimeout(() => child.kill(), timeout);
    child.once('close', (status) => {
      clearTimeout(timer);
      resolve({
        status,
        stdout: Buffer.concat(stdout).toString('utf8'),
        stderr: Buffer.concat(stderr).toString('utf8'),
      });
    });
  });
}

function onlyKeys(input, allowed) {
  if (!input || Object.getPrototypeOf(input) !== Object.prototype) throw new Error('Expected an options object');
  for (const key of Object.keys(input)) {
    if (!allowed.includes(key)) throw new Error(`Unsupported option: ${key}`);
  }
}

function checkedName(value, label) {
  if (typeof value !== 'string' || !namePattern.test(value)) throw new Error(`Invalid ${label}`);
  if (value.includes('fs-bench-pro')) throw new Error('fs-bench-pro is frozen');
  return value;
}

export function createOperations({ platform = process.platform, resources, runner = spawnRunner }) {
  if (typeof resources !== 'string' || !path.posix.isAbsolute(resources) || !resources.endsWith('/resources.json')) {
    throw new Error('A pinned absolute resources.json path is required');
  }

  function execute(command, args, timeout = 100000) {
    return platform === 'win32'
      ? runner('wsl.exe', [...windowsPrefix, command, ...args], { timeout })
      : runner(command, args, { timeout });
  }

  function launcher(action, args, timeout) {
    return execute('python3', ['tools/worktree.py', action, '--resources', resources, ...args], timeout);
  }

  return {
    async status() {
      const status = await execute('git', ['status', '--short', '--branch'], 10000);
      const revision = await execute('git', ['rev-parse', 'HEAD'], 10000);
      return { ...revision, stdout: status.stdout + revision.stdout, resources };
    },

    async prepare() {
      return launcher('prepare', ['--platform', 'linux']);
    },

    async build(input) {
      onlyKeys(input, ['profile', 'jobs']);
      const profile = input.profile ?? 'debug';
      const jobs = input.jobs ?? 2;
      if (!['debug', 'release'].includes(profile)) throw new Error('Profile must be debug or release');
      if (!Number.isInteger(jobs) || jobs < 1 || jobs > 32) throw new Error('Jobs must be in [1, 32]');
      return launcher('build', ['--platform', 'linux', '--profile', profile, '--jobs', String(jobs)]);
    },

    async cargoTest(input) {
      onlyKeys(input, ['package', 'testTarget', 'timeout']);
      const packageName = checkedName(input.package, 'package');
      const testTarget = checkedName(input.testTarget, 'test target');
      const timeout = input.timeout ?? 90;
      if (typeof timeout !== 'number' || !Number.isFinite(timeout) || timeout <= 0 || timeout > 100) {
        throw new Error('Test timeout must be in (0, 100] seconds');
      }
      return launcher('cargo', [
        '--platform', 'linux', '--timeout', String(timeout), '--', 'test', '-p', packageName,
        '--test', testTarget,
      ]);
    },

    async checkpoint() {
      const status = await execute('git', ['status', '--porcelain=v1', '-z'], 10000);
      const revision = await execute('git', ['rev-parse', 'HEAD'], 10000);
      const diff = await execute('git', ['diff', '--binary', 'HEAD'], 10000);
      for (const result of [status, revision, diff]) {
        if (result.status !== 0) throw new Error(`Checkpoint Git command failed: ${result.stderr}`);
      }
      const root = platform === 'win32' ? 'E:/layerfs-connect/backups' : '/mnt/e/layerfs-connect/backups';
      const stamp = new Date().toISOString().replaceAll(':', '-').replaceAll('.', '-');
      const directory = path.join(root, `checkpoint-${stamp}`);
      await mkdir(directory, { recursive: false });
      const patch = path.join(directory, 'changes.patch');
      await writeFile(patch, diff.stdout, { flag: 'wx' });
      const untracked = status.stdout.split('\0').filter((entry) => entry.startsWith('?? ')).map((entry) => entry.slice(3));
      const manifest = {
        revision: revision.stdout.trim(),
        patch: 'changes.patch',
        untracked,
        note: 'The binary diff excludes untracked files; review the exact target before restoring.',
      };
      await writeFile(path.join(directory, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n', { flag: 'wx' });
      return { status: 0, stdout: directory + '\n', stderr: '', receipt: directory };
    },
  };
}
