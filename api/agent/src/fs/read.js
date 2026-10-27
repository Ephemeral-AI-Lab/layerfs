// fs.read: one positional SDK operation.
export function read(client, ref, path, startLine = 1, endLine) {
  return client.call("fs.read", { branch: ref, path, startLine, endLine });
}
