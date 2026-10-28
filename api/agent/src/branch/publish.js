// branch.publish: one positional SDK operation.
export function publish(client, branch, commit) {
  return client.call("branch.publish", { branch, commit });
}
