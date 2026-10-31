// #529 §8 old records, step one: with a bundle from before #529
// (LFS527_AGENT is its SDK), fork `lane` if it is not there yet and make one
// Sandbox with that SDK's own sandbox.create(), from the environment's
// deployment. The Sandbox stays running; its record has no platform.
//   node make.mjs <project>
// The last stdout line is `LFS529-RESULT <json>`.
import { pathToFileURL } from "node:url";

const [project] = process.argv.slice(2);
const out = { project, ok: false };
const { LayerFS } = await import(pathToFileURL(process.env.LFS527_AGENT).href);
let layerfs;
try {
  layerfs = await LayerFS.open(project);
  const branches = (await layerfs.project.list_branches(1000, 0)).items.map((b) => b.name ?? b.branch ?? b);
  if (!branches.includes("lane")) out.fork = await layerfs.branch.fork("lane", "main");
  out.created = await layerfs.sandbox.create();
  out.ok = true;
} catch (error) {
  out.cause = `${error?.code ?? ""} ${error?.message ?? error}`.trim();
} finally {
  await layerfs?.close().catch(() => {});
}
console.log(`LFS529-RESULT ${JSON.stringify(out)}`);
process.exit(0);
