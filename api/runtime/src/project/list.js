// runtime.project.list(): every project of this home. A project is its home
// entry (`<home>/<name>/project.record`); the runtime's own record,
// `<stateDir>/projects/<name>.json` { name, volume, image?, source, created },
// written by start_init, adds its volume. A project made elsewhere has no
// record, so its volume is unknown here (project.remove then needs it named).
import { existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

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

/** `[{ name, volume, image, source, created, initialized }]`, by name. */
export async function list(runtime) {
  const { settings, platform } = runtime;
  const homes = new Set(await platform.homeNames(settings));
  const recorded = existsSync(dir(settings)) ? readdirSync(dir(settings)).filter((f) => f.endsWith(".json")).map((f) => f.slice(0, -5)) : [];
  return [...new Set([...homes, ...recorded])].filter((name) => NAME.test(name)).sort().map((name) => {
    const r = record(settings, name);
    return { name, volume: r?.volume ?? null, image: r?.image ?? null, source: r?.source ?? null, created: r?.created ?? null, initialized: homes.has(name) };
  });
}
