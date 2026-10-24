// runtime.events.subscribe({ project, since }): each call from every process
// in this home, as its trace line's fields plus `actor`, the trace file it
// came from (one per agent; LAYERFS_TRACE). It reads the trace folder's
// *.jsonl files every pollMs, as the dsh Host's collector does: lines whose
// `start` is at or after `since` (an ISO time), else only lines written from
// now on, of `project` when given (a line's `project` field). The answer is
// an async iterable of events; `close()` ends it. Lines are read, never
// changed or removed.
import { closeSync, openSync, readSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { LayerFSError } from "@layerfs/agent";

const CHUNK = 1 << 20;

export function subscribe(runtime, { project, since } = {}) {
  if (since !== undefined && Number.isNaN(Date.parse(since))) throw new LayerFSError("InvalidInput", "since is an ISO time", null);
  const dir = runtime.settings.traceDir, after = since === undefined ? null : Date.parse(since);
  /** file -> { offset, rest } */
  const files = new Map(), queue = [], waiting = [];
  let closed = false, timer = null;
  const push = (event) => (waiting.length ? waiting.shift()({ value: event, done: false }) : queue.push(event));
  const scan = (first) => {
    let names = [];
    try { names = readdirSync(dir).filter((name) => name.endsWith(".jsonl")); } catch { /* no traces yet */ }
    for (const name of names) {
      const path = join(dir, name);
      let size;
      try { size = statSync(path).size; } catch { continue; }
      let held = files.get(path);
      // without `since`, a file there at subscribe time starts at its end
      if (!held) files.set(path, (held = { offset: first && after === null ? size : 0, rest: "" }));
      if (size < held.offset) Object.assign(held, { offset: 0, rest: "" });
      if (size === held.offset) continue;
      const fd = openSync(path, "r");
      try {
        const buffer = Buffer.allocUnsafe(Math.min(CHUNK, size - held.offset));
        const read = readSync(fd, buffer, 0, buffer.length, held.offset);
        held.offset += read;
        const lines = (held.rest + buffer.subarray(0, read).toString("utf8")).split("\n");
        held.rest = lines.pop();
        for (const text of lines) {
          let line;
          try { line = JSON.parse(text); } catch { continue; }
          if (line?.layerfs_trace !== 1) continue;
          if (project !== undefined && line.project !== project) continue;
          if (after !== null && !(Date.parse(line.start) >= after)) continue;
          push({ actor: name.slice(0, -".jsonl".length), ...line });
        }
      } finally {
        closeSync(fd);
      }
    }
  };
  scan(true);
  const tick = () => { if (closed) return; scan(false); timer = setTimeout(tick, runtime.settings.pollMs); };
  timer = setTimeout(tick, runtime.settings.pollMs);
  return {
    close() {
      closed = true;
      clearTimeout(timer);
      for (const resolve of waiting.splice(0)) resolve({ value: undefined, done: true });
    },
    [Symbol.asyncIterator]() { return this; },
    next() {
      if (queue.length) return Promise.resolve({ value: queue.shift(), done: false });
      if (closed) return Promise.resolve({ value: undefined, done: true });
      return new Promise((resolve) => waiting.push(resolve));
    },
    return() { this.close(); return Promise.resolve({ value: undefined, done: true }); },
  };
}
