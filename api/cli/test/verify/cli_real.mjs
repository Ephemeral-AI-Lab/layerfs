// The layerfs command against the real SDK, Store, Docker and daemon (#428),
// checked for consistency with the SDK. One part of the shared scenario
// (api/agent/test/verify/scenario.mjs, every agent call) runs
// twice, each time on its own fresh project and Store:
// - first through the SDK in this process, each step checked for
//   correctness;
// - then as `layerfs` commands, one process per call, with the same checks.
//   The SDK run's Store is removed first, and this run's Init creates a new
//   one at the same path.
// Each step's command must print the SDK's answer as JSON, or exit 125 with
// the SDK's refusal; with --stream, pass through the same output and exit
// code the SDK's sinks receive. The two runs are compared after masking what differs
// between any two runs: the project's name, object ids, times, inode numbers
// and a Commit's stored bytes (PER_RUN).
// Disposable Store only. Under the E2E launcher (its Store volume,
// deployment and sealed Linux bundle for this commit):
//   python3 tools/worktree.py run --resources <resources.json> --bundle <sealed bundle> --timeout 100 -- node api/cli/test/verify/cli_real.mjs inner <store|workspace>
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { PARTS, source, throughSdk, verify } from "../../../agent/test/verify/scenario.mjs";

const [mode, part] = process.argv.slice(2);
if (mode !== "inner" || !PARTS[part]) throw new Error("usage: cli_real.mjs inner <store|workspace> (under tools/worktree.py run)");
const CLI = fileURLToPath(new URL("../../src/layerfs.js", import.meta.url));
const { LayerFS } = await import("@layerfs/agent");
const { plain } = await import("../../src/output.js");
const steps = PARTS[part];
const work = mkdtempSync(join(tmpdir(), "layerfs-cli-real-"));
const store = dirname(process.env.LAYERFS_STORE_PATH);
const stamp = Date.now().toString(36);

/** A run's own project and home. Both runs use the launcher's Store path,
 * the one place a Sandbox's daemon opens the Store, one run at a time. */
function run(name) {
  return { project: `cli-${part}-${name}-${stamp}`, source: source(join(work, `source-${name}`)),
    env: { LAYERFS_HOME: join(work, `home-${name}`) } };
}

/** One step as one `layerfs` command: its answer as the SDK's would be. */
function command(step, ctx) {
  const argv = step.argv(ctx);
  const started = Date.now();
  const r = spawnSync(process.execPath, [CLI, ...argv], { input: step.input ?? "", env: { ...process.env, ...ctx.env, LAYERFS_PROJECT: ctx.project }, timeout: 60_000, maxBuffer: 64 << 20 });
  console.log(`layerfs ${argv.join(" ")}  exit ${r.status}  ${Date.now() - started} ms`);
  const [out, err] = [r.stdout.toString(), r.stderr.toString()];
  if (r.status === 125) return { error: JSON.parse(err) };
  // --stream: the command's own output and exit code, passed through
  if (step.stream) return { result: { exitCode: r.status, stdout: out, stderr: err } };
  assert.equal(r.status, 0, `layerfs ${argv.join(" ")}: ${err}`);
  assert.equal(err, "");
  const { ws, ...result } = JSON.parse(out);
  if (argv[1] === "mount") assert.equal(ws, `${result.sandbox}:${result.path}`);
  return { result };
}

// Numbers that belong to a run, not a call: each Store opener reserves its
// own block of inode numbers (every command is a new opener, an SDK client
// one), and a Commit's stored bytes include its compressed ids and time.
const PER_RUN = new Set(["inode", "newStored", "newMetadata"]);

/** An answer with what differs between any two runs masked. */
function masked(answer, ctx) {
  const text = (s) => s.split(ctx.project).join("<project>");
  const mask = (value, key = "") => {
    if (typeof value === "string") return text(value).replace(/[0-9a-f]{12,}/g, "<id>");
    if (typeof value === "number" && PER_RUN.has(key)) return `<${key}>`;
    if (typeof value === "number" && /(At|Ms|Us|_ms|_us|time|elapsed)$/i.test(key)) return "<time>";
    if (Array.isArray(value)) return value.map((v) => mask(v));
    if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, mask(v, k)]));
    return value;
  };
  if (!answer.error) return { result: mask(plain(answer.result)) };
  const { code, message, detail } = answer.error;
  return { error: mask({ code, message: String(message).replace(`${code}: `, ""), detail: plain(detail ?? null) }) };
}

const sdkRun = run("sdk");
const cliRun = run("cli");
const started = Date.now();
try {
  Object.assign(process.env, sdkRun.env);
  const { answers: want, wrong } = await throughSdk(steps, LayerFS, sdkRun, plain);
  console.log(`sdk: ${want.length} calls in ${Date.now() - started} ms`);
  // the command's run Inits its own project into a fresh Store at the same
  // path; the daemon requires the directory private (0700), as the launcher made it
  rmSync(store, { recursive: true, force: true });
  mkdirSync(store, { mode: 0o700 });
  const differ = [];
  wrong.forEach((failure) => differ.push(`sdk ${failure}`));
  try {
    for (const [i, step] of steps.entries()) {
      let answer;
      try {
        answer = command(step, cliRun);
      } catch (error) {
        differ.push(`layerfs ${step.name}: not run: ${error.message}`);
        continue;
      }
      const failure = verify(step, answer, cliRun);
      if (failure) differ.push(`layerfs ${failure}`);
      const [have, expected] = [masked(answer, cliRun), masked(want[i], sdkRun)];
      try {
        assert.deepEqual(have, expected);
      } catch {
        differ.push(`${step.name}:\n  layerfs ${JSON.stringify(have).slice(0, 4000)}\n  sdk     ${JSON.stringify(expected).slice(0, 4000)}`);
      }
    }
  } finally {
    // one attempt each, reported, never retried
    const env = { ...process.env, ...cliRun.env, LAYERFS_PROJECT: cliRun.project };
    if (cliRun.ws) console.log("left mounted:", spawnSync(process.execPath, [CLI, "workspace", "unmount", `${cliRun.ws.sandbox}:${cliRun.ws.path}`, "--discard"], { env }).status);
    if (cliRun.sandbox && !cliRun.closed) console.log("left running:", spawnSync(process.execPath, [CLI, "sandbox", "close", cliRun.sandbox], { env }).status);
  }
  assert.deepEqual(differ, [], `wrong answers, or the command differs from the SDK:\n${differ.join("\n")}`);
  console.log(`cli_real ${part}: PASS, ${steps.length} calls each way in ${Date.now() - started} ms`);
} finally {
  rmSync(work, { recursive: true, force: true });
}
