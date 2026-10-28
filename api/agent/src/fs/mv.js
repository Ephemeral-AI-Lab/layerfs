// fs.mv: one positional SDK operation.
import { checkedMessage } from "../client.js";
export function mv(client, branch, fromPath, toPath, message) {
  return client.call("fs.mv", { branch, from: fromPath, to: toPath, message: checkedMessage(message) });
}
