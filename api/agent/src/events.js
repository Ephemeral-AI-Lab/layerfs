// Same-client opened-project events. The native subscription ID and each
// poll's in-flight state stay private to this module.
import { invoke, invokeClient, live, projectArgs, STATE, storeArgs, transportOf } from "./client.js";
import { LayerFSError } from "./errors.js";

const PRIVATE = new WeakMap();

const timeout = (value) => {
  if (!Number.isSafeInteger(value) || value < 0 || value > 30_000) {
    throw new LayerFSError("InvalidInput", "timeoutMs must be an integer in 0..30000", null);
  }
  return value;
};

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

function close(client, subscription, owner = false) {
  const state = PRIVATE.get(subscription);
  if (state.closing) return state.closing;
  state.closing = (async () => {
    const result = owner
      ? await invoke("events.unsubscribe", { ...projectArgs(client), ...storeArgs(client), subscription: state.id }, client.options, transportOf(client))
      : (live(client), await invokeClient(client, "events.unsubscribe", { subscription: state.id }));
    state.closed = true;
    if (client[STATE].subscription?.current === subscription) client[STATE].subscription = null;
    return result;
  })();
  return state.closing;
}

export async function subscribe(client) {
  live(client);
  const previous = client[STATE].subscription;
  if (previous) await previous.current.close();
  const answer = await invokeClient(client, "events.subscribe", {});
  const subscription = {
    cursor: answer.cursor,
    snapshot: answer.snapshot,
    async poll(timeoutMs = 0) {
      live(client);
      timeout(timeoutMs);
      const state = PRIVATE.get(this);
      if (state.polling) throw new LayerFSError("InvalidInput", "a poll is already outstanding", null);
      state.polling = true;
      try {
        const page = await invokeClient(client, "events.poll", { subscription: state.id, cursor: this.cursor, timeoutMs });
        this.cursor = page.cursor;
        if (page.stale) this.snapshot = page.snapshot;
        if (page.closed) state.closed = true;
        return page;
      } finally {
        state.polling = false;
      }
    },
    close() {
      return close(client, this);
    },
    [Symbol.asyncIterator]() {
      return iterate(this);
    },
  };
  PRIVATE.set(subscription, { id: answer.subscription, closed: false, polling: false, closing: null });
  client[STATE].subscription = { id: answer.subscription, current: subscription };
  return subscription;
}

export async function closeOwned(client) {
  const owned = client[STATE].subscription;
  if (!owned) return;
  client[STATE].subscription = null;
  await close(client, owned.current, true);
}
