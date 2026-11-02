// One #516 cell through the 2.0 `layerfs` package (#533 step 19), ported from
// 1.1.0's api/runtime/test/verify/cells/cell.mjs (#527/#529). The workload
// bodies and verifiers (workloads.mjs) are unchanged.
//   node cell.mjs <C01..C12> <machine> <receipt.json>
// In a fresh private home: store create, Init from an empty folder, a Branch
// `lane` forked from main, one Sandbox on <machine>, then
//   setup   (C09-C11) mount, exec the setup body, Commit, unmount;
//   warm-up mount, exec the timed line, unmount with discard;
//   timed   mount, exec, Commit, unmount;
//   verify  fresh mount, exec the identity and the verifier, unmount with discard;
//   close   the Sandbox, and the home removed.
// LFS533_DAEMON names the daemon for the `native` machine; it reaches the
// home's record through `layerfs import-env` (the repository's debug
// `layerfs`, or LFS533_LAYERFS), as a 1.1.0 environment would. On any other
// machine the Store moves there after Init; for `docker`, LFS533_IMAGE is
// the Sandbox image (`machine update docker --image`), and a docker worker
// this home started (`lfs-<home id>-docker`) is removed with its data
// volume at the end.
// LFS533_HOME names an existing home that already holds <machine> (a VM is
// made once per kind, not per cell): the cell's own Store `c<nn>` is made
// there, moved native -> <machine> and, after the cells, back to native, then
// removed; the home stays.
// One cell per command; nothing else runs a cell at the same time, so the
// 1.1.0 host-wide lock is not ported. The receipt holds every step's time,
// the 1-minute load and the verdict.
import { mkdtempSync, chmodSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { loadavg, tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
import { client } from "../../../src/index.js";
import { CF, IDENTITY, command, parseIdentity, parseTimed, timedLine, verifier, sha256 } from "./workloads.mjs";

const [cell, machine, receipt] = process.argv.slice(2);
if (!CF[cell] || !machine || !receipt) {
  console.error("usage: node cell.mjs <C01..C12> <machine> <receipt.json>");
  process.exit(2);
}
const now = () => performance.now();
const round = (ms) => Math.round(ms * 1000) / 1000;
const errorOf = (error) => ({ code: error?.code ?? null, message: String(error?.message ?? error).slice(0, 2000) });
// LFS533_PRODUCT names the product commit where there is no checkout (a
// Linux VM running a copy of this package).
const head = process.env.LFS533_PRODUCT ?? (() => {
  try {
    return execFileSync("git", ["rev-parse", "--short=10", "HEAD"], { encoding: "utf8" }).trim();
  } catch {
    return null;
  }
})();
const out = {
  format: "lfs533-cell/1", cell, machine, product: head, startedAt: new Date().toISOString(),
  load1: loadavg()[0], body: { sha256: sha256(CF[cell][0]) }, ok: false, cause: null, phases: {}, steps: [],
};

async function step(name, call) {
  const at = now();
  try {
    const value = await call();
    out.steps.push({ name, ms: round(now() - at), ok: true });
    return { value, ms: round(now() - at) };
  } catch (error) {
    out.steps.push({ name, ms: round(now() - at), ok: false, error: errorOf(error) });
    throw Object.assign(new Error(`${name}: ${error?.code ?? ""} ${error?.message ?? error}`.trim()), { step: name });
  }
}

const shared = process.env.LFS533_HOME;
const home = shared ?? mkdtempSync(join(tmpdir(), "lfs533-cell-"));
if (!shared) chmodSync(home, 0o700);
const empty = mkdtempSync(join(tmpdir(), "lfs533-src-"));
const lfs = client({ home });
const storeName = shared ? `c${cell.slice(1)}` : "cells";
const project = `${storeName}/p`;
let moved = false;
const name = `c${cell.slice(1)}`;
let sandbox = null;
const mounted = new Set();
const exec = (ws, text, label) => step(label, () => lfs.workspace.exec(ws, text, { message: `lfs533 ${cell} ${label}` }));
const summary = ({ value, ms }) => ({ wallMs: ms, exitCode: value.exit_code, stdout: value.stdout?.slice(0, 2000), stderr: value.stderr?.slice(0, 2000), ...parseTimed(value.stdout) });
const mount = async (label) => {
  const result = await step(`${label}.mount`, () => lfs.workspace.mount(sandbox, "lane"));
  mounted.add(result.value.ws);
  return result;
};
const unmount = async (label, ws, discard) => {
  const result = await step(`${label}.unmount`, () => lfs.workspace.unmount(ws, { discard }));
  mounted.delete(ws);
  return result;
};
const kindOf = (value) => value?.kind ?? value?.outcome ?? null;

try {
  if (process.env.LFS533_DAEMON) {
    const binary = process.env.LFS533_LAYERFS ?? new URL("../../../../../target/debug/layerfs", import.meta.url).pathname;
    await step("import-env", async () =>
      execFileSync(binary, ["import-env"], {
        env: { PATH: process.env.PATH, HOME: process.env.HOME, LAYERFS_HOME: home, LAYERFS_DAEMON: process.env.LFS533_DAEMON },
        encoding: "utf8",
      }));
  }
  await step("store.create", () => lfs.store.create(storeName));
  await step("project.init", () => lfs.project.init(project, empty));
  await step("branch.fork", () => lfs.branch.fork(project, "lane", "main"));
  if (machine === "docker" && process.env.LFS533_IMAGE) {
    await step("machine.update", () => lfs.machine.update("docker", { image: process.env.LFS533_IMAGE }));
  }
  if (machine !== "native") {
    await step("store.move", () => lfs.store.move(storeName, machine));
    moved = true;
  }
  out.sandbox = (await step("sandbox.create", () => lfs.sandbox.create(project, name, machine))).value;
  sandbox = name;
  const line = timedLine(command(cell));
  if (CF[cell][1]) {
    const { value } = await mount("setup");
    const ran = summary(await exec(value.ws, CF[cell][1], "setup.exec"));
    if (ran.exitCode !== 0) throw Object.assign(new Error(`setup body exited ${ran.exitCode}: ${ran.stderr}`), { step: "setup.exec" });
    const committed = await step("setup.commit", () => lfs.workspace.commit(value.ws, { message: `lfs533 ${cell} setup` }));
    await unmount("setup", value.ws, false);
    out.phases.setup = { exec: ran, commit: committed.value };
  }
  {
    const at = await mount("warmup");
    const ran = summary(await exec(at.value.ws, line, "warmup.exec"));
    const left = await unmount("warmup", at.value.ws, true);
    out.phases.warmup = { mountMs: at.ms, exec: ran, unmountMs: left.ms };
  }
  {
    const at = await mount("timed");
    const ws = at.value.ws;
    const ran = summary(await exec(ws, line, "timed.exec"));
    const committed = await step("timed.commit", () => lfs.workspace.commit(ws, { message: `lfs533 ${cell}` }));
    const left = await unmount("timed", ws, false);
    out.phases.timed = { mountMs: at.ms, workspace: at.value, exec: ran, commitMs: committed.ms, commit: committed.value, unmountMs: left.ms };
  }
  {
    const { value } = await mount("verify");
    const identity = await exec(value.ws, IDENTITY, "verify.identity");
    out.where = parseIdentity(identity.value.stdout);
    const checked = await exec(value.ws, verifier(cell), "verify.exec");
    await unmount("verify", value.ws, true);
    out.phases.verify = { exitCode: checked.value.exit_code, stdout: checked.value.stdout?.slice(0, 2000), stderr: checked.value.stderr?.slice(0, 2000) };
  }
  const timed = out.phases.timed;
  const problems = [];
  if (timed.exec.exitCode !== 0) problems.push(`exec exit ${timed.exec.exitCode}`);
  if (timed.exec.rc !== undefined && timed.exec.rc !== 0) problems.push(`body exit ${timed.exec.rc}`);
  if (!timed.commit || (!timed.commit.commit && kindOf(timed.commit) !== "UpToDate")) problems.push(`Commit answered ${JSON.stringify(timed.commit)}`);
  if (out.phases.verify.exitCode !== 0 || !/^VERIFY OK$/m.test(out.phases.verify.stdout ?? "")) {
    problems.push(`verifier: ${(out.phases.verify.stdout ?? "").trim()} ${(out.phases.verify.stderr ?? "").trim()}`.trim());
  }
  out.ok = problems.length === 0;
  out.cause = problems.join("; ") || null;
} catch (error) {
  out.ok = false;
  out.cause = error.message;
  out.failedStep = error.step ?? null;
} finally {
  for (const ws of mounted) {
    await lfs.workspace.unmount(ws, { discard: true, force: true }).then(
      () => out.steps.push({ name: "cleanup.unmount", ok: true }),
      (error) => out.steps.push({ name: "cleanup.unmount", ok: false, error: errorOf(error) }),
    );
  }
  if (sandbox) await step("sandbox.close", () => lfs.sandbox.close(sandbox)).catch((error) => { out.closeError = error.message; });
  if (shared) {
    // the Store comes back to native, then goes; the home and its machine stay
    if (moved) await step("store.move.back", () => lfs.store.move(storeName, "native")).catch((error) => { out.moveBackError = error.message; });
    await step("store.remove", () => lfs.store.remove(storeName)).catch((error) => { out.removeError = error.message; });
  }
  if (machine === "docker" && !shared) {
    // this home's own worker container and data volume, never another's
    const id = JSON.parse(readFileSync(join(home, "layerfs.json"), "utf8")).id.slice(0, 8);
    for (const args of [["rm", "-f", `lfs-${id}-docker`], ["volume", "rm", `lfs-${id}-docker-data`]]) {
      try {
        execFileSync("docker", args, { stdio: "ignore" });
      } catch {
        out.steps.push({ name: `cleanup.docker ${args.join(" ")}`, ok: false });
      }
    }
  }
  out.endedAt = new Date().toISOString();
  writeFileSync(receipt, JSON.stringify(out, null, 2) + "\n");
  if (!shared) rmSync(home, { recursive: true, force: true });
  rmSync(empty, { recursive: true, force: true });
}
if (out.moveBackError || out.removeError) {
  out.ok = false;
  out.cause = [out.cause, out.moveBackError, out.removeError].filter(Boolean).join("; ");
  writeFileSync(receipt, JSON.stringify(out, null, 2) + "\n");
}
console.log(`${cell} ${machine} ${out.ok ? "PASS" : "FAIL"} ${out.cause ?? ""} timed exec ${out.phases.timed?.exec?.wallMs ?? "-"} ms`);
process.exit(out.ok ? 0 : 1);
