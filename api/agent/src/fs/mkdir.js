// fs.mkdir: one positional SDK operation. The last argument is a message or
// `{ message, mode }`; `mode` applies to `path` when it is created.
import { writeOptions } from "../client.js";
export function mkdir(client, branch, path, options) {
  return client.call("fs.mkdir", { branch, path, recursive: true, ...writeOptions(options) });
}
