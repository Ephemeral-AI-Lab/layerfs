// Progress: a call's on_progress events become notifications/progress for the
// request's token, coalesced so one call cannot flood the channel.
//
// Rule: at most one notification every INTERVAL_MS (100 ms), carrying the
// latest event; `finish()` always sends the last one before the tool result.
// `progress` is a strictly increasing counter of notifications (1, 2, 3, ...),
// because event counters are not monotonic across every phase; `total` is
// included only when the latest event's total is a positive safe integer;
// `message` is that event as JSON, cut at MESSAGE_BYTES.

const INTERVAL_MS = 100;
const MESSAGE_BYTES = 512;

/**
 * A reporter for one request, or null when the client sent no progress token.
 * `send` is the request handler's sendNotification.
 */
export function progressReporter(send, token, interval = INTERVAL_MS) {
  if (token === undefined || token === null) return null;
  let latest = null;
  let sent = 0;
  let timer = null;
  let chain = Promise.resolve();
  let closed = false;

  const message = (event) => {
    let text;
    try {
      text = JSON.stringify(event);
    } catch {
      text = String(event);
    }
    return text.length > MESSAGE_BYTES ? `${text.slice(0, MESSAGE_BYTES)}…` : text;
  };

  const flush = () => {
    if (latest === null || closed) return;
    const event = latest;
    latest = null;
    sent += 1;
    const params = { progressToken: token, progress: sent };
    if (Number.isSafeInteger(event?.total) && event.total > 0) params.total = event.total;
    params.message = message(event);
    chain = chain.then(() => send({ method: "notifications/progress", params })).catch(() => {});
  };

  return {
    /** The call's on_progress handler; synchronous and cheap. */
    on_progress(event) {
      latest = event;
      if (timer !== null) return;
      timer = setTimeout(() => {
        timer = null;
        flush();
      }, interval);
      timer.unref?.();
    },
    /** Send the pending event, then wait until every notification has gone. */
    async finish() {
      if (timer !== null) {
        clearTimeout(timer);
        timer = null;
      }
      flush();
      closed = true;
      await chain;
    },
  };
}
