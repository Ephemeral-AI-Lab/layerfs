// fs.write: one positional SDK operation. Text is `content`; anything else
// is `bytes` (see body.js).
import { checkedMessage } from "../client.js";
import { body } from "./body.js";
export function write(client, branch, path, content, message) {
  return client.call("fs.write", { branch, path, ...(typeof content === "string" ? { content } : { bytes: body(content) }), message: checkedMessage(message) });
}
