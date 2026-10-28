// branch.fork: one positional SDK operation.
export function fork(client, name, fromRef = "main") {
  return client.call("branch.fork", { name, from: fromRef });
}
