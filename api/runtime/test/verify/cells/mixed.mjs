// #529 §8 mixed project, as one agent process (root, on a Linux home): a
// `docker` and a `linux` Sandbox of one project, both mounting Branch `lane`.
//   node mixed.mjs <project>
// Settings come from the environment (LFS527_AGENT is the bundle's SDK;
// LFS529_NS_IMAGE is the image LAYERFS_IMAGE_ARCHIVE holds, named by its
// config digest; LAYERFS_IMAGE is Docker's digest of it). Steps, each recorded:
//   1. create docker and linux, linux with no image: it runs the archive's own
//      (LFS529_NS_IMAGE); each answer names its platform; the listing shows both;
//   2. mount `lane` in each; while both are mounted, the Store file each
//      daemon holds open is found through /proc/<pid>/fd and compared by
//      (dev, inode) with LAYERFS_STORE_PATH: a holder in the Docker
//      container's cgroup and one in the linux Sandbox's must be the same file;
//   3. a Commit from docker is read by a fresh linux mount, and a Commit from
//      linux by a fresh docker mount;
//   4. docker is closed; linux still execs, and the listing shows it alone;
//   5. linux is closed;
//   6. the per-call image: a linux Sandbox given LFS529_NS_IMAGE runs it and
//      closes; one given Docker's digest (LAYERFS_IMAGE) is refused
//      InvalidInput with nothing made, as the archive does not hold it.
//   Whatever happens, what is still mounted or running is released. The last
//   stdout line is `LFS529-RESULT <json>`.
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, readlinkSync, statSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const [project] = process.argv.slice(2);
const out = { project, ok: false, cause: null, steps: [], checks: {} };
const errorOf = (error) => ({ code: error?.code ?? null, message: String(error?.message ?? error).slice(0, 2000) });
const now = () => performance.now();
async function step(name, call) {
  const at = now();
  try {
    const value = await call();
    out.steps.push({ name, ms: Math.round((now() - at) * 1000) / 1000, ok: true });
    return value;
  } catch (error) {
    out.steps.push({ name, ms: Math.round((now() - at) * 1000) / 1000, ok: false, error: errorOf(error) });
    throw Object.assign(new Error(`${name}: ${error?.code ?? ""} ${error?.message ?? error}`.trim()), { step: name });
  }
}
const need = (cond, what) => { if (!cond) throw Object.assign(new Error(what), { step: "check" }); };

/** Every process holding an fd to the file at (dev, ino): its pid, the (dev, ino) its fd resolves to, cgroup, command and fd target. */
function holders(dev, ino) {
  const found = [];
  for (const pid of readdirSync("/proc").filter((name) => /^\d+$/.test(name))) {
    let fds = [];
    try { fds = readdirSync(`/proc/${pid}/fd`); } catch { continue; }
    for (const fd of fds) {
      try {
        const st = statSync(`/proc/${pid}/fd/${fd}`);
        if (st.dev !== dev || st.ino !== ino) continue;
        found.push({
          pid: Number(pid), fd: Number(fd), dev: st.dev, ino: st.ino, target: readlinkSync(`/proc/${pid}/fd/${fd}`),
          cgroup: readFileSync(`/proc/${pid}/cgroup`, "utf8").trim(),
          command: readFileSync(`/proc/${pid}/cmdline`, "utf8").split("\0").filter(Boolean).slice(0, 3).join(" "),
        });
        break;
      } catch { /* gone meanwhile */ }
    }
  }
  return found;
}

const { LayerFS } = await import(pathToFileURL(process.env.LFS527_AGENT).href);
const home = process.env.LAYERFS_HOME, storePath = process.env.LAYERFS_STORE_PATH;
const mounted = new Set(), running = new Set();
let layerfs;
const exec = async (label, ws, text) => {
  const ran = await step(label, () => layerfs.workspace.exec(ws, text, `lfs529 mixed ${label}`));
  return { exitCode: ran.exitCode, stdout: ran.stdout?.slice(0, 500), stderr: ran.stderr?.slice(0, 500) };
};
const mount = async (label, sandbox) => { const ws = await step(label, () => layerfs.workspace.mount(sandbox, "lane")); mounted.add(ws); return ws; };
const unmount = async (label, ws, discard) => { await step(label, () => layerfs.workspace.unmount(ws, discard)); mounted.delete(ws); };
const listed = async () => (await layerfs.project.list_sandboxes(1000, 0)).items.map((s) => ({ sandbox: s.sandbox, platform: s.platform, image: s.image }));
try {
  layerfs = await step("open", () => LayerFS.open(project));
  await step("branch.fork", () => layerfs.branch.fork("lane", "main"));
  const docker = await step("create docker", () => layerfs.sandbox.create({ platform: "docker" }));
  running.add(docker.sandbox);
  const linux = await step("create linux (no image)", () => layerfs.sandbox.create({ platform: "linux" }));
  running.add(linux.sandbox);
  out.created = { docker, linux };
  need(docker.platform === "docker" && linux.platform === "linux", `create answered ${docker.platform} and ${linux.platform}`);
  need(linux.image === process.env.LFS529_NS_IMAGE, `linux with no image runs ${linux.image}, not the archive's ${process.env.LFS529_NS_IMAGE}`);
  need(docker.image === process.env.LAYERFS_IMAGE, `docker runs ${docker.image}, not LAYERFS_IMAGE ${process.env.LAYERFS_IMAGE}`);
  out.checks.listBoth = await listed();
  need(JSON.stringify(out.checks.listBoth.map((s) => s.platform).sort()) === JSON.stringify(["docker", "linux"]), "the listing does not show both platforms");

  const wsD = await mount("mount docker", docker.sandbox);
  const wsL = await mount("mount linux", linux.sandbox);
  need(wsD.platform === "docker" && wsL.platform === "linux", `mount answered ${wsD.platform} and ${wsL.platform}`);
  // one Store file: the host path's (dev, ino), and the holders in each Sandbox
  const st = statSync(storePath);
  const ids = Object.fromEntries([["docker", docker.sandbox], ["linux", linux.sandbox]].map(([name, n]) => [name, readFileSync(join(home, project, "sandboxes", n, "container"), "utf8").trim()]));
  const found = holders(st.dev, st.ino);
  const inDocker = found.filter((h) => h.cgroup.includes(ids.docker));
  const inLinux = found.filter((h) => h.cgroup.includes(ids.linux));
  let volume = null;
  try { volume = JSON.parse(execFileSync("docker", ["inspect", "--format", "{{json .Mounts}}", ids.docker], { encoding: "utf8", timeout: 10_000 })); } catch (error) { volume = errorOf(error); }
  const ids3 = (list) => list.map((h) => ({ pid: h.pid, dev: h.dev, ino: h.ino, target: h.target }));
  out.checks.store = {
    path: storePath, size: st.size, containerIds: ids, holders: found, dockerMounts: volume,
    // the three (dev, ino) values: the host path's, and what each Sandbox's daemon holds
    identity: { path: { dev: st.dev, ino: st.ino }, docker: ids3(inDocker), linux: ids3(inLinux) },
  };
  need(inDocker.length > 0 && inLinux.length > 0, `the Store file (dev ${st.dev}, ino ${st.ino}) is held by ${inDocker.length} process(es) of the docker Sandbox and ${inLinux.length} of the linux one`);
  out.checks.store.same = [...inDocker, ...inLinux].every((h) => h.dev === st.dev && h.ino === st.ino);
  need(out.checks.store.same, "a holder's (dev, ino) differs from the Store path's");

  // a Commit from each is seen by the other
  out.checks.writeDocker = await exec("docker write", wsD, "printf from-docker > from-docker.txt");
  out.checks.commitDocker = await step("docker commit", () => layerfs.workspace.commit(wsD, "lfs529 mixed: from docker"));
  await unmount("docker unmount", wsD, false);
  await unmount("linux unmount (earlier base)", wsL, true);
  const wsL2 = await mount("mount linux again", linux.sandbox);
  out.checks.linuxReads = await exec("linux reads", wsL2, "cat from-docker.txt && printf from-linux > from-linux.txt");
  need(out.checks.linuxReads.exitCode === 0 && out.checks.linuxReads.stdout === "from-docker", "linux did not read docker's Commit");
  out.checks.commitLinux = await step("linux commit", () => layerfs.workspace.commit(wsL2, "lfs529 mixed: from linux"));
  await unmount("linux unmount", wsL2, false);
  const wsD2 = await mount("mount docker again", docker.sandbox);
  out.checks.dockerReads = await exec("docker reads", wsD2, "cat from-docker.txt from-linux.txt");
  need(out.checks.dockerReads.exitCode === 0 && out.checks.dockerReads.stdout === "from-dockerfrom-linux", "docker did not read linux's Commit");
  await unmount("docker unmount again", wsD2, true);

  // closing one leaves the other
  await step("close docker", () => layerfs.sandbox.close(docker.sandbox));
  running.delete(docker.sandbox);
  out.checks.listAfterClose = await listed();
  need(JSON.stringify(out.checks.listAfterClose.map((s) => [s.sandbox, s.platform])) === JSON.stringify([[linux.sandbox, "linux"]]), "the listing after closing docker is not linux alone");
  const wsL3 = await mount("mount linux after close", linux.sandbox);
  out.checks.linuxAfterClose = await exec("linux after close", wsL3, "cat from-linux.txt");
  need(out.checks.linuxAfterClose.stdout === "from-linux", "linux does not work after docker closed");
  await unmount("linux unmount after close", wsL3, true);
  await step("close linux", () => layerfs.sandbox.close(linux.sandbox));
  running.delete(linux.sandbox);

  // the per-call image: given, it wins; Docker's digest is not the archive's, so it is refused before any effect
  const given = await step("create linux (image given)", () => layerfs.sandbox.create({ platform: "linux", image: process.env.LFS529_NS_IMAGE }));
  running.add(given.sandbox);
  out.created.given = given;
  need(given.platform === "linux" && given.image === process.env.LFS529_NS_IMAGE, `linux given ${process.env.LFS529_NS_IMAGE} answered ${given.platform} ${given.image}`);
  out.checks.listGiven = await listed();
  await step("close linux (image given)", () => layerfs.sandbox.close(given.sandbox));
  running.delete(given.sandbox);
  const before = await listed();
  let mismatch = null;
  try {
    const made = await layerfs.sandbox.create({ platform: "linux", image: process.env.LAYERFS_IMAGE });
    running.add(made.sandbox);
    mismatch = { made };
  } catch (error) {
    mismatch = { refused: errorOf(error) };
  }
  out.checks.dockerDigestForLinux = { image: process.env.LAYERFS_IMAGE, ...mismatch, listBefore: before, listAfter: await listed() };
  need(mismatch.refused?.code === "InvalidInput", `linux given Docker's digest was not refused InvalidInput: ${JSON.stringify(mismatch)}`);
  need(JSON.stringify(before) === JSON.stringify(out.checks.dockerDigestForLinux.listAfter), "the refused create changed the listing");
  out.ok = true;
} catch (error) {
  out.cause = error.message;
  out.failedStep = error.step ?? null;
} finally {
  for (const ws of mounted) await layerfs.workspace.unmount(ws, true, true).catch((error) => out.steps.push({ name: "cleanup.unmount", ok: false, error: errorOf(error) }));
  for (const sandbox of running) await layerfs.sandbox.close(sandbox).catch((error) => out.steps.push({ name: `cleanup.close ${sandbox}`, ok: false, error: errorOf(error) }));
  await layerfs?.close().catch(() => {});
}
console.log(`LFS529-RESULT ${JSON.stringify(out)}`);
process.exit(0);
