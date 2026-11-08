// Read the SAME saved Store through each real front end. Full identities, nulls,
// coverage and refusals must agree; only actual scan-work counters may differ
// between cached persistent clients and fresh CLI processes. No ID/time rewrite.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { client, table, LayerFSError } from "../../src/index.js";
import { decode, encode } from "../../src/values.js";

const root = fileURLToPath(new URL("../../../../", import.meta.url));
const driver = fileURLToPath(new URL("../contract/driver.py", import.meta.url));
const python = process.env.LAYERFS_PYTHON ?? "python3";
const command = process.env.LAYERFS_CLI ?? join(root, "target/debug/layerfs");
const specs = new Map(table.map(s => [`${s.family}.${s.call}`, s]));

export function semantics(value) {
  if (Array.isArray(value)) return value.map(semantics);
  if (!value || typeof value !== "object" || Buffer.isBuffer(value)) return value;
  return Object.fromEntries(Object.entries(value).map(([key, item]) => {
    if (key === "scan" && item && typeof item === "object") {
      const fields = Object.entries(item).filter(([name, count]) => {
        if (!["examined", "candidates_examined"].includes(name)) return true;
        assert.ok(Number.isInteger(count) && count >= 0 && count <= 10_000, `${name}: bounded actual work`);
        return false;
      });
      return [key, semantics(Object.fromEntries(fields))];
    }
    return [key, semantics(item)];
  }));
}

function processResult(program, args, options = {}) {
  const r = spawnSync(program, args, { encoding: "utf8", timeout: 20_000, maxBuffer: 4 << 20, ...options });
  assert.equal(r.error, undefined, `${program}: ${r.error}`);
  assert.equal(r.signal, null, `${program}: ${r.signal}`);
  return r;
}

export function requireFrontends() {
  assert.ok(existsSync(command), `Build the CLI or set LAYERFS_CLI: ${command}`);
  const r = processResult(python, ["-B", "-c", `import sys; sys.path.insert(0, ${JSON.stringify(join(root, "api/python/src"))}); import layerfs`]);
  assert.equal(r.status, 0, `Build Python's native module: ${r.stderr}`);
}

export async function parity(home, cases, scope) {
  const lfs = client({ home, ...(scope ? { scope } : {}) });
  const node = [];
  for (const step of cases) {
    const [family, call] = step.call.split(".");
    try {
      node.push({ ok: await lfs[family][call](...step.args, ...(Object.keys(step.opts ?? {}).length ? [step.opts] : [])) });
    } catch (e) {
      assert.ok(e instanceof LayerFSError, `${step.name}: ${e.stack}`);
      node.push({ refused: { code: e.code, message: e.message, detail: e.detail } });
    }
  }
  const fronts = { node };
  for (const aio of [false, true]) {
    const input = encode({ home, aio, scope: scope ?? null, steps: cases.map(s => ({ call: s.call, args: s.args, opts: s.opts ?? {} })) });
    const r = processResult(python, ["-B", driver], { input });
    assert.equal(r.status, 0, `Python aio=${aio}: ${r.stderr}`);
    fronts[aio ? "aio" : "python"] = decode(r.stdout);
  }
  fronts.cli = cases.map(step => {
    const spec = specs.get(step.call);
    assert.ok(spec, `No call table entry: ${step.call}`);
    const words = [spec.family, spec.call, ...step.args.map(String)];
    for (const [name, value] of Object.entries(step.opts ?? {})) {
      words.push(`--${name.replaceAll("_", "-")}=${typeof value === "object" ? encode(value) : value}`);
    }
    if (scope) words.push("--scope", scope.folder, "--scope-project", scope.project, "--scope-branch", scope.branch);
    const r = processResult(command, words, { env: { ...process.env, LAYERFS_HOME: home } });
    if (r.status === 0) return { ok: decode(r.stdout) };
    assert.equal(r.status, 125, `${step.name}: expected SDK refusal, got ${r.status}: ${r.stderr}`);
    return { refused: decode(r.stderr.trim().split("\n").at(-1)) };
  });
  cases.forEach((step, i) => {
    for (const [front, answers] of Object.entries(fronts)) {
      assert.deepEqual(semantics(answers[i]), semantics(node[i]), `${step.name}: ${front} differs from Node on the same pinned Store`);
      if (step.error) assert.equal(answers[i].refused?.code, step.error, `${step.name}: ${front}`);
      else {
        assert.ok(answers[i].ok, `${step.name}: ${front}: ${encode(answers[i])}`);
        step.check?.(answers[i].ok);
      }
    }
  });
  return node.map(v => v.ok ?? v.refused);
}
