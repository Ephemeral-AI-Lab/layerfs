import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { test } from 'node:test';

function request(child, pending, message) {
  return new Promise((resolve, reject) => {
    pending.set(message.id, { resolve, reject });
    child.stdin.write(JSON.stringify(message) + '\n');
  });
}

test('stdio exposes exactly five tools and rejects unsafe input without a build', async (t) => {
  const child = spawn(process.execPath, ['src/server.js'], {
    cwd: new URL('..', import.meta.url),
    env: { ...process.env, LAYERFS_BUILD_RESOURCES: '/root/.cache/layerfs-worktrees/area-p/resources.json' },
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  t.after(() => child.kill());
  const pending = new Map();
  child.once('exit', (code) => {
    for (const waiter of pending.values()) waiter.reject(new Error(`MCP server exited with code ${code}: ${stderr}`));
    pending.clear();
  });
  const protocolLines = [];
  let stderr = '';
  let buffer = '';
  child.stdout.setEncoding('utf8');
  child.stdout.on('data', (chunk) => {
    buffer += chunk;
    while (buffer.includes('\n')) {
      const index = buffer.indexOf('\n');
      const line = buffer.slice(0, index);
      buffer = buffer.slice(index + 1);
      if (!line.trim()) continue;
      const message = JSON.parse(line);
      protocolLines.push(message);
      const waiter = pending.get(message.id);
      if (waiter) {
        pending.delete(message.id);
        waiter.resolve(message);
      }
    }
  });
  child.stderr.setEncoding('utf8');
  child.stderr.on('data', (chunk) => { stderr += chunk; });

  const initialized = await request(child, pending, {
    jsonrpc: '2.0', id: 1, method: 'initialize', params: {
      protocolVersion: '2026-07-28', capabilities: {}, clientInfo: { name: 'build-mcp-test', version: '1.0.0' },
    },
  });
  assert.equal(initialized.result.serverInfo.name, 'layerfs-area-p-build');
  child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');

  const listed = await request(child, pending, { jsonrpc: '2.0', id: 2, method: 'tools/list', params: {} });
  assert.deepEqual(listed.result.tools.map((tool) => tool.name).sort(), [
    'layerfs_build', 'layerfs_cargo_test', 'layerfs_checkpoint', 'layerfs_prepare', 'layerfs_status',
  ]);

  const invalid = await request(child, pending, {
    jsonrpc: '2.0', id: 3, method: 'tools/call', params: {
      name: 'layerfs_cargo_test',
      arguments: { package: 'layerfs-api', testTarget: 'api_contract', timeout: 90, workspace: true },
    },
  });
  assert.equal(invalid.result.isError, true);
  assert.match(invalid.result.content[0].text, /Unsupported option/);
  assert.equal(protocolLines.some((message) => message.method?.startsWith('notifications/message')), false);
  assert.doesNotMatch(stderr, /tools\/worktree\.py/);
});
