// One case list (contract/cases.mjs) through every front end against its own fresh Disposable
// home, then compared step by step after the ids, times and home paths are normalized (#32):
// Node (the package), Python (the package, sync, and `layerfs.aio`) and the command. Each case
// has a real success or a real refusal; refusals are compared by `code`, the key set
// {code, message, detail} and, unless the wording is the front end's own, `message` and `detail`.
//
// Needs the built addon, and for the other front ends:
//   Python   LAYERFS_PYTHON (default python3) able to import layerfs (maturin develop in api/python)
//   command  target/debug/layerfs (or under CARGO_TARGET_DIR, or LAYERFS_CLI)
// A front end that is absent is skipped with its reason, never counted as agreeing.
// Point TMPDIR at a filesystem with SQLite locks (ext4, xfs, btrfs).
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync, chmodSync, rmSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { client, doctor, close, table, LayerFSError } from "../src/index.js";
import { encode, decode } from "../src/values.js";
import { steps, P } from "./contract/cases.mjs";

const root = fileURLToPath(new URL("../../../", import.meta.url));
const driver = fileURLToPath(new URL("./contract/driver.py", import.meta.url));
const target = process.env.CARGO_TARGET_DIR ? resolve(root, process.env.CARGO_TARGET_DIR) : join(root, "target");
const command = process.env.LAYERFS_CLI || join(target, "debug", "layerfs");
const python = process.env.LAYERFS_PYTHON || "python3";
const specs = new Map(table.map((spec) => [`${spec.family}.${spec.call}`, spec]));

const probe = spawnSync(python, ["-B", "-c", `import sys; sys.path.insert(0, ${JSON.stringify(join(root, "api/python/src"))}); import layerfs`], { encoding: "utf8" });
const missing = {
  python: probe.status === 0 ? null : `${python} cannot import layerfs (set LAYERFS_PYTHON to an interpreter that can: maturin develop in api/python)`,
  cli: existsSync(command) ? null : `the layerfs command is not built: ${command} (cargo build -p layerfs-cli)`,
};
missing.aio = missing.python;

const temp = (tag) => {
  const dir = mkdtempSync(join(tmpdir(), `layerfs-contract-${tag}-`));
  chmodSync(dir, 0o700);
  return dir;
};
const resolveArgs = (step, ctx) => (typeof step.args === "function" ? step.args(ctx) : step.args);
const hasOptions = (step) => Object.keys(step.opts).length > 0;

/** JSON for the Python driver: bytes as `{"$bytes"}`, a bigint as its own digits. */
function toDriver(request) {
  return JSON.stringify(request, function replace(key, item) {
    const original = this[key];
    if (typeof original === "bigint") return JSON.rawJSON(original.toString());
    if (Buffer.isBuffer(original)) return { $bytes: original.toString("base64") };
    return item;
  });
}

/** The driver's JSON: a whole number beyond a safe integer is a bigint, `{"$bytes"}` a Buffer. */
function fromDriver(text) {
  return JSON.parse(text, (_key, item, context) => {
    if (typeof item === "number" && !Number.isSafeInteger(item) && /^-?\d+$/.test(context.source)) return BigInt(context.source);
    if (item && typeof item === "object" && !Array.isArray(item) && Object.keys(item).length === 1 && typeof item.$bytes === "string") {
      return Buffer.from(item.$bytes, "base64");
    }
    return item;
  });
}

async function runNode(ctx) {
  const lfs = client({ home: ctx.home });
  const out = [];
  for (const step of steps) {
    try {
      if (step.call === "doctor") {
        out.push({ ok: await doctor({ home: ctx.home }) });
        continue;
      }
      const [family, call] = step.call.split(".");
      const given = resolveArgs(step, ctx);
      out.push({ ok: await lfs[family][call](...given, ...(hasOptions(step) ? [step.opts] : [])) });
    } catch (error) {
      out.push(error instanceof LayerFSError
        ? { refused: { code: error.code, message: error.message, detail: error.detail } }
        : { crash: String(error?.stack ?? error) });
    }
  }
  close();
  return out;
}

function runPython(ctx, aio) {
  const request = { home: ctx.home, aio, steps: steps.map((step) => ({ call: step.call, args: resolveArgs(step, ctx), opts: { ...step.opts, ...step.pythonOpts } })) };
  const run = spawnSync(python, ["-B", driver], { input: toDriver(request), encoding: "utf8", timeout: 100_000, maxBuffer: 1 << 28 });
  assert.equal(run.error, undefined, String(run.error));
  assert.equal(run.status, 0, `the Python driver failed (aio=${aio}): ${run.stderr}`);
  return fromDriver(run.stdout);
}

/** The command line of `step`: positionals (a gap cannot be said), then `--name=value`; content and JSON on stdin. */
function cliWords(spec, step, ctx) {
  const given = resolveArgs(step, ctx);
  const words = [spec.family, spec.call];
  let input;
  let gap = false;
  given.forEach((value, i) => {
    const arg = spec.args[i];
    if (arg && (arg.kind === "content" || arg.kind === "json")) {
      input = arg.kind === "json" ? encode(value) : Buffer.from(value);
    } else if (value === null || value === undefined) {
      gap = true;
    } else {
      assert.ok(!gap, `${step.id}: the command cannot say a positional after a null one`);
      words.push(String(value));
    }
  });
  for (const [name, value] of Object.entries(step.opts)) {
    if (value === null || value === undefined || typeof value === "function") continue;
    const flag = `--${name.replaceAll("_", "-")}`;
    if (typeof value === "boolean") words.push(value ? flag : `${flag}=false`);
    else words.push(`${flag}=${typeof value === "object" ? encode(value) : String(value)}`);
  }
  return { words, input };
}

function runCli(ctx) {
  return steps.map((step) => {
    if (step.cli === "skip") return { skipped: true };
    const { words, input } = step.call === "doctor" ? { words: ["doctor"] } : cliWords(specs.get(step.call), step, ctx);
    const run = spawnSync(command, words, { env: { ...process.env, LAYERFS_HOME: ctx.home }, input: input ?? "", timeout: 60_000 });
    assert.equal(run.error, undefined, `${step.id}: ${run.error}`);
    const lines = run.stderr.toString().trim().split("\n");
    if (run.status === 0) return { ok: decode(run.stdout.toString()) };
    if (run.status === 125) return { refused: decode(lines[lines.length - 1]) };
    if (run.status === 2) return { usage: run.stderr.toString().trim() };
    return { crash: `exit ${run.status}: ${run.stderr}` };
  });
}

// --- normalizing: what differs between homes and runs but not in meaning
// times; inode numbers (a process hands out its own range); stored sizes (they hold the times)
const TIMES = new Set(["created", "time", "inode", "new_stored", "new_metadata"]);
const ids = (text, ctx) =>
  /^[0-9a-f]{32,}$/.test(text)
    ? "<id>"
    : text.replaceAll(ctx.home, "<home>").replaceAll(ctx.folder, "<folder>").replace(/@[0-9a-f]{12}\b/g, "@<id12>").replace(/at byte \d+/g, "at byte N");

function normalize(value, ctx) {
  if (typeof value === "string") return ids(value, ctx);
  if (Array.isArray(value)) return value.map((item) => normalize(item, ctx));
  if (value && typeof value === "object" && !Buffer.isBuffer(value)) {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, TIMES.has(key) && typeof item === "number" ? "<n>" : normalize(item, ctx)]));
  }
  return value;
}

// The command is one process per call, so an earlier call of it leaves the lease of a process
// that has ended, and `doctor` reports it as a warning ("Store x holds a stale lease of process N,
// which ended"); the front ends that live in one process never see one. That is a property of the
// front end, not of the contract, so exactly that warning is set aside; any other finding is compared.
const STALE_LEASE = /^Store \S+ holds a stale lease of process \d+, which ended$/;
const withoutStaleLeases = (value) =>
  value && Array.isArray(value.findings)
    ? {
        ...value,
        findings: value.findings.filter(
          (finding) => !(finding.severity === "warning" && finding.entity?.kind === "store" && STALE_LEASE.test(finding.message)),
        ),
      }
    : value;

const outcome = (result, ctx) => {
  if (result.ok !== undefined) return { ok: normalize(withoutStaleLeases(result.ok), ctx) };
  if (result.refused) return { refused: normalize(result.refused, ctx) };
  return result;
};
const shown = (value) => JSON.stringify(value, (_k, v) => (typeof v === "bigint" ? `${v}n` : v)).slice(0, 700);

// --- running every available front end once
const fronts = ["node", "python", "aio", "cli"];
const homes = {};
const results = {};
const available = fronts.filter((front) => !missing[front] || front === "node");
for (const front of available) {
  const ctx = { front, home: temp(front), folder: temp(`${front}-folder`) };
  writeFileSync(join(ctx.folder, "README.md"), "hello\nsecond\n");
  homes[front] = ctx;
  results[front] = front === "node" ? await runNode(ctx) : front === "cli" ? runCli(ctx) : runPython(ctx, front === "aio");
}

const index = new Map(steps.map((step, i) => [step.id, i]));
assert.equal(index.size, steps.length, "step ids are unique");

/** What the step's answer in `front` must be, by the step; `null` when it is fine. */
function expectation(step, front, result) {
  const want = step.differs?.[front] ?? (front === "cli" && step.cli === "usage" ? "usage" : step.expect);
  if (want === "ok") return result.ok !== undefined ? null : `want a value, got ${shown(result)}`;
  if (want === "usage") return result.usage !== undefined ? null : `want exit 2 (a wrong command line), got ${shown(result)}`;
  if (result.refused === undefined) return `want ${want}, got ${shown(result)}`;
  const keys = Object.keys(result.refused).sort().join();
  if (keys !== "code,detail,message") return `refusal keys are ${keys}`;
  return result.refused.code === want ? null : `want ${want}, got ${shown(result)}`;
}

function failures(group) {
  const bad = [];
  for (const step of steps.filter((s) => s.group === group)) {
    const at = index.get(step.id);
    const seen = {};
    for (const front of available) {
      const result = results[front][at];
      if (result.skipped) continue;
      if (result.crash) {
        bad.push(`${step.id} [${front}]: crashed: ${result.crash}`);
        continue;
      }
      const normal = outcome(result, homes[front]);
      seen[front] = normal;
      const wrong = expectation(step, front, normal);
      if (wrong) bad.push(`${step.id} [${front}]: ${wrong}`);
      try {
        step.check?.(normal.ok ?? normal.refused ?? normal);
      } catch (error) {
        bad.push(`${step.id} [${front}]: check: ${error.message.split("\n")[0]}`);
      }
      if (step.sameAs) {
        const before = outcome(results[front][index.get(step.sameAs)], homes[front]);
        if (!isDeepEqual(normal, before)) bad.push(`${step.id} [${front}]: differs from "${step.sameAs}": ${shown(normal)} vs ${shown(before)}`);
      }
    }
    if (step.differs || !seen.node) continue;
    for (const front of available.filter((f) => f !== "node" && seen[f])) {
      const mine = seen[front];
      const node = seen.node;
      if (mine.usage !== undefined) continue; // the command's wrong command line: checked by `expectation`
      const same = node.refused && step.loose
        ? mine.refused?.code === node.refused.code
        : isDeepEqual(mine, node);
      if (!same) bad.push(`${step.id}: ${front} differs from node: ${shown(mine)} vs ${shown(node)}`);
    }
  }
  return bad;
}

function isDeepEqual(a, b) {
  try {
    assert.deepStrictEqual(a, b);
    return true;
  } catch {
    return false;
  }
}

for (const group of [...new Set(steps.map((s) => s.group))]) {
  test(`contract ${group}: ${steps.filter((s) => s.group === group).length} cases agree across ${available.join(", ")}`, () => {
    assert.deepEqual(failures(group), []);
  });
}

for (const front of fronts) {
  if (missing[front]) test(`contract: ${front} agrees with node`, { skip: missing[front] }, () => {});
}

// LAYERFS_CONTRACT_REPORT=<file> writes each step's normalized answer per front end, for the record
if (process.env.LAYERFS_CONTRACT_REPORT) {
  const report = steps.map((step, i) => ({
    step: step.id,
    ...Object.fromEntries(available.map((front) => [front, results[front][i].skipped ? "skipped" : outcome(results[front][i], homes[front])])),
  }));
  writeFileSync(process.env.LAYERFS_CONTRACT_REPORT, JSON.stringify(report, (_k, v) => (typeof v === "bigint" ? `${v}n` : v), 1));
}

test("contract: result names are snake_case", () => {
  const odd = [];
  const walk = (value, path) => {
    if (Array.isArray(value)) value.forEach((item, i) => walk(item, `${path}[${i}]`));
    else if (value && typeof value === "object" && !Buffer.isBuffer(value)) {
      for (const [key, item] of Object.entries(value)) {
        if (!/^[a-z][a-z0-9_]*$/.test(key)) odd.push(`${path}.${key}`);
        walk(item, `${path}.${key}`);
      }
    }
  };
  steps.forEach((step, i) => results.node[i].ok !== undefined && walk(results.node[i].ok, step.id));
  assert.deepEqual(odd, []);
});

test("contract: the command's --text is a readable result, not JSON", { skip: missing.cli ?? false }, () => {
  const run = spawnSync(command, ["store", "list", "--text"], { env: { ...process.env, LAYERFS_HOME: homes.cli.home }, encoding: "utf8" });
  assert.equal(run.status, 0, run.stderr);
  assert.match(run.stdout, /shop/);
  assert.throws(() => JSON.parse(run.stdout));
});

test("contract: the command reads content from --from and a text option from --NAME-from", { skip: missing.cli ?? false }, () => {
  const env = { ...process.env, LAYERFS_HOME: homes.cli.home };
  const content = join(homes.cli.folder, "payload.bin");
  const message = join(homes.cli.folder, "message.txt");
  writeFileSync(content, Buffer.from([0, 255, 1]));
  writeFileSync(message, "from a file ☃");
  const words = ["file", "write", P, "agent", "/from.bin", "--from", content, "--message-from", message];
  const run = spawnSync(command, words, { env, encoding: "utf8" });
  assert.equal(run.status, 0, run.stderr);
  assert.equal(JSON.parse(run.stdout).commit.message, "from a file ☃");
  const stat = spawnSync(command, ["file", "stat", P, "agent", "/from.bin"], { env, encoding: "utf8" });
  assert.equal(JSON.parse(stat.stdout).size, 3);
  // no such file is a wrong command line, before any call
  const none = spawnSync(command, ["file", "write", P, "agent", "/never.bin", "--from", join(homes.cli.folder, "nope")], { env, encoding: "utf8" });
  assert.equal(none.status, 2);
  assert.equal(spawnSync(command, ["file", "stat", P, "agent", "/never.bin"], { env }).status, 125);
});

test("contract: Unknown and Unresolved outcomes are NOT RUN", { skip: "no real operation without a daemon, a fault or a second process produces one; see docs/architecture/api/calls/coverage.md" }, () => {});

test("contract: homes are removed", () => {
  for (const ctx of Object.values(homes)) {
    rmSync(ctx.home, { recursive: true, force: true });
    rmSync(ctx.folder, { recursive: true, force: true });
  }
});
