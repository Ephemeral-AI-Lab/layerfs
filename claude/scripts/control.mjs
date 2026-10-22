#!/usr/bin/env node
// Operator setup, outside an agent session. No proxy and no automatic import.
import { mkdirSync, existsSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { configPath, loadConfig, checkDeployment, nativeSDK, sdkHash, sha } from './runtime.mjs';
import { read, write, requireThat } from './state.mjs';

const [command, argument, source] = process.argv.slice(2);
try {
  const root = resolve(process.env.CC_LAYERFS_CONTROL || process.cwd());
  if (command === 'configure') {
    requireThat(argument && !source, 'usage: control.mjs configure /absolute/deployment.json (from the control directory)');
    requireThat(!existsSync(join(root, '.git')), 'choose a dedicated control directory, not a checkout');
    const input = read(resolve(argument));
    requireThat(input.profile === 'Disposable', 'select Disposable explicitly');
    mkdirSync(input.home, { recursive: true, mode: 0o700 });
    const config = { ...input, schema: 1, build: { sdk: sdkHash(input.sdk), native: sha(input.native), daemon: sha(input.daemon) } };
    // Validate before replacing the selected deployment.
    const previous = existsSync(configPath(root));
    requireThat(!previous, 'configuration already exists; preserve it and its sessions before explicitly replacing it');
    write(configPath(root), config);
    const loaded = loadConfig(root);
    checkDeployment(loaded);
    console.log(`Configured ${loaded.project} in ${root}. Use doctor to recheck, import to initialize once, then start Claude.`);
  } else if (command === 'doctor' || command === 'import') {
    const config = loadConfig(root);
    checkDeployment(config);
    const { LayerFS } = await nativeSDK(config, root);
    const lf = new LayerFS(config.project);
    try {
      if (command === 'import') {
        requireThat(argument && !source, 'usage: control.mjs import /absolute/source (once per project)');
        console.log(JSON.stringify(await lf.project.init(resolve(argument)), null, 2));
      } else {
        requireThat(!argument, 'usage: control.mjs doctor');
        console.log(JSON.stringify(await lf.project.list_branches(), null, 2));
        console.log('Deployment and direct Store access checked. Claude tool loading and Docker/FUSE execution require the acceptance session.');
      }
    } finally { await lf.close(); }
  } else throw new Error('usage: node /path/to/claude/scripts/control.mjs configure <deployment.json> | import <source> | doctor');
} catch (error) { console.error(`${error.code ?? 'layerfs'}: ${error.message}`); process.exitCode = 2; }
