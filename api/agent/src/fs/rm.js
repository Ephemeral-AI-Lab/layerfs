// fs.rm: one positional SDK operation.
import { checkedMessage } from "../client.js";
export function rm(client, branch, path, message) {
  return client.call("fs.rm", { branch, path, recursive: true, message: checkedMessage(message) });
}
