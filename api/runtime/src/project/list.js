// runtime.project.list(): every project of this home. A project is its home
// entry (`<home>/<name>/project.record`); the runtime's own record,
// `<stateDir>/projects/<name>.json` { name, volume, image?, source, created },
// written by start_init, adds its volume. A project made elsewhere has no
// record, so its volume is unknown here (project.remove then needs it named).
// Since #506 a Store may hold many projects: those of shared Stores are
// listed from the home's stores.json (store/registry.js), with their Store.
import { existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { LayerFSError } from "@layerfs/agent";
import { entries } from "../store/registry.js";

export const NAME = /^[a-z0-9](?:[a-z0-9_.-]{0,61}[a-z0-9])?$/;
const dir = (settings) => join(settings.stateDir, "projects");

/** The runtime's record of project `name`, or null. */
export function record(settings, name) {
  try { return JSON.parse(readFileSync(join(dir(settings), `${name}.json`), "utf8")); } catch { return null; }
}

export function saveRecord(settings, project) {
  mkdirSync(dir(settings), { recursive: true });
  const file = join(dir(settings), `${project.name}.json`);
  writeFileSync(`${file}.new`, JSON.stringify(project));
  renameSync(`${file}.new`, file);
}

export const dropRecord = (settings, name) => rmSync(join(dir(settings), `${name}.json`), { force: true });

/** Projects with a Store of their own (#506 `legacy`): `[{ name, store, kind, volume, image, source, created, initialized, shared: false }]`, by name. */
export async function legacyProjects(runtime) {
  const { settings, platform } = runtime;
  const homes = new Set(await platform.homeNames(settings));
  const recorded = existsSync(dir(settings)) ? readdirSync(dir(settings)).filter((f) => f.endsWith(".json")).map((f) => f.slice(0, -5)) : [];
  return [...new Set([...homes, ...recorded])].filter((name) => NAME.test(name)).sort().map((name) => {
    const r = record(settings, name);
    return { name, store: name, kind: "sqlite", volume: r?.volume ?? null, image: r?.image ?? null, source: r?.source ?? null, created: r?.created ?? null, initialized: homes.has(name), shared: false };
  });
}

/** Every project of every Store of this home, by name then Store: those
 * with a Store of their own, and those of shared Stores (stores.json),
 * which carry `shared: true`. */
export async function list(runtime) {
  const shared = entries(runtime.settings).flatMap((s) => (s.projects ?? []).filter((p) => !(s.legacy && p.name === s.name)).map((p) => ({
    name: p.name, store: s.name, kind: s.kind, volume: s.volume ?? null, image: null, source: p.source ?? null, created: p.created ?? null, initialized: true, shared: true,
  })));
  return [...(await legacyProjects(runtime)), ...shared].sort((a, b) => a.name.localeCompare(b.name) || a.store.localeCompare(b.store));
}

/** The project `name` (of `store` when given); a name in several Stores needs its Store. */
export async function find(runtime, name, store) {
  const found = (await list(runtime)).filter((p) => p.name === name && (store === undefined || p.store === store));
  if (!found.length) throw new LayerFSError("NoSuchProject", `no LayerFS project named ${name}${store ? ` in Store ${store}` : ""} in ${runtime.settings.home}`, null);
  if (found.length > 1) throw new LayerFSError("InvalidInput", `project ${name} is in Stores ${found.map((p) => p.store).join(", ")}: name its { store }`, null);
  return found[0];
}

/** The key a project's client and Sandboxes go by: `store/name` in a shared Store. */
export const keyOf = (project) => (project.shared ? `${project.store}/${project.name}` : project.name);
