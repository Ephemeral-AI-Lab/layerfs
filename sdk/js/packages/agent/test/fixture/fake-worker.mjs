// A stand-in LayerFS worker for tests: the worker's frames on loopback, its
// secret check and ping, and `answer(request)` for every call (default: an
// empty page). `calls` lists the requests; `endpoint(file, home)` writes an
// endpoint file for it. No addon, Store or Docker.
import { randomBytes } from "node:crypto";
import { writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { frame, readFrames } from "../../src/transport/frames.js";

export async function fakeWorker(answer = () => ({ value: { items: [], hasMore: false } })) {
  const secret = randomBytes(32).toString("hex"), calls = [], sockets = new Set();
  const server = createServer((socket) => {
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
    socket.on("error", () => {});
    let trusted = false;
    readFrames(socket, (request) => {
      if (!trusted) {
        if (request?.secret !== secret) return socket.destroy();
        trusted = true;
        return socket.write(frame({ id: 0, answer: { value: "hello" } }));
      }
      if (request.fn === "ping") return socket.write(frame({ id: request.id, answer: { value: { ready: true, active: 0, stores: 0, build: "fake" } } }));
      calls.push(request);
      socket.write(frame({ id: request.id, answer: answer(request) }));
    }, () => socket.destroy());
  });
  server.unref();
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const port = server.address().port;
  return {
    port, secret, calls,
    endpoint(file, home) { writeFileSync(file, JSON.stringify({ home, port, secret, build: "fake" })); },
    close() { for (const socket of sockets) socket.destroy(); server.close(); },
  };
}
