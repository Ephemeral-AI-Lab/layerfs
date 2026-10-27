// Opt-in per-call trace: one JSON line per SDK or backend call, appended to
// the client's `trace` file (LAYERFS_TRACE). Off when no file is named. With
// LAYERFS_TRACE_START set to 1, a start line precedes each call. A line never holds file contents, and
// a failed trace write is dropped: it never changes the call's value or refusal.
import { appendFileSync } from "node:fs";
import { randomBytes } from "node:crypto";

const RUN = randomBytes(4).toString("hex");
let next = 0;
/** Fields whose text is file content: only their byte length is traced. */
const CONTENT = new Set(["content", "bytes", "oldText", "newText"]);
/** Deployment and discovery settings are not call arguments. */
const SETTINGS = new Set(["deploy", "home", "project", "storePath"]);
const TEXT = 200;

const size = (value) => (typeof value === "string" ? Buffer.byteLength(value) : value?.length ?? 0);
const short = (text) => (text.length > TEXT ? `${text.slice(0, TEXT)}…` : text);

/** Paths, refs, ids, flags and numbers; content as byte counts; lists as counts. */
export function summary(args = {}) {
  const out = {};
  for (const [name, value] of Object.entries(args)) {
    if (SETTINGS.has(name) || value === undefined) continue;
    if (CONTENT.has(name)) out[`${name}Bytes`] = size(value);
    else if (name === "changes" && Array.isArray(value)) {
      out.changes = value.length;
      out.changeBytes = value.reduce((total, change) => total + [...CONTENT].reduce((sum, field) => sum + size(change?.[field]), 0), 0);
    } else if (typeof value === "string") out[name] = short(value);
    else if (typeof value === "number" || typeof value === "boolean" || value === null) out[name] = value;
    else if (Array.isArray(value)) out[name] = value.length;
  }
  return out;
}

/** Methods that publish a Commit (or answer UpToDate with the head). */
const PUBLISHING = new Set(["fs.write", "fs.edit", "fs.apply", "fs.mkdir", "fs.rm", "fs.cp", "fs.mv", "workspace.commit", "branch.merge", "branch.publish"]);

/** The Commit id, and its size counters when the value carries the record. A
 * publishing call names its new Commit (`commit.id`), its merge or UpToDate
 * head (`head`), or the published source Commit and Layer. */
function committed(method, value) {
  if (!value || typeof value !== "object") return {};
  const record = value.commit && typeof value.commit === "object" ? value.commit : null;
  const out = {};
  if (value.kind) out.kind = value.kind;
  const publishing = PUBLISHING.has(method);
  const id = record?.id ?? (typeof value.commit === "string" ? value.commit : undefined) ?? (publishing ? value.source?.commit ?? value.head : undefined);
  if (id) out.commit = id;
  if (publishing && value.publishedLayer?.id) out.layer = value.publishedLayer.id;
  if (record && "logical" in record) {
    out.sizes = { logical: record.logical, newCanonical: record.newCanonical, newStored: record.newStored, newMetadata: record.newMetadata };
  }
  return out;
}

/** Result ids a viewer needs to follow a call: the Branch and head a fork or
 * merge made, the Workspace a mount made (its Sandbox, path and image), and an
 * exec's process or exit code. Ids and numbers only, never output text. */
const RESULT_IDS = {
  "branch.fork": ["branch", "base", "head"],
  "branch.merge": ["branch", "head"],
  "workspace.mount": ["sandbox", "path", "image", "branch", "base", "baseLayer"],
  "sandbox.create": ["sandbox", "image"],
};

function results(method, args, value) {
  if (!value || typeof value !== "object") return {};
  const out = {};
  for (const name of RESULT_IDS[method] ?? []) {
    if (typeof value[name] === "string" || value[name] === null) out[name] = value[name];
  }
  if (typeof value.process === "string") out.process = value.process;
  if (Number.isInteger(value.exitCode)) out.exitCode = value.exitCode;
  return out;
}

/** One daemon telemetry value with its flat timing nodes as a tree. */
function daemon(telemetry) {
  const { timing, ...rest } = telemetry ?? {};
  const nodes = [];
  const roots = [];
  for (const [name, parent, us, outcome, incomplete] of timing?.nodes ?? []) {
    const node = { name, ms: us / 1000, ...(outcome === "ok" ? {} : { outcome }), ...(incomplete ? { incomplete } : {}), children: [] };
    nodes.push(node);
    (parent >= 0 && nodes[parent] ? nodes[parent].children : roots).push(node);
  }
  return { timing: roots, ...(timing?.omitted ? { omitted: timing.omitted } : {}), ...rest };
}

/** The step label for a call: LAYERFS_TRACE_STEP as it is at the call;
 * empty means none. */
function step() {
  const label = process.env.LAYERFS_TRACE_STEP;
  return typeof label === "string" && label ? { step: short(label) } : {};
}

/** Sends one call once through `send(scope)` and returns the binding's
 * answer. When `options.trace` names a file the call is traced: `send` gets
 * a fresh scope, the answer's telemetry goes to the trace line and is
 * removed from it. */
export async function traced(options, method, args, send) {
  const file = options?.trace;
  if (!file) return send(null);
  const id = `${RUN}-${++next}`;
  const scope = randomBytes(32).toString("hex");
  const start = new Date();
  const began = performance.now();
  let line;
  try {
    line = { layerfs_trace: 1, id, ...step(), method, ...(typeof args?.project === "string" ? { project: args.project } : {}), args: summary(args), start: start.toISOString(), scope };
  } catch {
    line = { layerfs_trace: 1, id, method, args: null, start: start.toISOString(), scope };
  }
  if (process.env.LAYERFS_TRACE_START === "1") write(file, { ...line, phase: "start" });
  line.phase = "end";
  let answer;
  try {
    answer = await send(scope);
  } catch (error) {
    write(file, { ...line, ms: elapsed(began), ok: false, code: error?.code ?? null, message: error?.message ?? String(error) });
    throw error;
  }
  const { telemetry, client, ...plain } = answer;
  const ms = elapsed(began);
  // Queue time, never hidden in the call's work (#352 F-017): the wait for a
  // worker thread (the call's time outside its run in the binding) and the
  // wait for a free control connection of its Sandbox.
  const queue = client ? queued(ms, client) : null;
  try {
    const result = plain.refused
      ? { ok: false, code: plain.refused.code, message: plain.refused.message }
      : { ok: true, ...committed(method, plain.value), ...results(method, args, plain.value) };
    write(file, { ...line, ms, ...(queue ? { queue } : {}), ...result, ...(telemetry ? { daemon: telemetry.map(daemon) } : {}) });
  } catch {
    write(file, { ...line, ms, ok: !plain.refused });
  }
  return plain;
}

function queued(ms, { runUs, connectionWaitUs }) {
  const round = (v) => Math.round(v * 1000) / 1000;
  const thread = round(Math.max(0, ms - runUs / 1000));
  const connection = round(connectionWaitUs / 1000);
  return { ms: round(thread + connection), thread, connection };
}

const elapsed = (began) => Math.round((performance.now() - began) * 1000) / 1000;

function write(file, line) {
  try {
    appendFileSync(file, `${JSON.stringify(line)}\n`);
  } catch {
    // A trace is diagnostic; its failure is not the call's.
  }
}
