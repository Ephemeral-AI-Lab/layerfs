// fs.read_bytes: one bounded window of a file's raw bytes at a saved snapshot.
export function read_bytes(client, ref, path, offset = 0, length) {
  return client.call("fs.read_bytes", { branch: ref, path, offset, length });
}
