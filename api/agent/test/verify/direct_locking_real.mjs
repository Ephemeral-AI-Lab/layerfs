// Locking (#352 F-020 step 5.1): the in-process SDK and two daemons, each in
// its own Sandbox on one Store volume, publish to one Store together. The
// two daemons are the project's Sandboxes 1 and 2, each from sandbox.create,
// and each Workspace is mounted in the one it names. Every
// publish is applied once or refused with its exact outcome, reads during
// publication succeed, `PRAGMA integrity_check` passes, and every Branch's
// final tree equals a serial oracle built from the outcomes.
//
//   node api/agent/test/verify/direct_locking_real.mjs outer <daemon> <addon> <image digest>
//
// One attempt per call. The publishes start together (one Promise.all after
// every Workspace is prepared), except daemonA.shared, released after
// direct.same settles so the same-path pair is ordered (F-055); reads loop
// until they settle.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { docker, inRuntime, STORE_PATH } from "./container.mjs";

const PUBLISHED = new Set(["Committed", "MergeCommitted", "Merged"]);

async function inner() {
  const { LayerFS } = await import("@layerfs/agent");
  const work = mkdtempSync(join(tmpdir(), "layerfs-lock-"));
  const source = join(work, "site");
  mkdirSync(source);
  writeFileSync(join(source, "README.md"), "base\n");
  process.env.LAYERFS_HOME = join(work, "home");
  let lf;
  const report = { outcomes: {}, reads: { ok: 0, refused: [], maxMs: 0 } };
  try {
    lf = await LayerFS.init("site", source);
    for (const b of ["shared", "lane-a", "lane-b", "lane-c", "m1", "m2"]) await lf.branch.fork(b, "main");
    await lf.fs.write("m1", "/m1.txt", "m1\n", "m1");
    await lf.fs.write("m2", "/m2.txt", "m2\n", "m2");
    // Workspaces, written but not committed.
    const prepared = async (sandbox, branch, files) => {
      const ws = await lf.workspace.mount(sandbox, branch);
      const command = Object.entries(files).map(([f, t]) => `printf '%s\\n' '${t}' > '${f.slice(1)}'`).join(" && ");
      const ran = await lf.workspace.exec(ws, command);
      assert.equal(ran.exitCode, 0, JSON.stringify(ran));
      return ws;
    };
    const [a, b] = [(await lf.sandbox.create()).sandbox, (await lf.sandbox.create()).sandbox];
    assert.deepEqual([a, b], ["1", "2"], "two daemons: Sandboxes 1 and 2");
    const wsA1 = await prepared(a, "shared", { "/a.txt": "a", "/same.txt": "A" });
    const wsA2 = await prepared(a, "lane-a", { "/la.txt": "la" });
    const wsB1 = await prepared(b, "shared", { "/b.txt": "b" });
    const wsB2 = await prepared(b, "lane-b", { "/lb.txt": "lb" });
    assert.deepEqual([wsA1, wsA2, wsB1, wsB2].map((ws) => ws.sandbox), ["1", "1", "2", "2"]);
    const before = {};
    for (const b of ["shared", "lane-a", "lane-b", "lane-c"]) before[b] = (await lf.branch.list_commits(b, 1000)).items.length;

    // daemonA.shared's Commit is released once direct.same has settled
    // (F-055). wsA1 was mounted in setup, so both change /same.txt from bases
    // without it: direct.same binds and publishes first, and A's Commit meets
    // the moved head and is refused Conflict. Unordered, direct.same could
    // bind after A published, and both would Commit legitimately.
    let sameSettled;
    const sameDone = new Promise((resolve) => { sameSettled = resolve; });
    // [name, branch, files it adds, the call]
    const ops = [
      ["daemonA.shared", "shared", { "/a.txt": "a", "/same.txt": "A" }, () => sameDone.then(() => lf.workspace.commit(wsA1, "A shared"))],
      ["daemonB.shared", "shared", { "/b.txt": "b" }, () => lf.workspace.commit(wsB1, "B shared")],
      ["daemonA.lane-a", "lane-a", { "/la.txt": "la" }, () => lf.workspace.commit(wsA2, "A lane")],
      ["daemonB.lane-b", "lane-b", { "/lb.txt": "lb" }, () => lf.workspace.commit(wsB2, "B lane")],
      ["direct.shared", "shared", { "/d.txt": "d" }, () => lf.fs.write("shared", "/d.txt", "d\n", "direct")],
      ["direct.same", "shared", { "/same.txt": "D" }, () => lf.fs.write("shared", "/same.txt", "D\n", "direct same").finally(sameSettled)],
      ["direct.lane-c", "lane-c", { "/lc.txt": "lc" }, () => lf.fs.write("lane-c", "/lc.txt", "lc\n", "direct lane")],
      ["direct.merge", "merged", { "/m1.txt": "m1", "/m2.txt": "m2" }, () => lf.branch.merge("m1", "m2", "merged", "merge")],
    ];
    let settled = false;
    const reader = (async () => {
      while (!settled) {
        for (const [b, p] of [["main", "/README.md"], ["shared", "/README.md"]]) {
          const t = Date.now();
          try {
            await lf.fs.read(b, p);
            report.reads.ok += 1;
          } catch (e) {
            report.reads.refused.push(e.code);
          }
          report.reads.maxMs = Math.max(report.reads.maxMs, Date.now() - t);
        }
      }
    })();
    const results = await Promise.allSettled(ops.map(([, , , call]) => call()));
    settled = true;
    await reader;
    ops.forEach(([name], i) => {
      const r = results[i];
      report.outcomes[name] = r.status === "fulfilled" ? { kind: r.value?.kind } : { refused: r.reason?.code, message: String(r.reason?.message).slice(0, 160) };
    });
    for (const ws of [wsA1, wsB1, wsA2, wsB2]) await lf.workspace.unmount(ws, true);

    // Serial oracle: the base, then each published op's files in any order
    // (paths are disjoint but /same.txt, which only its one winner may hold).
    const applied = (name) => PUBLISHED.has(report.outcomes[name].kind);
    const expected = { shared: { "/README.md": "base" }, "lane-a": { "/README.md": "base" }, "lane-b": { "/README.md": "base" }, "lane-c": { "/README.md": "base" }, merged: { "/README.md": "base" } };
    for (const [name, branch, files] of ops) if (applied(name)) Object.assign(expected[branch], files);
    // The ordered pair: direct.same publishes, A's Commit is refused Conflict.
    assert.deepEqual(
      [report.outcomes["direct.same"].kind, report.outcomes["daemonA.shared"].refused],
      ["Committed", "Conflict"],
      `ordered same-path writers: ${JSON.stringify(report.outcomes)}`,
    );
    const sameWinners = ["daemonA.shared", "direct.same"].filter(applied);
    assert.ok(sameWinners.length <= 1, `both writers of /same.txt published: ${JSON.stringify(report.outcomes)}`);
    if (sameWinners.length === 1) expected.shared["/same.txt"] = sameWinners[0] === "direct.same" ? "D" : "A";
    for (const [branch, files] of Object.entries(expected)) {
      const listed = (await lf.fs.list(branch, "/", 1000)).items.map((i) => i.path ?? `/${i.name}`).sort();
      assert.deepEqual(listed, Object.keys(files).sort(), `${branch} paths`);
      for (const [path, text] of Object.entries(files)) {
        const read = await lf.fs.read(branch, path);
        const got = (read.lines ?? []).map((l) => l.text).join("\n") || read.text;
        assert.equal(got.trim(), text, `${branch}${path}`);
      }
    }
    // Applied once: each Branch gained exactly one Commit per published op.
    for (const b of ["shared", "lane-a", "lane-b", "lane-c"]) {
      const gained = (await lf.branch.list_commits(b, 1000)).items.length - before[b];
      const published = ops.filter(([name, branch]) => branch === b && applied(name)).length;
      assert.equal(gained, published, `${b}: ${gained} new Commits for ${published} published calls`);
    }
    assert.deepEqual(report.reads.refused, [], "reads during publication");
    assert.ok(report.reads.ok > 0, "reads ran during publication");
  } finally {
    await lf.close?.().catch(() => {});
    rmSync(work, { recursive: true, force: true });
  }
  // Every opener is closed: the Store's own check.
  const { DatabaseSync } = await import("node:sqlite");
  const db = new DatabaseSync(STORE_PATH, { readOnly: true });
  const integrity = db.prepare("PRAGMA integrity_check").all().map((r) => Object.values(r)[0]);
  db.close();
  report.integrity = integrity;
  assert.deepEqual(integrity, ["ok"]);
  console.log(`LOCKING ${JSON.stringify(report)}`);
}

function outer([daemon, addon, image]) {
  assert.ok(daemon && addon && image, "usage: outer <daemon> <addon> <image digest>");
  const stamp = `f020-lock-${Date.now().toString(36)}`;
  const volume = `layerfs-issue352-${stamp}`;
  execFileSync("docker", ["volume", "create", volume], { stdio: "ignore" });
  let run;
  try {
    run = inRuntime({ script: fileURLToPath(import.meta.url), daemon, addon, image, volume, prefix: `${stamp}-`, timeout: 110_000 });
  } finally {
    docker(["volume", "rm", volume]);
  }
  console.log(run.out.split("\n").slice(-12).join("\n"));
  assert.equal(run.status, 0, `inner run failed (signal ${run.signal})`);
  console.log("direct locking: ok");
}

const [mode, ...rest] = process.argv.slice(2);
if (mode === "inner") await inner();
else outer(rest);
