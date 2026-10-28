// fs.blame: one positional SDK operation.
export function blame(client, ref, path, fromLine = 1) {
  return client.call("fs.blame", { branch: ref, path, fromLine });
}
