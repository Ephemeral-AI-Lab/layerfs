// One cell on one project, as a LayerFS agent process (#527, #516 class B).
// Settings come only from the environment, like any agent's; the SDK is the
// pinned bundle's (`LFS527_AGENT`, its api/agent/src/index.js).
//   node cell.mjs <cell> <project>
// Protocol, all on one Branch `lane`, forked untimed from `main` (Init
// publishes the source as main's Layer and makes no Branch; a Workspace mounts
// a Branch), and in one Sandbox, created untimed next and reused by every phase:
//   setup   (C09-C11) mount, exec the setup body, Commit, unmount;
//   warm-up mount, exec the timed line, unmount with discard (no Commit);
//   timed   [host lock] mount, exec, Commit, unmount (no discard) [unlock];
//   verify  fresh mount, exec the Sandbox identity and the verifier, unmount with discard;
//   close   the Sandbox.
// #529: the Sandbox is made by sandbox.create({ platform: LFS529_PLATFORM }),
// and its create and mount answers must name that platform. With
// LFS529_EXTRAS (C01 only), after the cell: `default`, a Sandbox made with no
// platform must be the home's default (LFS529_DEFAULT), then closed; and
// `refusals`, every platform outside LFS529_ALLOWED is Unsupported and
// windows, native, namespaces and an unknown name InvalidInput, with the
// project's Sandboxes the same before and after. Neither changes the cell's verdict.
// The lock is asked of the parent: this process prints `LFS527-LOCK acquire`
// and waits for one stdin line `LFS527-LOCK granted <json>`, then prints
// `LFS527-LOCK release`. The last stdout line is `LFS527-RESULT <json>`.
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { CF, IDENTITY, command, parseIdentity, parseTimed, timedLine, verifier } from "./workloads.mjs";

const [cell, project] = process.argv.slice(2);
const BRANCH = "lane";
const out = { cell, project, branch: BRANCH, ok: false, cause: null, phases: {}, steps: [] };
const now = () => performance.now();
const round = (ms) => Math.round(ms * 1000) / 1000;
const errorOf = (error) => ({ code: error?.code ?? null, message: String(error?.message ?? error).slice(0, 2000), detail: error?.detail ?? null });

const lines = createInterface({ input: process.stdin });
const pending = [];
lines.on("line", (line) => pending.shift()?.(line));
const nextLine = () => new Promise((resolve) => pending.push(resolve));

/** One SDK call, its wall time and what it answered or refused, recorded under `name`. */
async function step(name, call) {
  const at = now();
  try {
    const value = await call();
    out.steps.push({ name, ms: round(now() - at), ok: true });
    return { value, ms: round(now() - at) };
  } catch (error) {
    out.steps.push({ name, ms: round(now() - at), ok: false, error: errorOf(error) });
    const failed = new Error(`${name}: ${error?.code ?? ""} ${error?.message ?? error}`.trim());
    failed.step = name;
    throw failed;
  }
}

const exec = (layerfs, ws, text, label) => step(label, () => layerfs.workspace.exec(ws, text, `lfs527 ${cell} ${label}`));
const execSummary = ({ value, ms }) => ({ wallMs: ms, exitCode: value.exitCode, stdout: value.stdout?.slice(0, 2000), stderr: value.stderr?.slice(0, 2000), ...parseTimed(value.stdout) });

const PLATFORM = process.env.LFS529_PLATFORM;
if (!CF[cell] || !project || !process.env.LFS527_AGENT || !PLATFORM) {
  console.error("usage: LFS527_AGENT=<bundle>/api/agent/src/index.js LFS529_PLATFORM=<platform> node cell.mjs <C01..C12> <project>");
  process.exit(2);
}
const { LayerFS } = await import(pathToFileURL(process.env.LFS527_AGENT).href);
let layerfs, sandbox;
const mounted = new Set();
try {
  layerfs = (await step("open", () => LayerFS.open(project))).value;
  out.fork = (await step("branch.fork", () => layerfs.branch.fork(BRANCH, "main"))).value;
  const created = (await step("sandbox.create", () => layerfs.sandbox.create({ platform: PLATFORM }))).value;
  sandbox = created.sandbox;
  out.sandbox = created;

  const line = timedLine(command(cell));
  const mount = async (label) => {
    const result = await step(`${label}.mount`, () => layerfs.workspace.mount(sandbox, BRANCH));
    mounted.add(result.value);
    return result;
  };
  const unmount = async (label, ws, discard) => {
    const result = await step(`${label}.unmount`, () => layerfs.workspace.unmount(ws, discard));
    mounted.delete(ws);
    return result;
  };

  if (CF[cell][1]) {
    const { value: ws } = await mount("setup");
    const ran = execSummary(await exec(layerfs, ws, CF[cell][1], "setup.exec"));
    if (ran.exitCode !== 0) throw Object.assign(new Error(`setup body exited ${ran.exitCode}: ${ran.stderr}`), { step: "setup.exec" });
    const committed = await step("setup.commit", () => layerfs.workspace.commit(ws, `lfs527 ${cell} setup`));
    const left = await unmount("setup", ws, false);
    out.phases.setup = { exec: ran, commitKind: committed.value?.kind, commit: committed.value, unmount: left.value };
  }

  {
    const mountAt = await mount("warmup");
    const ran = execSummary(await exec(layerfs, mountAt.value, line, "warmup.exec"));
    // a warm-up's changes are never committed: discard drops them with the mount
    const left = await unmount("warmup", mountAt.value, true);
    out.phases.warmup = { mountMs: mountAt.ms, exec: ran, unmountMs: left.ms, unmount: left.value };
  }

  // the timed phase, under the host-wide lock
  const askedAt = now();
  console.log("LFS527-LOCK acquire");
  const granted = await nextLine();
  if (!granted.startsWith("LFS527-LOCK granted")) throw Object.assign(new Error(`lock not granted: ${granted}`), { step: "lock" });
  out.lock = { askedMs: round(now() - askedAt), ...JSON.parse(granted.slice("LFS527-LOCK granted".length).trim() || "{}") };
  try {
    const mountAt = await mount("timed");
    const ws = mountAt.value;
    const ran = execSummary(await exec(layerfs, ws, line, "timed.exec"));
    const committed = await step("timed.commit", () => layerfs.workspace.commit(ws, `lfs527 ${cell}`));
    const left = await unmount("timed", ws, false);
    out.phases.timed = {
      mountMs: mountAt.ms, image: ws.image ?? null, platform: ws.platform ?? null, workspace: { sandbox: ws.sandbox, path: ws.path },
      exec: ran, commitMs: committed.ms, commitKind: committed.value?.kind ?? null, commit: committed.value,
      unmountMs: left.ms, unmount: left.value,
    };
  } finally {
    console.log("LFS527-LOCK release");
  }

  {
    const { value: ws } = await mount("verify");
    const identity = await exec(layerfs, ws, IDENTITY, "verify.identity");
    out.where = parseIdentity(identity.value.stdout);
    const checked = await exec(layerfs, ws, verifier(cell), "verify.exec");
    await unmount("verify", ws, true);
    out.phases.verify = { exitCode: checked.value.exitCode, stdout: checked.value.stdout.slice(0, 2000), stderr: checked.value.stderr.slice(0, 2000) };
  }

  const timed = out.phases.timed;
  const problems = [];
  if (timed.exec.exitCode !== 0) problems.push(`exec exit ${timed.exec.exitCode}`);
  if (timed.exec.rc !== 0) problems.push(`body exit ${timed.exec.rc}`);
  if (!["Committed", "UpToDate"].includes(timed.commitKind)) problems.push(`Commit kind ${timed.commitKind}`);
  if (created.platform !== PLATFORM) problems.push(`sandbox.create answered platform ${created.platform}, not ${PLATFORM}`);
  if (timed.platform !== PLATFORM) problems.push(`workspace.mount answered platform ${timed.platform}, not ${PLATFORM}`);
  if (out.phases.verify.exitCode !== 0 || !/^VERIFY OK$/m.test(out.phases.verify.stdout)) problems.push(`verifier: ${out.phases.verify.stdout.trim()} ${out.phases.verify.stderr.trim()}`.trim());
  out.ok = problems.length === 0;
  out.cause = problems.join("; ") || null;
} catch (error) {
  out.ok = false;
  out.cause = error.message;
  out.failedStep = error.step ?? null;
} finally {
  // whatever happened: force-unmount what is still mounted, then close the Sandbox
  for (const ws of mounted) {
    await layerfs.workspace.unmount(ws, true, true).then(
      () => out.steps.push({ name: "cleanup.unmount", ok: true }),
      (error) => out.steps.push({ name: "cleanup.unmount", ok: false, error: errorOf(error) }),
    );
  }
  if (sandbox) {
    // not part of the verdict (PASS = verifier, Commit, clean unmount); the driver's teardown reports leftovers
    await step("sandbox.close", () => layerfs.sandbox.close(sandbox)).catch((error) => { out.closeError = error.message; });
  }
  if (process.env.LFS529_EXTRAS && layerfs) out.extras = await extras(layerfs).catch((error) => ({ ok: false, error: errorOf(error) }));
  await layerfs?.close().catch(() => {});
  lines.close();
}

/** #529's default and refusal checks on this home (see the head comment). */
async function extras(layerfs) {
  const names = ["docker", "mac", "linux", "apple-container", "microsandbox", "lima", "firecracker", "ssh"];
  const allowed = process.env.LFS529_ALLOWED.split(",");
  const listed = async () => (await layerfs.project.list_sandboxes(1000, 0)).items.map((s) => `${s.sandbox}:${s.platform}`).sort();
  const result = { ok: false, default: {}, refusals: [] };
  const made = (await step("extras.default.create", () => layerfs.sandbox.create())).value;
  result.default = { expected: process.env.LFS529_DEFAULT, made };
  await step("extras.default.close", () => layerfs.sandbox.close(made.sandbox));
  const before = await listed();
  const tries = [...names.filter((name) => !allowed.includes(name)).map((name) => [name, "Unsupported"]),
    ...["windows", "native", "namespaces", "podman"].map((name) => [name, "InvalidInput"])];
  for (const [platform, expected] of tries) {
    const at = now();
    try {
      const value = await layerfs.sandbox.create({ platform });
      result.refusals.push({ platform, expected, code: null, made: value });
      await layerfs.sandbox.close(value.sandbox).catch(() => {});
    } catch (error) {
      result.refusals.push({ platform, expected, code: error.code ?? null, message: String(error.message).slice(0, 300), ms: round(now() - at) });
    }
  }
  result.before = before;
  result.after = await listed();
  result.ok = made.platform === result.default.expected && result.refusals.every((r) => r.code === r.expected) && JSON.stringify(before) === JSON.stringify(result.after);
  return result;
}
console.log(`LFS527-RESULT ${JSON.stringify(out)}`);
process.exit(0);
