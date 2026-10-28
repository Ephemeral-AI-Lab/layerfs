// branch.merge: one positional SDK operation. `units` (optional) lists merge
// units: patterns, or { name, patterns } groups (#352 F-057).
import { checkedMessage } from "../client.js";
export function merge(client, a, b, name, message, units) {
  const args = { branch: a, with: b, name, message: checkedMessage(message) };
  if (units !== undefined) args.units = units;
  return client.call("branch.merge", args);
}
