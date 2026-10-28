// fs.stat: one positional SDK operation.
export function stat(client, ref, path) {
  return client.call("fs.stat", { branch: ref, path });
}
