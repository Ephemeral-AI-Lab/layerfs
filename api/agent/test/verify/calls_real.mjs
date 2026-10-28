// Every agent call against the real Store, Docker and daemon (#428), checked
// for correctness: one part of the shared scenario (scenario.mjs), each step
// one SDK call made once on a fresh project. Disposable Store only. Under the
// E2E launcher (its Store volume, deployment and sealed Linux bundle):
//   node api/agent/test/verify/calls_real.mjs inner <store|workspace>
// The `layerfs` command runs the same scenario in api/cli/test/verify/cli_real.mjs.
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { PARTS, source, throughSdk } from "./scenario.mjs";

const [mode, part] = process.argv.slice(2);
if (mode !== "inner" || !PARTS[part]) throw new Error("usage: calls_real.mjs inner <store|workspace> (under tools/worktree.py run)");
const { LayerFS } = await import("@layerfs/agent");
const work = mkdtempSync(join(tmpdir(), "layerfs-calls-real-"));
// the run's own fresh Store, where its daemons open it (LAYERFS_STORE_PATH)
process.env.LAYERFS_HOME = join(work, "home");
const ctx = { project: `calls-${part}-${Date.now().toString(36)}`, source: source(join(work, "source")) };
const started = Date.now();
try {
  const { answers, wrong } = await throughSdk(PARTS[part], LayerFS, ctx);
  if (wrong.length) {
    console.error(`calls_real ${part}: FAIL, ${wrong.length} of ${answers.length} calls wrong\n${wrong.join("\n")}`);
    process.exitCode = 1;
  } else {
    console.log(`calls_real ${part}: PASS, ${answers.length} calls in ${Date.now() - started} ms`);
  }
} finally {
  rmSync(work, { recursive: true, force: true });
}
