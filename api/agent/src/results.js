// P10 per-call results: each client records one result per call a trace file
// would record, in completion order (see api/docs/agent.md#per-call-results).
// The ring keeps the most recent 1024 records and drops oldest without
// reusing sequence numbers. Reading is local and works after close().
import { LayerFSError } from "./errors.js";

const CAPACITY = 1024;

/** A client's per-call recorder: `sink` appends one settled call's record,
 * `read(after)` pages the kept records. */
export function createResults() {
  let next = 1;
  let evicted = 0;
  const records = [];
  const sink = (record) => {
    record.seq = next++;
    if (records.length === CAPACITY) {
      records.shift();
      evicted++;
    }
    records.push(record);
  };
  const read = (after = 0) => {
    if (after === null || after === undefined) after = 0;
    if (!Number.isSafeInteger(after) || after < 0) {
      throw new LayerFSError("InvalidInput", "after must be a nonnegative safe integer", null);
    }
    const items = records.filter((record) => record.seq > after);
    const stale = after + 1 < next - records.length;
    return {
      items,
      next: items.length ? items[items.length - 1].seq : after,
      evicted,
      ...(stale ? { stale: true } : {}),
    };
  };
  return { sink, read };
}
