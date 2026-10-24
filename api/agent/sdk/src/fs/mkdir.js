// fs.mkdir: one positional SDK operation.
import { checkedMessage } from "../client.js";
export function mkdir(client, branch, path, message) {
  return client.call("fs.mkdir", { branch, path, recursive: true, message: checkedMessage(message) });
}
