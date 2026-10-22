import { accessSync, constants, readFileSync, readdirSync, statfsSync } from 'node:fs';
import { dirname, join, resolve, isAbsolute, relative } from 'node:path';
import { release } from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { read, key, requireThat } from './state.mjs';

export const pluginRoot = fileURLToPath(new URL('../', import.meta.url));
export const configPath = (root) => join(root, '.layerfs-claude', 'config.json');
export const sha = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');
export function sdkHash(root) {
  const hash = createHash('sha256');
  function visit(part) {
    for (const entry of readdirSync(join(root, part), { withFileTypes: true }).sort((a,b) => a.name.localeCompare(b.name))) {
      const path = join(part, entry.name);
      if (entry.isDirectory()) visit(path);
      else if (entry.name.endsWith('.js')) hash.update(path).update(readFileSync(join(root, path)));
    }
  }
  visit('src'); visit('operator');
  return hash.digest('hex');
}
export function controlRoot(cwd) {
  if (process.env.CC_LAYERFS_CONTROL) return resolve(process.env.CC_LAYERFS_CONTROL);
  for (let dir = resolve(cwd); ; dir = dirname(dir)) {
    try { accessSync(configPath(dir)); return dir; } catch (error) { if (error.code !== 'ENOENT') throw error; }
    requireThat(dirname(dir) !== dir, 'no .layerfs-claude/config.json; configure a dedicated control directory first');
  }
}
export function loadConfig(root) {
  const config = read(configPath(root));
  requireThat(config.schema === 1 && config.profile === 'Disposable', 'configuration requires schema 1 and Disposable profile');
  requireThat(/^[a-z0-9](?:[a-z0-9_.-]{0,61}[a-z0-9])?$/.test(config.project), 'invalid project name');
  for (const field of ['sdk', 'native', 'daemon', 'home', 'storePath']) {
    requireThat(typeof config[field] === 'string' && isAbsolute(config[field]), `${field} must be an absolute Linux path`);
  }
  for (const field of ['docker', 'volume', 'image']) requireThat(typeof config[field] === 'string' && config[field], `${field} is required`);
  requireThat(/^sha256:[a-f0-9]{64}$/.test(config.image), 'image must be a local image ID (sha256:...)');
  requireThat(process.platform === 'linux' && Number(process.versions.node.split('.')[0]) >= 20, 'Linux and Node >=20 are required');
  const pkg = read(join(config.sdk, 'package.json'));
  requireThat(pkg.name === '@layerfs/sdk' && pkg.version === '1.0.0', 'this plugin targets @layerfs/sdk 1.0.0');
  const compatibility = read(join(pluginRoot, 'compatibility.json'));
  requireThat(sdkHash(config.sdk) === compatibility.sdk.sha256, 'SDK source differs from the plugin compatibility pin; rebuild/sync the plugin for that source');
  requireThat(config.build?.sdk === compatibility.sdk.sha256, 'configure the deployment before starting Claude');
  return { ...config, identity: key(JSON.stringify(config)) };
}
export function checkDeployment(config) {
  requireThat(config.docker.startsWith('/') || config.docker.startsWith('unix:///'), 'use a local Unix Docker socket; remote engines are unsupported');
  accessSync('/bin/bash', constants.X_OK);
  accessSync('/dev/fuse', constants.R_OK | constants.W_OK);
  accessSync(config.native, constants.R_OK);
  accessSync(config.daemon, constants.R_OK | constants.X_OK);
  accessSync(config.home, constants.R_OK | constants.W_OK);
  accessSync(dirname(config.storePath), constants.R_OK | constants.W_OK | constants.X_OK);
  const type = statfsSync(dirname(config.storePath)).type;
  requireThat([0xef53, 0x58465342, 0x9123683e].includes(type), 'Store must be on local ext4/xfs/btrfs; Windows drives and network filesystems are unsupported');
  requireThat(sha(config.native) === config.build.native && sha(config.daemon) === config.build.daemon, 'native addon or daemon changed; configure the matched build again');
  const docker = (...args) => execFileSync('docker', ['--host', config.docker, ...args], { encoding: 'utf8', timeout: 5000, stdio: ['ignore','pipe','pipe'] });
  const engine = JSON.parse(docker('info', '--format', '{{json .}}'));
  requireThat(engine.OSType === 'linux' && engine.KernelVersion === release(), 'Docker and the SDK must share the host Linux kernel');
  const volume = JSON.parse(docker('volume', 'inspect', config.volume))[0];
  requireThat(volume.Driver === 'local', 'Store volume must use the local Docker driver');
  const inside = relative(volume.Mountpoint, config.storePath);
  requireThat(inside && !inside.startsWith('..') && !isAbsolute(inside), 'storePath must be a file inside the selected Docker volume Mountpoint');
  // The SDK validates the actual Store and container mount when it opens them.
  docker('image', 'inspect', config.image);
}
export function environment(config, root, extra = {}, inherited = process.env) {
  const env = Object.fromEntries(Object.entries(inherited).filter(([k]) => !/^(LAYERFS_|DSH_LAYERFS_|CC_LAYERFS_)/.test(k) && !['BASH_ENV','ENV'].includes(k)));
  const fields = { HOME: config.home, DOCKER: config.docker, IMAGE: config.image, VOLUME: config.volume, DAEMON: config.daemon,
    STORE_PATH: config.storePath, CONTAINER_PREFIX: config.containerPrefix, DAEMON_HOST: config.daemonHost };
  for (const [name, value] of Object.entries(fields)) if (value) env[`DSH_LAYERFS_${name}`] = value;
  return { ...env, LAYERFS_NATIVE: config.native,
    DSH_LAYERFS_PROJECT: config.project,
    DSH_LAYERFS_SDK: pathToFileURL(join(pluginRoot, 'scripts/sdk-entry.mjs')).href,
    DSH_LAYERFS_OPERATOR: pathToFileURL(join(pluginRoot, 'scripts/operator-entry.mjs')).href,
    DSH_LAYERFS_CLI: join(pluginRoot, 'scripts/lfs.mjs'),
    DSH_LAYERFS_SKILL: join(pluginRoot, 'skills/layerfs-js-sdk/SKILL.md'),
    CC_LAYERFS_CONTROL: root, ...extra };
}
export async function nativeSDK(config, root) {
  const trace = {};
  if (process.env.CC_LAYERFS_CALL) {
    for (const name of ['TRACE','TRACE_STEP','TRACE_START']) {
      const value = process.env[`DSH_LAYERFS_${name}`];
      if (value) trace[`DSH_LAYERFS_${name}`] = value;
    }
  }
  const env = environment(config, root, trace);
  for (const name of Object.keys(process.env)) if (/^(LAYERFS_|DSH_LAYERFS_)/.test(name)) delete process.env[name];
  Object.assign(process.env, env);
  return import(pathToFileURL(join(config.sdk, 'src/index.js')).href);
}
