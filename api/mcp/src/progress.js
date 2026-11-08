// Progress: a call's on_progress events become notifications/progress for the
// request's token, coalesced so one call cannot flood the channel.
//
// Rule: the first event is sent at once; after that at most one every
// INTERVAL_MS (100 ms), carrying the latest event; `finish()` sends the last
// event if it is not the one already sent. `progress` is a strictly increasing
// counter of notifications (1, 2, 3, ...), because event counters are not
// monotonic across every phase; `total` is included only when the latest
// event's total is a positive safe integer; `message` is that event as JSON,
// cut at MESSAGE_CHARS.
//
// `finish()` then yields SETTLE_MS before the caller writes the tool result.
// The MCP client SDK dispatches notifications in a microtask but responses
// synchronously, so a notification written in the same pipe chunk as its
// response can be dropped as an unknown token; the yield makes the client read
// the notification first. It is only paid by a call that sent a notification.

const INTERVAL_MS = 100;
const SETTLE_MS = 10;
const MESSAGE_CHARS = 512;
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * A reporter for one request, or null when the client sent no progress token.
 * `send` is the request handler's sendNotification.
 */
export function progressReporter(send, token, interval = INTERVAL_MS) {
  if (token === undefined || token === null) return null;
  let latest = null;
  let sent = 0;
  let first = true;
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
    return text.length > MESSAGE_CHARS ? `${text.slice(0, MESSAGE_CHARS)}…` : text;
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
      if (first) {
        first = false;
        flush();
        return;
      }
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
      if (sent > 0) await pause(SETTLE_MS);
    },
  };
}
