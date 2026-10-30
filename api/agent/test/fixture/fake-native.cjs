// The one stand-in native binding for the SDK and `layerfs` CLI tests: no
// Docker, Store or daemon. Store calls name the client's storePath; Workspace
// calls their Sandbox. Every call resolves like the real binding, with small
// fixed answers.
// - Recording: in-process tests read the exports `calls`, `observed`,
//   `deploys`, `args` and `stores`. A CLI test runs the SDK in a subprocess,
//   so each call is also appended to the JSON-lines file CLI_FAKE_LOG as
//   [op, args] (bytes as { base64 }); the Store opens as "store.open"/"close".
// - Refusals: a path "/missing" is refused NoSuchPath and a Store path
//   "/missing" NoSuchProject; each op named in CLI_FAKE_REFUSE
//   (comma-separated) is refused with code CLI_FAKE_CODE (default Conflict);
//   `initRefusal`, when set, is what project.init refuses; a policy depth over
//   255 is refused as the addon refuses it.
// - Answers: `branchList` enables Branch paging, with `branchReplyBudget`
//   optionally limiting each reply; `sandboxList` is the project's running
//   Sandboxes, `workspaceList` a Sandbox's daemon records. An exec command
//   "exit <n>" ends with exit code n, stdout "out:<command>" and stderr "err";
//   any other command ends with code 3 and 5000 bytes of stdout.
const { appendFileSync } = require("node:fs");

const plain = (value) => {
  if (Buffer.isBuffer(value) || value instanceof Uint8Array) return { base64: Buffer.from(value).toString("base64") };
  if (Array.isArray(value)) return value.map(plain);
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, plain(v)]));
  return value;
};
const log = (op, args) => {
  if (process.env.CLI_FAKE_LOG) appendFileSync(process.env.CLI_FAKE_LOG, `${JSON.stringify([op, plain(args)])}\n`);
};

const commit = { id: "c0ffee", author: null, labels: [], logical: 120, newCanonical: 40, newStored: 30, newMetadata: 8 };
const committed = { kind: "Committed", head: commit.id, commit };
const refusal = (code, message, detail = null) => ({ refused: { code, message, detail } });

const P8_COMMIT = `12${"a".repeat(64)}`;
const P8_LAYER = `32${"b".repeat(64)}`;
const P8_UNKNOWN_COMMIT = `12${"c".repeat(64)}`;
const P8_UNKNOWN_LAYER = `32${"d".repeat(64)}`;
const P8_BAD_TAG = `99${"e".repeat(64)}`;
const P8_NONHEX = "z".repeat(66);
const P8_REFS = new Set([P8_COMMIT, P8_LAYER, P8_UNKNOWN_COMMIT, P8_UNKNOWN_LAYER, P8_BAD_TAG, P8_NONHEX, "p8-unknown-branch"]);
const p8Text = (ref) => ref === P8_COMMIT ? "v1\n" : "v2\n";
const p8Header = (ref, path) => ({ kind: "ok", snapshot: ref.slice(2, 14), root: `${ref === P8_COMMIT ? "aa" : "bb"}`.repeat(33), path });
const p8Items = (ref, path) => {
  const names = path === "/dir" ? [["inside.txt", "file"]] : [["binary.bin", "file"], ["dir", "dir"], ["file.txt", "file"], ["link", "symlink"], ["same.txt", "file"]];
  return names.map(([name, type], index) => ({ name, path: `${path === "/" ? "" : path}/${name}`, type, inode: index + 2 }));
};
const p8List = (args) => {
  const items = p8Items(args.branch, args.path);
  const at = args.pagePos ?? 0;
  const size = args.pageSize ?? 50;
  const page = items.slice(at, at + size);
  return { value: { ...p8Header(args.branch, args.path), items: page, hasMore: at + page.length < items.length, nextPagePos: at + page.length < items.length ? at + page.length : null } };
};
const p8Read = (args) => {
  if (args.branch === P8_BAD_TAG) return refusal("InvalidInput", `\`${args.branch}\` is not a Commit or Layer identity`);
  if (args.branch === P8_UNKNOWN_COMMIT) return refusal("NoSuchCommit", `Commit ${args.branch} does not exist`);
  if (args.branch === P8_UNKNOWN_LAYER) return refusal("NotFound", `Layer ${args.branch} does not exist`);
  if (args.branch === P8_NONHEX || args.branch === "p8-unknown-branch") return refusal("NoSuchBranch", `Branch ${args.branch} does not exist`);
  if (args.path === "bad") return refusal("InvalidInput", "expected an absolute path without empty, dot or parent components");
  if (args.path === "/dir") return refusal("IsDirectory", "expected a regular file");
  if (args.path === "/link") return refusal("NotFile", "expected a regular file");
  if (args.path === "/missing") return refusal("NoSuchPath", `/missing does not exist at ${args.branch.slice(2, 14)}`, { path: "/missing" });
  const binary = args.path === "/binary.bin";
  if (binary && !args.binary) return refusal("Binary", "text contains NUL or invalid UTF-8");
  const content = binary ? Buffer.from([0, 255, 1, 10]) : Buffer.from(args.path === "/same.txt" ? "same\n" : p8Text(args.branch));
  const continued = typeof args.offset === "string";
  const value = { ...p8Header(args.branch, args.path), startLine: continued ? 2 : args.startLine ?? 1, endLine: continued ? 2 : 1, lineTruncated: false, truncated: continued, nextLine: continued ? 3 : null, nextOffset: continued ? `lfr1.t.3.6.${"ab".repeat(32)}.2f66696c652e747874` : null, size: content.length };
  return { value: args.binary ? { ...value, bytes: content } : { ...value, text: content.toString("utf8") } };
};

const MAX_SAFE = Number.MAX_SAFE_INTEGER;
const EVENT_BUDGET = 49_152;
const subscriptions = new Map();
let nextSubscription = 1;
let admittedPolls = 0;
const valueCost = (value) => {
  if (value === null || typeof value === "boolean") return 1;
  if (typeof value === "number") return 9;
  if (typeof value === "string") return Buffer.byteLength(value) + 5;
  if (Buffer.isBuffer(value)) return value.length + 5;
  if (Array.isArray(value)) return 5 + value.reduce((sum, item) => sum + valueCost(item), 0);
  return 5 + Object.entries(value).reduce((sum, [name, item]) => sum + Buffer.byteLength(name) + 2 + valueCost(item), 0);
};
const eventPage = (events, cursor, hasMore = false) => ({ value: { events, cursor, hasMore, stale: false, closed: false } });
const closedPage = (state) => ({ value: { events: [], cursor: state.seq, hasMore: false, stale: false, closed: true } });
const invalid = (message) => refusal("InvalidInput", message);
function fakeSnapshot() {
  const branches = (exports.branchList ?? ["main", "cart"]).map((name) => ({ name, head: name === "cart" ? "head1234567890ab" : null }));
  const sandboxes = exports.sandboxList.map(({ sandbox }) => ({ sandbox }));
  const workspaces = [];
  for (const { sandbox } of exports.sandboxList) {
    const records = exports.workspaceLists?.[sandbox] ?? exports.workspaceList;
    for (const workspace of records) if (workspace.mounted && workspace.directory) workspaces.push({ sandbox, path: workspace.directory, branch: workspace.branch ?? null });
  }
  return { branches, workspaces, sandboxes };
}
function settlePoll(state) {
  const waiter = state.waiter;
  if (!waiter) return;
  state.waiter = null;
  clearTimeout(waiter.timer);
  waiter.resolve(pollPage(state, waiter.cursor, waiter.limit));
}
function pollPage(state, cursor, limit) {
  if (state.closed) return closedPage(state);
  if (state.exhausted) return invalid("cursor exhausted; resubscribe");
  if (cursor > state.seq) return invalid("unknown cursor");
  if (state.stale || cursor < state.minimum) {
    const current = state.seq;
    state.events = [];
    state.minimum = current;
    state.stale = false;
    return { value: { events: [], cursor: current, hasMore: false, stale: true, closed: false, snapshot: fakeSnapshot() } };
  }
  const deliverable = state.events.filter((event) => event.seq > cursor);
  const events = [];
  let spent = 0;
  while (deliverable.length && events.length < limit) {
    const event = deliverable[0];
    if (events.length && spent + event.cost > EVENT_BUDGET) break;
    events.push(deliverable.shift());
    spent += event.cost;
    if (spent >= EVENT_BUDGET) break;
  }
  if (events.length) {
    const last = events.at(-1).seq;
    state.events = state.events.filter((event) => event.seq > last);
    state.minimum = last;
    cursor = last;
  }
  return eventPage(events.map(({ cost, ...event }) => event), cursor, state.events.length > 0);
}
function eventSubscribe() {
  const id = (nextSubscription++).toString(16).padStart(64, "0");
  const state = { id, seq: 0, minimum: 0, events: [], stale: false, closed: false, exhausted: false, polling: false, waiter: null };
  subscriptions.set(id, state);
  return { value: { subscription: id, cursor: 0, snapshot: fakeSnapshot() } };
}
function eventPoll(args) {
  const state = subscriptions.get(args.subscription);
  if (!state) return invalid("unknown subscription");
  const { cursor, timeoutMs = 0, limit = 50 } = args;
  if (!Number.isSafeInteger(cursor) || cursor < 0) return invalid("cursor must be a nonnegative safe integer");
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 0 || timeoutMs > 30_000) return invalid("timeoutMs must be an integer in 0..30000");
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 500) return invalid("limit must be an integer in 1..500");
  if (state.polling) return invalid("a poll is already outstanding");
  if (exports.pollRefusals?.length) return refusal(exports.pollRefusals.shift(), "poll refused by the fake");
  state.polling = true;
  if (timeoutMs > 0) {
    const workers = Number(process.env.UV_THREADPOOL_SIZE ?? 4);
    const admission = Math.max(0, Math.min(16, workers - 1));
    if (admittedPolls >= admission) {
      state.polling = false;
      return refusal("Busy", "long-poll admission is saturated; no wait was started", { kind: "long-poll admission" });
    }
    admittedPolls++;
  }
  const finish = (page) => {
    state.polling = false;
    if (timeoutMs > 0) admittedPolls--;
    return page;
  };
  const page = pollPage(state, cursor, limit);
  if (timeoutMs === 0 || page.refused || page.value.events.length || page.value.stale || page.value.closed) return finish(page);
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      state.waiter = null;
      resolve(finish(pollPage(state, cursor, limit)));
    }, timeoutMs);
    state.waiter = { cursor, limit, timer, resolve: (page) => resolve(finish(page)) };
  });
}
function eventUnsubscribe(args) {
  const state = subscriptions.get(args.subscription);
  if (!state) return invalid("unknown subscription");
  subscriptions.delete(args.subscription);
  state.closed = true;
  state.events = [];
  settlePoll(state);
  return { value: { kind: "Unsubscribed" } };
}
function emitEvent(id, type, source, payload) {
  const state = subscriptions.get(id);
  if (!state || state.closed || state.exhausted) return;
  if (state.seq === MAX_SAFE) {
    state.exhausted = true;
    settlePoll(state);
    return;
  }
  state.seq++;
  const event = { seq: state.seq, type, source, payload };
  const capacity = Number(process.env.LAYERFS_FAKE_EVENT_CAP ?? 1024);
  if (state.events.length === capacity) {
    const dropped = state.events.shift();
    state.minimum = dropped.seq;
    state.stale = true;
  }
  state.events.push({ ...event, cost: valueCost(event) });
  settlePoll(state);
}
function mapEvent(op, args, value) {
  const kind = value.kind;
  if (op === "workspace.commit" && kind === "Committed") return ["commit.created", "store", { branch: args.branch ?? "main", workspace: { sandbox: args.sandbox, path: args.path }, commit: value.commit }];
  if (/^fs\.(write|apply|edit|mkdir|rm|mv|cp)$/.test(op) && kind === "Committed") {
    const moving = op === "fs.mv" || op === "fs.cp";
    const plain = ["fs.write", "fs.edit", "fs.mkdir", "fs.rm"].includes(op);
    return ["fs.committed", "store", { branch: value.branch ?? args.branch, method: op, path: plain ? args.path : null, from: moving ? args.from : null, to: moving ? args.to : null, commit: value.commit }];
  }
  if (op === "branch.fork" && kind === "Forked") return ["branch.forked", "store", { branch: value.branch, forkPoint: value.head === null ? { kind: "layer", id: value.base } : { kind: "commit", id: value.head } }];
  if (op === "branch.merge" && kind === "Merged") return ["branch.merged", "store", { merge: value }];
  if (op === "branch.publish" && kind === "Added") return ["branch.published", "store", { branch: value.source.branch, commit: value.source.commit, layer: value.publishedLayer.id, basedOn: value.basedOn }];
  if (op === "workspace.mount" && kind === "Mounted") return ["workspace.mounted", "daemon", { workspace: { sandbox: value.sandbox, path: value.path }, branch: value.branch }];
  if (op === "workspace.unmount" && ["Unmounted", "ForceUnmounted"].includes(kind)) return ["workspace.unmounted", "daemon", { workspace: { sandbox: args.sandbox, path: args.path } }];
  if (op === "sandbox.create") return ["sandbox.created", "daemon", { sandbox: value.sandbox }];
  if (op === "sandbox.close" && kind === "Closed") return ["sandbox.closed", "daemon", { sandbox: args.sandbox }];
  return null;
}

function answer(op, args) {
  const refuse = (process.env.CLI_FAKE_REFUSE || "").split(",").filter(Boolean);
  if (refuse.includes(op)) return refusal(process.env.CLI_FAKE_CODE || "Conflict", `${op} refused by the fake`, { op });
  if (args.storePath === "/missing") return refusal("NoSuchProject", "no such project");
  if (args.path === "/missing" && !P8_REFS.has(args.branch)) return refusal("NoSuchPath", "/missing does not exist");
  switch (op) {
    case "project.init":
      if (exports.initRefusal) return { refused: exports.initRefusal };
      if (args.policy?.chunkDepth > 255) return refusal("InvalidInput", "policy.chunkDepth is out of range");
      return { value: { kind: "Initialized", project: args.name, layer: "lay0", root: "r00t", store: args.storePath } };
    case "project.list_branches": {
      if (!exports.branchList) return { value: { items: [
        { name: "main", base: "layer0000000000aa", head: null },
        { name: "cart", base: "base1234567890ab", head: "head1234567890ab" },
      ], hasMore: false, nextPagePos: null } };
      const items = [];
      const names = exports.branchList.slice(args.pagePos ?? 0);
      const limit = args.pageSize ?? 50;
      let spent = 0;
      let hasMore = false;
      for (const name of names) {
        const cost = Buffer.byteLength(name) + 512;
        if (items.length === limit || (exports.branchReplyBudget != null && spent + cost > exports.branchReplyBudget)) {
          hasMore = true;
          break;
        }
        spent += cost;
        items.push({ name, base: "base1234567890ab", head: null });
      }
      const pagePos = args.pagePos ?? 0;
      return { value: { items, hasMore, nextPagePos: hasMore ? pagePos + items.length : null } };
    }
    case "project.list_sandboxes": return { value: { kind: "ok", items: exports.sandboxList } };
    case "branch.fork": return { value: { kind: "Forked", branch: args.name, base: "lay0", head: args.from === "layer" ? null : "not-a-publication" } };
    case "branch.graph": return { value: {
      branch: { name: args.branch, id: "branch1234567890ab", base: "base1234567890ab", head: "head1234567890ab", forkedFrom: "c0ffee" },
      forkPoint: { kind: "commit", id: "c0ffee" },
      commits: [{ ...commit, id: "decaf", branch: args.branch, parent: "parent1234567890", published: true }],
      hasMore: true,
      nextPagePos: (args.pagePos ?? 0) + 1,
    } };
    case "branch.changed_paths": return { value: {
      commit: args.commit,
      parent: "parent1234567890",
      items: [{ path: "/changed.txt", kind: "Modified", type: "file", before: { type: "file", inode: 7 }, after: { type: "file", inode: 7 }, content: true, identity: false, metadata: false, hunks: null, hunksOmitted: false }],
      hasMore: false,
      nextPagePos: null,
      nextCursor: null,
    } };
    case "branch.merge": return { value: exports.mergeResults?.[args.name] ?? { kind: "Merged", branch: args.name, head: "m3rge", root: "r00t" } };
    case "branch.publish": return { value: exports.publishResults?.[args.branch] ?? { kind: "Added", source: { branch: "feature", commit: "pub1" }, publishedLayer: { id: "lay1" } } };
    case "fs.read":
      if (P8_REFS.has(args.branch)) return p8Read(args);
      return { value: args.startLine === 1 ? { text: "a\n", nextLine: 2, nextOffset: `lfr1.t.2.2.${"ab".repeat(32)}.2f61` } : { text: "b\n", nextLine: null, nextOffset: null } };
    case "fs.list":
      if (args.branch === P8_COMMIT || args.branch === P8_LAYER) return p8List(args);
      return { value: { items: [], hasMore: false, nextPagePos: null } };
    case "fs.stat":
      if (args.branch === P8_COMMIT || args.branch === P8_LAYER) {
        const size = Buffer.byteLength(p8Text(args.branch));
        return { value: { ...p8Header(args.branch, args.path), type: "file", inode: 2, size, mode: 420 } };
      }
      return { value: { type: "file", size: 5, mode: 420 } };
    case "fs.rm": return { value: exports.fsResults?.[op] ?? { kind: "UpToDate", head: "h3ad" } };
    case "workspace.mount": return { value: { kind: "Mounted", sandbox: args.sandbox, container: `layerfs-website-${args.sandbox}`, path: "/workspaces/w1", image: "fake-image", branch: args.branch, base: "b4se", baseLayer: "lay0", root: "r00t" } };
    case "workspace.exec": {
      const exit = /^exit (\d+)$/.exec(args.command);
      if (!exit) return { value: { kind: "exit:3", exitCode: 3, stdout: "x".repeat(5000), stderr: "", truncated: false } };
      const code = Number(exit[1]);
      return { value: { kind: `exit:${code}`, exitCode: code, stdout: `out:${args.command}`, stderr: "err", truncated: false } };
    }
    case "workspace.commit": return { value: exports.workspaceCommitResult ?? { ...committed, commit: { ...commit, ...(exports.eventPayloadText === undefined ? {} : { message: exports.eventPayloadText }), author: args.author ?? null, labels: args.labels ?? [] } } };
    case "workspace.unmount": return { value: args.force ? { kind: "ForceUnmounted", abort: "Written", detach: "Detached", cleanup: "Queued" } : { kind: "Unmounted" } };
    case "backend.daemon.process.start": return { value: { kind: "running", process: "proc_1_ab", command: args.command } };
    case "backend.store.project.get_history": return { value: exports.history ?? { kind: "ok", version: "h1", project: "website", branches: [], commits: [], more: {}, layers: [], workspaces: [] } };
    case "backend.store.project.get_storage": return { value: { dedupe: "2", compression: null } };
    case "sandbox.create": return { value: { sandbox: "2", name: "layerfs-website-2", image: args.deploy?.image } };
    case "sandbox.list_workspaces": return { value: { kind: "ok", version: "1", workspaces: exports.workspaceList } };
    case "sandbox.close": return { value: { kind: "Closed", sandbox: args.sandbox } };
    case "runtime.sandbox.reclaim": return { value: { kind: "ok", reclaimed: exports.reclaimed ?? [] } };
    default:
      if (/^fs\.(write|edit|apply|mkdir|cp|mv)$/.test(op)) return { value: exports.fsResults?.[op] ?? (exports.fullCommits ? { ...committed, branch: args.branch, commit: { ...commit, branch: args.branch } } : committed) };
      return { value: { items: [], hasMore: false, nextPagePos: null } };
  }
}
// A traced call (args.observe) gets daemon telemetry like the real binding:
// flat timing nodes [name, parent, us, outcome, incomplete], per-call SQL work,
// and the Workspace's FUSE opcode counts on status and unmount.
const telemetry = (op) => [{
  timing: { nodes: [["commit", -1, 2500, "ok", false], ["capture", 0, 400, "ok", false], ["construct", 0, 1500, "ok", false], ["captured.namespace", 2, 1400, "ok", false]], omitted: 0 },
  sql: { writer: { statements: 3, fullscan_steps: 0 }, readers: { statements: 2, vm_steps: 40 } },
  ...(op === "backend.daemon.workspace.status" || op === "workspace.unmount" ? { fuse: { lookup: 5, getattr: 7, served: 12 } } : {}),
  ...(op === "workspace.commit" ? { storage: { pack_write_bytes: 4096 }, construction: { files_constructed: 1 } } : {}),
}];
exports.calls = [];
exports.observed = [];
exports.deploys = [];
exports.args = [];
exports.behaviorArgs = [];
exports.eventStates = subscriptions;
exports.workspaceList = [];
exports.sandboxList = [{ sandbox: "1", name: "layerfs-website-1", image: "fake-image", workspaces: 0 }];
const traced = async (op, args) => {
  log(op, args);
  exports.calls.push(op);
  exports.observed.push(args.observe ?? null);
  if (args.deploy) exports.deploys.push(args.deploy);
  exports.args.push(args);
  const { subscription, ...behaviorArgs } = args;
  exports.behaviorArgs.push(behaviorArgs);
  let result;
  if (op === "events.subscribe") {
    const delay = Number(process.env.LAYERFS_FAKE_EVENT_SUBSCRIBE_DELAY ?? 0);
    if (delay) await new Promise((resolve) => setTimeout(resolve, delay));
    result = eventSubscribe();
  } else if (op === "events.poll") result = await eventPoll(args);
  else if (op === "events.unsubscribe") result = eventUnsubscribe(args);
  else result = answer(op, behaviorArgs);
  if (result.value && subscription) {
    const event = mapEvent(op, behaviorArgs, result.value);
    if (event) emitEvent(subscription, ...event);
  }
  return args.observe ? { ...result, telemetry: telemetry(op) } : result;
};
exports.stores = [];
// The in-process Store (#352 F-020): opened once per client, never traced.
exports.storeOpen = async (args) => { log("store.open", args); exports.stores.push(["open", args.storePath]); return { value: { clients: 1 } }; };
exports.storeClose = async (args) => { log("store.close", args); exports.stores.push(["close", args.storePath]); return { value: { clients: 0 } }; };
exports.call = async (op, args) => traced(op, args);
exports.backendCall = async (op, args) => traced(op, args);
