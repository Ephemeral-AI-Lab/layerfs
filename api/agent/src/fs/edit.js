// fs.edit: one positional SDK operation.
import { checkedMessage } from "../client.js";
export function edit(client, branch, path, oldText, newText, message) {
  return client.call("fs.edit", { branch, path, oldText, newText, message: checkedMessage(message) });
}
