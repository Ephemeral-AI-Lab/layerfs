// fs.write: one positional SDK operation. Text is `content`; anything else
// is `bytes` (see body.js). The last argument is a message or
// `{ message, mode }`.
import { writeOptions } from "../client.js";
import { body } from "./body.js";
export function write(client, branch, path, content, options) {
  return client.call("fs.write", { branch, path, ...(typeof content === "string" ? { content } : { bytes: body(content) }), ...writeOptions(options) });
}
