// In-memory Init-job watchers. Each has one private native-style queue, but
// waits in JavaScript and carries complete immutable job values.
import { randomBytes } from "node:crypto";
import { LayerFSError } from "@layerfs/agent";

const PRIVATE = new WeakMap();
const copy = (value) => structuredClone(value);
const validTimeout = (value) => {
  if (!Number.isSafeInteger(value) || value < 0 || value > 30_000) throw new LayerFSError("InvalidInput", "timeoutMs must be an integer in 0..30000", null);
};

class Watcher {
  constructor(job, close) {
    this.id = randomBytes(32).toString("hex");
    this.job = copy(job);
    this.capacity = 1024;
    this.seq = 0;
    this.minimum = 0;
    this.events = [];
    this.stale = false;
    this.closed = false;
    this.waiter = null;
    this.closeOwner = close;
  }

  report(job) {
    if (this.closed) return;
    this.job = copy(job);
    if (this.seq === Number.MAX_SAFE_INTEGER) {
      this.exhausted = true;
      this.wake();
      return;
    }
    this.seq += 1;
    const event = { seq: this.seq, type: "init.progress", source: "runtime", payload: { job: copy(this.job) } };
    if (this.events.length === this.capacity) {
      const dropped = this.events.shift();
      this.minimum = dropped.seq;
      this.stale = true;
    }
    this.events.push(event);
    this.wake();
  }

  wake() {
    const waiter = this.waiter;
    if (waiter) {
      clearTimeout(waiter.timer);
      waiter.resolve();
    }
  }

  page(cursor, limit = 50) {
    if (this.closed) return { events: [], cursor: this.seq, hasMore: false, stale: false, closed: true };
    if (this.exhausted) throw new LayerFSError("InvalidInput", "cursor exhausted; resubscribe", null);
    if (!Number.isSafeInteger(cursor) || cursor < 0 || cursor > this.seq) throw new LayerFSError("InvalidInput", "unknown cursor", null);
    if (this.stale || cursor < this.minimum) {
      const current = this.seq;
      this.events = [];
      this.minimum = current;
      this.stale = false;
      return { events: [], cursor: current, hasMore: false, stale: true, closed: false, snapshot: copy(this.job) };
    }
    const events = this.events.filter((event) => event.seq > cursor).slice(0, limit);
    if (events.length) {
      const last = events.at(-1).seq;
      this.events = this.events.filter((event) => event.seq > last);
      this.minimum = last;
      cursor = last;
    }
    return { events: copy(events), cursor, hasMore: this.events.length > 0, stale: false, closed: false };
  }

  async poll(cursor, timeoutMs) {
    validTimeout(timeoutMs);
    if (this.waiter) throw new LayerFSError("InvalidInput", "a poll is already outstanding", null);
    let page = this.page(cursor);
    if (timeoutMs === 0 || page.events.length || page.stale || page.closed || this.exhausted) return page;
    const deadline = Date.now() + timeoutMs;
    while (!this.closed && !this.stale && !this.exhausted && !this.events.some((event) => event.seq > cursor)) {
      const remaining = deadline - Date.now();
      if (remaining <= 0) break;
      await new Promise((resolve) => {
        const timer = setTimeout(resolve, remaining);
        this.waiter = { resolve, timer };
      });
      this.waiter = null;
    }
    page = this.page(cursor);
    return page;
  }

  close() {
    if (this.closed) return;
    this.closed = true;
    this.events = [];
    this.closeOwner(this);
    this.wake();
  }
}

async function* iterate(subscription) {
  let wait = 1000;
  for (;;) {
    const page = await subscription.poll(wait);
    if (page.closed) return;
    if (page.stale) throw new LayerFSError("InvalidInput", "stream went stale; resubscribe", null);
    for (const event of page.events) yield event;
    wait = page.hasMore ? 0 : 1000;
  }
}

export function subscription(watcher) {
  const current = {
    cursor: 0,
    snapshot: copy(watcher.job),
    async poll(timeoutMs = 0) {
      validTimeout(timeoutMs);
      const state = PRIVATE.get(this);
      if (state.polling) throw new LayerFSError("InvalidInput", "a poll is already outstanding", null);
      state.polling = true;
      try {
        const page = await watcher.poll(this.cursor, timeoutMs);
        this.cursor = page.cursor;
        if (page.stale) this.snapshot = page.snapshot;
        if (page.closed) state.closed = true;
        return page;
      } finally {
        state.polling = false;
      }
    },
    async close() {
      const state = PRIVATE.get(this);
      watcher.close();
      state.closed = true;
    },
    [Symbol.asyncIterator]() {
      return iterate(this);
    },
  };
  PRIVATE.set(current, { closed: false, polling: false });
  return current;
}

export function watcher(job, close) {
  const owned = new Watcher(job, close);
  return { owned, current: subscription(owned) };
}
