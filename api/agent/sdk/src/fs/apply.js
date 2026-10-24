// fs.apply: one positional SDK operation. Write bodies as in fs.write.
import { checkedMessage } from "../client.js";
import { change } from "./body.js";
export function apply(client, branch, changes, message) {
  return client.call("fs.apply", { branch, changes: Array.isArray(changes) ? changes.map(change) : changes, message: checkedMessage(message) });
}
