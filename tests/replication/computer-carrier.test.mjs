import assert from "node:assert/strict";
import { test } from "node:test";
import {
  admitComputerEfsCarrierV1,
  COMPUTER_EFS_CARRIER_V1_RESOURCES,
  computerEfsCarrierV1Stats,
  ReplicationError,
  validateComputerEfsCarrierV1,
} from "../../packages/replication/dist/index.js";

const MIB = 1024 * 1024;

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

const maximumLimits = Object.freeze({
  hostProfile: "computer-efs-carrier-v1",
  maxRequestBytes: 3 * MIB,
  maxResponseBytes: 3 * MIB,
  maxInFlightBatches: 1,
  maxMutatingAcknowledgementBytes: 64 * 1024,
  compression: false,
});

test("computer carrier profile freezes the 17.25 MiB reservation", () => {
  assert.equal(COMPUTER_EFS_CARRIER_V1_RESOURCES.maxReservationBytes, 17.25 * MIB);
  assert.equal(COMPUTER_EFS_CARRIER_V1_RESOURCES.processPoolBytes, 20 * MIB);
  assert.equal(COMPUTER_EFS_CARRIER_V1_RESOURCES.maxRawFrameBytes, 4 * MIB + 64 * 1024);
  assert.equal(
    validateComputerEfsCarrierV1(maximumLimits).reservationBytes,
    17.25 * MIB,
  );
  for (const limits of [
    { ...maximumLimits, maxRequestBytes: 3 * MIB + 1 },
    { ...maximumLimits, maxInFlightBatches: 2 },
    { ...maximumLimits, compression: true },
    { ...maximumLimits, maxMutatingAcknowledgementBytes: 64 * 1024 + 1 },
  ])
    assert.throws(
      () => validateComputerEfsCarrierV1(limits),
      (error) =>
        error instanceof ReplicationError && error.code === "IncompatibleLimit",
    );
});

test("process-global admission is strict FIFO and occurs before endpoint construction", async () => {
  const opened = [];
  const first = await admitComputerEfsCarrierV1({
    limits: maximumLimits,
    openEndpoint() {
      opened.push("first");
      return {
        async exchange(request) {
          return request;
        },
      };
    },
  });
  const secondPromise = admitComputerEfsCarrierV1({
    limits: maximumLimits,
    openEndpoint() {
      opened.push("second");
      return {
        async exchange(request) {
          return request;
        },
      };
    },
  });
  await Promise.resolve();
  assert.deepEqual(opened, ["first"]);
  assert.deepEqual(computerEfsCarrierV1Stats(), {
    reservedBytes: 17.25 * MIB,
    queued: 1,
  });
  await first.close();
  const second = await secondPromise;
  assert.deepEqual(opened, ["first", "second"]);
  await second.close();
  assert.deepEqual(computerEfsCarrierV1Stats(), { reservedBytes: 0, queued: 0 });
});

test("queued admission aborts without constructing an endpoint", async () => {
  const first = await admitComputerEfsCarrierV1({
    limits: maximumLimits,
    openEndpoint: () => ({
      async exchange(request) {
        return request;
      },
    }),
  });
  const controller = new AbortController();
  let opened = false;
  const queued = admitComputerEfsCarrierV1({
    limits: maximumLimits,
    signal: controller.signal,
    openEndpoint() {
      opened = true;
      return {
        async exchange(request) {
          return request;
        },
      };
    },
  });
  controller.abort();
  await assert.rejects(
    queued,
    (error) => error instanceof ReplicationError && error.code === "Aborted",
  );
  assert.equal(opened, false);
  await first.close();
  assert.deepEqual(computerEfsCarrierV1Stats(), { reservedBytes: 0, queued: 0 });
});

test("admitted target exposes only exchange, bounds bytes, and close waits active", async () => {
  const response = deferred();
  let closes = 0;
  const admitted = await admitComputerEfsCarrierV1({
    limits: maximumLimits,
    openEndpoint: () => ({
      async exchange() {
        return response.promise;
      },
      async close() {
        closes += 1;
      },
    }),
  });
  assert.deepEqual(Object.keys(admitted.target), ["exchange"]);
  const active = admitted.target.exchange(new Uint8Array());
  await assert.rejects(
    admitted.target.exchange(new Uint8Array()),
    (error) => error instanceof ReplicationError && error.code === "Busy",
  );
  let closed = false;
  const close = admitted.close().then(() => {
    closed = true;
  });
  await Promise.resolve();
  assert.equal(closed, false);
  response.resolve(new Uint8Array());
  await active;
  await close;
  await admitted.close();
  assert.equal(closes, 1);
  await assert.rejects(
    admitted.target.exchange(new Uint8Array()),
    (error) => error instanceof ReplicationError && error.code === "Closed",
  );
  assert.deepEqual(computerEfsCarrierV1Stats(), { reservedBytes: 0, queued: 0 });
});

test("carrier maps endpoint failures and enforces decoded response bounds", async () => {
  const failing = await admitComputerEfsCarrierV1({
    limits: maximumLimits,
    openEndpoint: () => ({
      async exchange() {
        throw new Error("private failure");
      },
    }),
  });
  await assert.rejects(
    failing.target.exchange(new Uint8Array()),
    (error) => error instanceof ReplicationError && error.code === "TransportFailure",
  );
  await failing.close();

  const oversized = await admitComputerEfsCarrierV1({
    limits: { ...maximumLimits, maxResponseBytes: 1 },
    openEndpoint: () => ({
      async exchange() {
        return new Uint8Array(2);
      },
    }),
  });
  await assert.rejects(
    oversized.target.exchange(new Uint8Array()),
    (error) => error instanceof ReplicationError && error.code === "ResourceLimit",
  );
  await oversized.close();
  assert.deepEqual(computerEfsCarrierV1Stats(), { reservedBytes: 0, queued: 0 });
});

test("endpoint-open and close faults release process admission exactly once", async () => {
  await assert.rejects(
    admitComputerEfsCarrierV1({
      limits: maximumLimits,
      openEndpoint() {
        throw new Error("open fault");
      },
    }),
    (error) => error instanceof ReplicationError && error.code === "TransportFailure",
  );
  assert.deepEqual(computerEfsCarrierV1Stats(), { reservedBytes: 0, queued: 0 });

  const admitted = await admitComputerEfsCarrierV1({
    limits: maximumLimits,
    openEndpoint: () => ({
      async exchange(request) {
        return request;
      },
      close() {
        throw new Error("close fault");
      },
    }),
  });
  await assert.rejects(
    admitted.close(),
    (error) => error instanceof ReplicationError && error.code === "TransportFailure",
  );
  await assert.rejects(admitted.close(), /replication carrier close failed/);
  assert.deepEqual(computerEfsCarrierV1Stats(), { reservedBytes: 0, queued: 0 });
});
