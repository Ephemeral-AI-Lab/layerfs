// End-to-end check of LayerFS × Pi: a fresh LayerFS project, a Pi session
// started the way `pi --layerfs <project>` starts one, and its tools called
// as the model calls them (no model needed). Another SDK client plays a
// second writer and checks what the Branch and main hold. It runs on the
// LAYERFS_* deployment with a fresh volume (on macOS, through the ported SDK
// and its Store container, as the extension does), and imports the Pi
// installed globally with npm. The session's model is Pi's faux provider,
// which replays scripted assistant messages through the real agent loop.
// Pi 1.3.0 does not export it from @earendil-works/pi-coding-agent; it is in
// @earendil-works/pi-ai, a dependency of Pi pinned to Pi's version, so the
// test loads the copy Pi itself resolves: a faux provider registered in
// another copy would not be in Pi's provider registry.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, test } from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

const PORTED = process.platform === "darwin";
const here = (path) => fileURLToPath(new URL(path, import.meta.url));
// macOS: Init's container mounts the source folder, so it is in the home, which Docker's VM shares
const work = mkdtempSync(join(PORTED ? homedir() : tmpdir(), "pi-layerfs-e2e-"));

const piRoot = join(execFileSync("npm", ["root", "-g"], { encoding: "utf8" }).trim(), "@earendil-works/pi-coding-agent");
const piEntry = join(piRoot, "dist/index.js");
const pi = await import(pathToFileURL(piEntry).href);
const piAi = createRequire(piEntry).resolve.paths("@earendil-works/pi-ai").map((dir) => join(dir, "@earendil-works/pi-ai")).find(existsSync);
const { fauxAssistantMessage, fauxToolCall, registerFauxProvider } = await import(pathToFileURL(join(piAi, "dist/compat.js")).href);
const { LayerFS } = await import(PORTED ? "../../port/src/sdk.js" : "../../sdk/js/src/index.js");
const { LayerFSOperator } = await import(PORTED ? "../../port/src/operator.js" : "../../sdk/js/operator/index.js");

const project = `pi-e2e-${Date.now().toString(36)}`;
const cwd = join(work, "cwd");
const other = new LayerFS(project);
const faux = registerFauxProvider();
let runtime, branch;

const call = (name, args, ctx) => runtime.session.getToolDefinition(name).execute(`call-${name}`, args, undefined, undefined, ctx);
const output = (result) => result.content.map((c) => c.text).join("");
const commits = async () => (await other.branch.list_commits(branch, 1000, 0)).items.length;
const fileOn = async (ref, path) => (await other.fs.read(ref, path)).text;

before(async () => {
  const src = join(work, "src");
  mkdirSync(join(src, "src"), { recursive: true });
  writeFileSync(join(src, "README.md"), "Hello from main\n");
  writeFileSync(join(src, "src", "app.js"), "export const a = 1;\nexport const b = 2;\n");
  mkdirSync(cwd);
  if (PORTED) await (await import("../../port/src/index.js")).useStoreContainer((error) => console.error(error));
  await other.project.init(src);

  const agentDir = join(work, "agent");
  const model = faux.getModel();
  mkdirSync(agentDir);
  writeFileSync(join(agentDir, "models.json"), JSON.stringify({
    providers: { [model.provider]: { baseUrl: model.baseUrl, api: model.api, apiKey: "faux", models: [{ id: model.id, name: model.name }] } },
  }));
  let errors;
  ({ runtime, errors } = await startSession(project));
  assert.deepEqual(errors, []);
});

/** A Pi session with `--layerfs name` in `cwd`, and the errors its extensions reported. */
async function startSession(name) {
  const agentDir = join(work, "agent"), model = faux.getModel();
  const started = await pi.createAgentSessionRuntime(async ({ cwd: at, sessionManager, sessionStartEvent }) => {
    const services = await pi.createAgentSessionServices({
      cwd: at,
      agentDir,
      extensionFlagValues: new Map([["layerfs", name]]),
      resourceLoaderOptions: { additionalExtensionPaths: [here("../src/pi-layerfs/index.js")] },
    });
    const created = await pi.createAgentSessionFromServices({ services, sessionManager, sessionStartEvent, model });
    return { ...created, services, diagnostics: services.diagnostics };
  }, { cwd, agentDir, sessionManager: pi.SessionManager.inMemory(cwd) });
  assert.deepEqual(started.diagnostics.filter((d) => d.type === "error"), []);
  const errors = [];
  await started.session.bindExtensions({ onError: (error) => errors.push(error) });
  return { runtime: started, errors };
}

after(async () => {
  await runtime?.dispose();
  faux.unregister();
  await other.close();
  rmSync(work, { recursive: true, force: true });
});

test("the session forks its own Branch and reads main's files", async () => {
  const names = (await other.project.list_branches()).items.map((b) => b.name).filter((name) => name !== "main");
  assert.equal(names.length, 1);
  [branch] = names;
  assert.match(branch, /^pi-/);
  assert.match(output(await call("read", { path: "README.md" })), /^Hello from main/);
});

test("write makes its folder in one Commit; edit replaces exactly and keeps another writer's change", async () => {
  let before = await commits();
  await call("write", { path: "notes/todo.txt", content: "- buy milk\n" });
  assert.equal(await fileOn(branch, "/notes/todo.txt"), "- buy milk\n");
  assert.equal(await commits(), before + 1);

  assert.match(output(await call("read", { path: "src/app.js" })), /a = 1;\nexport const b = 2;/);
  await other.fs.edit(branch, "/src/app.js", "export const b = 2;", "export const b = 20;", "another writer");
  before = await commits();
  const edited = await call("edit", { path: "src/app.js", edits: [{ oldText: "export const a = 1;", newText: "export const a = 10;" }] });
  assert.equal(await fileOn(branch, "/src/app.js"), "export const a = 10;\nexport const b = 20;\n");
  assert.equal(await commits(), before + 1);
  assert.match(edited.details.diff, /\+\s*1 export const a = 10;/);
});

test("ls, find and grep see committed files; a path outside the working directory is refused", async () => {
  assert.deepEqual(output(await call("ls", {})).split("\n"), ["README.md", "notes/", "src/"]);
  assert.equal(output(await call("find", { pattern: "*.txt" })), "notes/todo.txt");
  assert.equal(output(await call("grep", { pattern: "const b" })), "src/app.js:2: export const b = 20;");
  await assert.rejects(call("read", { path: "../outside.txt" }), /outside .*Branch root/);
});

test("bash runs in the working directory's folder, commits each call, failures too, and leaves nothing mounted", async () => {
  let before = await commits();
  const built = await call("bash", { command: "echo built > out.txt && ls" }, { cwd: join(cwd, "src") });
  assert.match(output(built), /app\.js/);
  assert.equal(await fileOn(branch, "/src/out.txt"), "built\n");
  assert.equal(await commits(), before + 1);

  before = await commits();
  const failed = await call("bash", { command: "echo partial > failed.txt; exit 3" });
  assert.equal(failed.isError, true);
  assert.match(output(failed), /Command exited with code 3/);
  assert.equal(await fileOn(branch, "/failed.txt"), "partial\n");
  assert.equal(await commits(), before + 1);

  const operator = new LayerFSOperator(other);
  for (const sandbox of await operator.sandbox.list()) {
    for (let n = 1; n <= 16; n++) {
      await assert.rejects(other.workspace.status({ sandbox: sandbox.id, path: `/workspaces/${n}` }), { code: "NotMounted" });
    }
  }
  assert.deepEqual(readdirSync(cwd), []);
});

// after the bash test, so the Sandbox is running and a mount is as quick as the edit
test("a bash call after an edit in the same message sees the edit", async () => {
  faux.setResponses([
    fauxAssistantMessage([
      fauxToolCall("edit", { path: "notes/todo.txt", edits: [{ oldText: "milk", newText: "bread" }] }),
      fauxToolCall("bash", { command: "cat notes/todo.txt" }),
    ]),
    fauxAssistantMessage("done"),
  ]);
  await runtime.session.prompt("buy bread instead, then show the list");
  const bash = runtime.session.messages.find((m) => m.role === "toolResult" && m.toolName === "bash");
  assert.equal(output(bash), "- buy bread\n");
  assert.equal(await fileOn(branch, "/notes/todo.txt"), "- buy bread\n");
});

test("the layerfs tool lists the Commits, diffs Branches and Commits, and publishes the Branch to main", async () => {
  const log = output(await call("layerfs", { action: "log" }));
  for (const message of ["write /notes/todo.txt", "edit /src/app.js", "another writer", "echo built > out.txt && ls (exit 0)", "(exit 3)"]) {
    assert.ok(log.includes(message), `${message} in\n${log}`);
  }
  const fromMain = output(await call("layerfs", { action: "diff", from: "main" })).split("\n");
  for (const change of ["Added /notes/todo.txt", "Modified /src/app.js", "Added /failed.txt"]) assert.ok(fromMain.includes(change), `${change} in\n${fromMain.join("\n")}`);
  // the build's Commit and the failing call's, which came next
  const idOf = (message) => log.split("\n").find((line) => line.includes(message)).split(" ")[0];
  assert.equal(output(await call("layerfs", { action: "diff_commits", from: idOf("echo built"), to: idOf("(exit 3)") })), "Added /failed.txt");
  await call("layerfs", { action: "publish" });
  assert.equal(await fileOn("main", "/notes/todo.txt"), "- buy bread\n");
  assert.equal(await fileOn("main", "/src/app.js"), "export const a = 10;\nexport const b = 20;\n");
});

test("a session whose setup fails writes nothing locally and refuses !commands with the setup's error", async () => {
  const { runtime: failed, errors } = await startSession(`${project}-missing`);
  try {
    assert.equal(errors.length, 1);
    const write = failed.session.getToolDefinition("write").execute("call-write", { path: "local.txt", content: "x" }, undefined, undefined, undefined);
    await assert.rejects(write);
    const bang = failed.session.extensionRunner.emitUserBash({ type: "user_bash", command: "touch local.txt", excludeFromContext: false, cwd });
    await assert.rejects(bang, (error) => error.message === errors[0].error);
    assert.deepEqual(readdirSync(cwd), []);
  } finally {
    await failed.dispose();
  }
});
