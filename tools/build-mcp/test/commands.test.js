import assert from 'node:assert/strict';
import { test } from 'node:test';

import { createOperations } from '../src/commands.js';

const resources = '/root/.cache/layerfs-worktrees/area-p/resources.json';
const windowsPrefix = [
  'wsl.exe', '-d', 'Ubuntu', '-u', 'root', '--cd', '/mnt/e/layerfs-connect/layerfs',
];

function fixture(platform = 'linux') {
  const calls = [];
  const operations = createOperations({
    platform,
    resources,
    runner: async (command, args, options) => {
      calls.push({ command, args, options });
      return { status: 0, stdout: 'receipt\n', stderr: '' };
    },
  });
  return { calls, operations };
}

test('status reads the registered checkout without a shell or mutable Git command', async () => {
  const { calls, operations } = fixture('win32');
  const result = await operations.status();
  assert.deepEqual(calls[0], {
    command: 'wsl.exe',
    args: [...windowsPrefix.slice(1), 'git', 'status', '--short', '--branch'],
    options: { timeout: 10000 },
  });
  assert.deepEqual(calls[1].args, [...windowsPrefix.slice(1), 'git', 'rev-parse', 'HEAD']);
  assert.equal(result.status, 0);
  assert.equal(result.resources, resources);
});

test('prepare selects the pinned Linux resources', async () => {
  const { calls, operations } = fixture();
  await operations.prepare();
  assert.deepEqual(calls, [{
    command: 'python3',
    args: ['tools/worktree.py', 'prepare', '--resources', resources, '--platform', 'linux'],
    options: { timeout: 100000 },
  }]);
});

test('build defaults to Linux debug with two jobs', async () => {
  const { calls, operations } = fixture();
  await operations.build({});
  assert.deepEqual(calls[0].args, [
    'tools/worktree.py', 'build', '--resources', resources, '--platform', 'linux',
    '--profile', 'debug', '--jobs', '2',
  ]);
});

test('build accepts only release or debug and positive bounded jobs', async () => {
  const { calls, operations } = fixture();
  await assert.rejects(operations.build({ profile: 'durable' }));
  await assert.rejects(operations.build({ jobs: 0 }));
  await assert.rejects(operations.build({ jobs: 33 }));
  await assert.rejects(operations.build({ platform: 'macos' }));
  assert.equal(calls.length, 0);
});

test('cargo test compiles and runs one named package test with a 90 second timeout', async () => {
  const { calls, operations } = fixture('win32');
  await operations.cargoTest({ package: 'layerfs-api', testTarget: 'api_contract', timeout: 90 });
  assert.deepEqual(calls[0].args, [
    ...windowsPrefix.slice(1), 'python3', 'tools/worktree.py', 'cargo', '--resources', resources,
    '--platform', 'linux', '--timeout', '90', '--', 'test', '-p', 'layerfs-api', '--test',
    'api_contract',
  ]);
  assert.deepEqual(calls[0].options, { timeout: 100000 });
});

test('cargo test refuses workspace, path and frozen benchmark overrides before spawning', async () => {
  const { calls, operations } = fixture();
  const base = { package: 'layerfs-api', testTarget: 'api_contract', timeout: 90 };
  for (const extra of [
    { workspace: true },
    { manifestPath: 'Cargo.toml' },
    { targetDir: 'target-other' },
    { package: 'fs-bench-pro' },
    { testTarget: 'api;whoami' },
    { package: 'layerfs-api;whoami' },
    { resources: '/tmp/other/resources.json' },
    { timeout: 101 },
    { timeout: 0 },
    { args: ['--workspace'] },
  ]) {
    await assert.rejects(operations.cargoTest({ ...base, ...extra }));
  }
  assert.equal(calls.length, 0);
});
