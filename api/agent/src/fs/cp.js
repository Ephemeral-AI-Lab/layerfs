// fs.cp: one positional SDK operation.
import { checkedMessage } from "../client.js";
export function cp(client, branch, fromPath, toPath, message) {
  return client.call("fs.cp", { branch, from: fromPath, to: toPath, recursive: true, message: checkedMessage(message) });
}
