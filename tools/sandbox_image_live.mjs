// One bounded live verification phase. Launched by sandbox_image_live.py;
// SDK routing and cleanup use the existing public API, never provider fallbacks.
import assert from 'node:assert/strict';
import { appendFileSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { join } from 'node:path';
const [work, phase] = process.argv.slice(2);
const plan = JSON.parse(readFileSync(join(work, 'plan.json'), 'utf8'));
const { client, close } = await import(pathToFileURL(plan.sdk));
const lfs = client({ home: plan.home });
const provider = phase.split('-')[0];
const owned = plan.providers[provider];
const record = (kind, value) => appendFileSync(join(work, 'events.ndjson'), JSON.stringify({ at: new Date().toISOString(), phase, kind, value }) + '\n');
const progress = value => record('progress', value);
const statePath = join(work, `${provider}-workspace.json`);
const action = phase.slice(provider.length + 1);
const sandbox = `${provider}-import`;
const reuse = `${provider}-reuse`;
const project = `${provider}-store/app`;
async function call(name, run) {
  record('call', { name });
  const result = await run();
  record('result', { name, result });
  return result;
}
try {
  assert.ok(owned, `unknown provider ${provider}`);
  if (action === 'machine') {
    if (provider === 'lima') await call('machine.create', () => lfs.machine.create(owned.machine, 'lima', { cpus: 2, memory: '4G', disk: '20G', ...(plan.osImage ? { os_image: plan.osImage } : {}), on_progress: progress }));
    else await call('machine.start', () => lfs.machine.start('docker', { on_progress: progress }));
    const inventory = await call('machine.images.before', () => lfs.machine.images(owned.machine));
    assert.ok(!inventory.images.some(image => image.digest === plan.digest));
  } else if (action === 'project') {
    await call('store.create', () => lfs.store.create(`${provider}-store`));
    await call('project.init', () => lfs.project.init(project, plan.source, { on_progress: progress }));
    await call('branch.fork', () => lfs.branch.fork(project, 'verify', 'main'));
    await close();
    await call('store.push', () => lfs.store.push(`${provider}-store`, owned.machine, { on_progress: progress }));
  } else if (action === 'import') {
    assert.ok(existsSync(plan.archive));
    const result = await call('sandbox.create.object', () => lfs.sandbox.create(project, sandbox, owned.machine, { image: { archive: plan.archive, digest: plan.digest }, on_progress: progress }));
    assert.equal(result.image, plan.digest);
    const inventory = await call('machine.images.after', () => lfs.machine.images(owned.machine));
    assert.ok(inventory.images.some(image => image.digest === plan.digest));
  } else if (action === 'exec' || action === 'reuse-exec') {
    const name = action === 'exec' ? sandbox : reuse;
    const mount = await call('workspace.mount', () => lfs.workspace.mount(name, 'verify', { name: 'proof' }));
    writeFileSync(statePath, JSON.stringify({ ws: mount.ws }));
    assert.equal(mount.image, plan.digest);
    const command = `set -eu; test "$(cat /.layerfs-image-proof)" = "${plan.identity}"; test "$(cat proof.txt)" = "custom-image-proof"; command -v bash; uname -s; cat /etc/os-release; found=; for f in /proc/[0-9]*/comm; do IFS= read -r name < "$f" || continue; if [ "$name" = layerfs-daemon ]; then found=yes; fi; done; test "$found" = yes; printf "injected-daemon-serving-workspace\\n"`;
    const result = await call('workspace.exec', () => lfs.workspace.exec(mount.ws, command));
    assert.equal(result.exit_code, 0);
    assert.match(String(result.stdout), /injected-daemon-serving-workspace/);
    await call('workspace.unmount', () => lfs.workspace.unmount(mount.ws));
    writeFileSync(statePath, JSON.stringify({ ws: null }));
  } else if (action === 'reuse') {
    assert.ok(!existsSync(plan.archive), 'caller archive must be deleted before digest reuse');
    const result = await call('sandbox.create.digest', () => lfs.sandbox.create(project, reuse, owned.machine, { image: plan.digest, on_progress: progress }));
    assert.equal(result.image, plan.digest);
  } else if (action === 'cleanup-sandboxes') {
    if (existsSync(statePath)) {
      const { ws } = JSON.parse(readFileSync(statePath));
      if (ws) await call('workspace.unmount', () => lfs.workspace.unmount(ws));
    }
    const listed = await lfs.sandbox.list();
    for (const name of [sandbox, reuse]) {
      if (listed.items.some(item => item.name === name)) await call('sandbox.close', () => lfs.sandbox.close(name, { force: true }));
    }
  } else if (action === 'cleanup-store') {
    const listed = await lfs.store.list();
    if (listed.items.some(item => item.name === `${provider}-store`)) await call('store.remove', () => lfs.store.remove(`${provider}-store`));
  } else if (action === 'cleanup-machine') {
    if (provider === 'lima') {
      const listed = await lfs.machine.list();
      if (listed.machines.some(item => item.name === owned.machine)) await call('machine.remove', () => lfs.machine.remove(owned.machine));
    } else {
      await call('machine.stop', () => lfs.machine.stop('docker'));
    }
  } else throw new Error(`unknown phase ${phase}`);
  record('passed', { phase });
} catch (error) {
  record('failed', { message: error.message, code: error.code, stack: error.stack });
  process.exitCode = 1;
} finally {
  await close();
}
