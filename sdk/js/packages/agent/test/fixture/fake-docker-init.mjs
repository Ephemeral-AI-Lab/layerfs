#!/usr/bin/env node
// A stand-in `docker` for the macOS Init container (worker_client.js
// initRun): reads the encoded call on stdin and answers Initialized, echoing
// the `timeoutMs` it received (`null` when the call sent no such field).
import { decode, encode } from "../../src/transport/frames.js";

const chunks = [];
for await (const chunk of process.stdin) chunks.push(chunk);
const { args } = decode(Buffer.concat(chunks));
const timeoutMs = Object.hasOwn(args, "timeoutMs") ? args.timeoutMs : null;
process.stdout.write(encode({ value: { kind: "Initialized", project: args.name, timeoutMs } }));
