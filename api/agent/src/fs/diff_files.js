// fs.diff_files: one positional SDK operation.
export function diff_files(client, leftRef, leftPath, rightRef, rightPath) {
  return client.call("fs.diff_files", { branch: leftRef, left: { commit: leftRef, path: leftPath }, right: { commit: rightRef, path: rightPath } });
}
