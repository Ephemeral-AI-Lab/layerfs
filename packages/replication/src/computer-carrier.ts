import { ReplicationError } from "./errors.js";
import { REPLICATION_HOST_PROFILE } from "./types.js";

const KIB = 1024;
const MIB = 1024 * KIB;
const MAX_DECODED_BYTES = 3 * MIB;
const RPC_FRAMING_BYTES = 64 * KIB;
const MAX_BASE64_BYTES = 4 * MIB;
const MAX_RAW_FRAME_BYTES = MAX_BASE64_BYTES + RPC_FRAMING_BYTES;
const MAX_UTF16_BYTES = 2 * MAX_RAW_FRAME_BYTES;
const MAX_ACKNOWLEDGEMENT_BYTES = 64 * KIB;
const MAX_SCRATCH_BYTES = 2 * MIB;
const PROCESS_POOL_BYTES = 20 * MIB;

function rawFrameBytes(decodedBytes: number): number {
  return Math.ceil(decodedBytes / 3) * 4 + RPC_FRAMING_BYTES;
}

function reservationBytes(decodedBytes: number): number {
  const rawBytes = rawFrameBytes(decodedBytes);
  return (
    rawBytes +
    2 * rawBytes +
    decodedBytes +
    MAX_ACKNOWLEDGEMENT_BYTES +
    MAX_SCRATCH_BYTES
  );
}

export const COMPUTER_EFS_CARRIER_V1_RESOURCES = Object.freeze({
  hostProfile: REPLICATION_HOST_PROFILE,
  maxDecodedEnvelopeBytes: MAX_DECODED_BYTES,
  maxBase64Bytes: MAX_BASE64_BYTES,
  rpcFramingBytes: RPC_FRAMING_BYTES,
  maxRawFrameBytes: MAX_RAW_FRAME_BYTES,
  maxUtf16Bytes: MAX_UTF16_BYTES,
  maxMutatingAcknowledgementBytes: MAX_ACKNOWLEDGEMENT_BYTES,
  maxScratchBytes: MAX_SCRATCH_BYTES,
  processPoolBytes: PROCESS_POOL_BYTES,
  maxReservationBytes: reservationBytes(MAX_DECODED_BYTES),
  maxInFlightExchanges: 1,
  compression: false,
});

export interface ComputerEfsCarrierV1Limits {
  readonly hostProfile?: typeof REPLICATION_HOST_PROFILE;
  readonly maxRequestBytes: number;
  readonly maxResponseBytes: number;
  readonly maxInFlightBatches?: number;
  readonly maxMutatingAcknowledgementBytes?: number;
  readonly compression?: false;
}

export interface ValidatedComputerEfsCarrierV1 {
  readonly hostProfile: typeof REPLICATION_HOST_PROFILE;
  readonly maxRequestBytes: number;
  readonly maxResponseBytes: number;
  readonly maxInFlightBatches: 1;
  readonly maxMutatingAcknowledgementBytes: number;
  readonly compression: false;
  readonly reservationBytes: number;
}

function positiveSafeInteger(value: number, name: string): number {
  if (!Number.isSafeInteger(value) || value <= 0)
    throw new ReplicationError(
      "IncompatibleLimit",
      `${name} must be a positive safe integer`,
    );
  return value;
}

export function validateComputerEfsCarrierV1(
  input: ComputerEfsCarrierV1Limits,
): Readonly<ValidatedComputerEfsCarrierV1> {
  if (input.hostProfile !== undefined && input.hostProfile !== REPLICATION_HOST_PROFILE)
    throw new ReplicationError(
      "CapabilityMismatch",
      "carrier host profile is not computer-efs-carrier-v1",
    );
  const maxRequestBytes = positiveSafeInteger(input.maxRequestBytes, "maxRequestBytes");
  const maxResponseBytes = positiveSafeInteger(
    input.maxResponseBytes,
    "maxResponseBytes",
  );
  if (maxRequestBytes > MAX_DECODED_BYTES || maxResponseBytes > MAX_DECODED_BYTES)
    throw new ReplicationError(
      "IncompatibleLimit",
      "decoded envelope limit exceeds computer-efs-carrier-v1",
    );
  if (input.maxInFlightBatches !== undefined && input.maxInFlightBatches !== 1)
    throw new ReplicationError(
      "IncompatibleLimit",
      "computer-efs-carrier-v1 permits exactly one in-flight exchange",
    );
  if (input.compression !== undefined && input.compression !== false)
    throw new ReplicationError(
      "IncompatibleLimit",
      "computer-efs-carrier-v1 disables compression",
    );
  const maxMutatingAcknowledgementBytes = positiveSafeInteger(
    input.maxMutatingAcknowledgementBytes ?? MAX_ACKNOWLEDGEMENT_BYTES,
    "maxMutatingAcknowledgementBytes",
  );
  if (maxMutatingAcknowledgementBytes > MAX_ACKNOWLEDGEMENT_BYTES)
    throw new ReplicationError(
      "IncompatibleLimit",
      "mutating acknowledgement exceeds computer-efs-carrier-v1",
    );
  const reservation = reservationBytes(Math.max(maxRequestBytes, maxResponseBytes));
  if (reservation > PROCESS_POOL_BYTES)
    throw new ReplicationError(
      "IncompatibleLimit",
      "carrier reservation exceeds the process pool",
    );
  return Object.freeze({
    hostProfile: REPLICATION_HOST_PROFILE,
    maxRequestBytes,
    maxResponseBytes,
    maxInFlightBatches: 1,
    maxMutatingAcknowledgementBytes,
    compression: false,
    reservationBytes: reservation,
  });
}

interface AdmissionWaiter {
  readonly bytes: number;
  readonly resolve: (release: () => void) => void;
  readonly reject: (reason: ReplicationError) => void;
  readonly signal: AbortSignal | undefined;
  readonly abort: () => void;
  settled: boolean;
}

interface ProcessAdmissionPool {
  reservedBytes: number;
  readonly waiters: AdmissionWaiter[];
}

const pool: ProcessAdmissionPool = { reservedBytes: 0, waiters: [] };

function releaseOnce(bytes: number): () => void {
  let released = false;
  return () => {
    if (released) return;
    released = true;
    pool.reservedBytes -= bytes;
    if (pool.reservedBytes < 0) {
      pool.reservedBytes = 0;
      throw new Error("replication carrier admission accounting underflow");
    }
    drainAdmissions();
  };
}

function settleAdmission(waiter: AdmissionWaiter): void {
  waiter.settled = true;
  waiter.signal?.removeEventListener("abort", waiter.abort);
}

function drainAdmissions(): void {
  while (pool.waiters[0]) {
    const waiter = pool.waiters[0];
    if (!waiter) return;
    if (waiter.settled) {
      pool.waiters.shift();
      continue;
    }
    if (pool.reservedBytes + waiter.bytes > PROCESS_POOL_BYTES) return;
    pool.waiters.shift();
    settleAdmission(waiter);
    pool.reservedBytes += waiter.bytes;
    waiter.resolve(releaseOnce(waiter.bytes));
  }
}

function reserve(bytes: number, signal?: AbortSignal): Promise<() => void> {
  if (signal?.aborted)
    return Promise.reject(
      new ReplicationError("Aborted", "carrier admission was aborted"),
    );
  if (pool.waiters.length === 0 && pool.reservedBytes + bytes <= PROCESS_POOL_BYTES) {
    pool.reservedBytes += bytes;
    return Promise.resolve(releaseOnce(bytes));
  }
  return new Promise<() => void>((resolve, reject) => {
    const waiter: AdmissionWaiter = {
      bytes,
      resolve,
      reject,
      signal,
      settled: false,
      abort: () => {
        if (waiter.settled) return;
        settleAdmission(waiter);
        const index = pool.waiters.indexOf(waiter);
        if (index >= 0) pool.waiters.splice(index, 1);
        reject(new ReplicationError("Aborted", "carrier admission was aborted"));
        drainAdmissions();
      },
    };
    signal?.addEventListener("abort", waiter.abort, { once: true });
    pool.waiters.push(waiter);
  });
}

export function computerEfsCarrierV1Stats(): Readonly<{
  reservedBytes: number;
  queued: number;
}> {
  return Object.freeze({
    reservedBytes: pool.reservedBytes,
    queued: pool.waiters.filter((waiter) => !waiter.settled).length,
  });
}

export interface ComputerEfsCarrierV1Endpoint {
  exchange(request: Uint8Array): Promise<Uint8Array>;
  close?(): void | Promise<void>;
}

export interface ComputerEfsCarrierV1RpcTarget {
  exchange(request: Uint8Array): Promise<Uint8Array>;
}

export interface AdmittedComputerEfsCarrierV1 extends AsyncDisposable {
  readonly target: Readonly<ComputerEfsCarrierV1RpcTarget>;
  readonly limits: Readonly<ValidatedComputerEfsCarrierV1>;
  close(): Promise<void>;
}

function transportFailure(error: unknown, action: string): ReplicationError {
  if (error instanceof ReplicationError) return error;
  return new ReplicationError("TransportFailure", `${action} failed`, {
    cause: error,
  });
}

class ComputerEfsCarrierV1Admission implements AdmittedComputerEfsCarrierV1 {
  readonly target: Readonly<ComputerEfsCarrierV1RpcTarget>;
  readonly limits: Readonly<ValidatedComputerEfsCarrierV1>;
  readonly #endpoint: ComputerEfsCarrierV1Endpoint;
  readonly #release: () => void;
  #activeDone: Promise<void> | undefined;
  #closePromise: Promise<void> | undefined;
  #closed = false;

  constructor(
    endpoint: ComputerEfsCarrierV1Endpoint,
    limits: Readonly<ValidatedComputerEfsCarrierV1>,
    release: () => void,
  ) {
    this.#endpoint = endpoint;
    this.limits = limits;
    this.#release = release;
    this.target = Object.freeze({
      exchange: (request: Uint8Array) => this.#exchange(request),
    });
  }

  async #exchange(request: Uint8Array): Promise<Uint8Array> {
    if (this.#closed)
      throw new ReplicationError("Closed", "replication carrier is closed");
    if (this.#activeDone)
      throw new ReplicationError(
        "Busy",
        "one replication exchange is already in flight",
      );
    if (!(request instanceof Uint8Array))
      throw new ReplicationError(
        "ProtocolMismatch",
        "replication request must be Uint8Array",
      );
    if (request.byteLength > this.limits.maxRequestBytes)
      throw new ReplicationError(
        "ResourceLimit",
        "decoded replication request exceeds its negotiated limit",
      );
    let finish!: () => void;
    this.#activeDone = new Promise<void>((resolve) => {
      finish = resolve;
    });
    try {
      const response = await this.#endpoint.exchange(request);
      if (!(response instanceof Uint8Array))
        throw new ReplicationError(
          "ProtocolMismatch",
          "replication response must be Uint8Array",
        );
      if (response.byteLength > this.limits.maxResponseBytes)
        throw new ReplicationError(
          "ResourceLimit",
          "decoded replication response exceeds its negotiated limit",
        );
      return response;
    } catch (error) {
      throw transportFailure(error, "replication carrier exchange");
    } finally {
      finish();
      this.#activeDone = undefined;
    }
  }

  close(): Promise<void> {
    if (this.#closePromise) return this.#closePromise;
    this.#closed = true;
    this.#closePromise = (async () => {
      await this.#activeDone;
      try {
        await this.#endpoint.close?.();
      } catch (error) {
        throw transportFailure(error, "replication carrier close");
      } finally {
        this.#release();
      }
    })();
    return this.#closePromise;
  }

  async [Symbol.asyncDispose](): Promise<void> {
    await this.close();
  }
}

export async function admitComputerEfsCarrierV1(options: {
  readonly limits: ComputerEfsCarrierV1Limits;
  readonly signal?: AbortSignal;
  readonly openEndpoint: () =>
    ComputerEfsCarrierV1Endpoint | Promise<ComputerEfsCarrierV1Endpoint>;
}): Promise<AdmittedComputerEfsCarrierV1> {
  const limits = validateComputerEfsCarrierV1(options.limits);
  const release = await reserve(limits.reservationBytes, options.signal);
  let endpoint: ComputerEfsCarrierV1Endpoint | undefined;
  try {
    endpoint = await options.openEndpoint();
    if (
      !endpoint ||
      typeof endpoint !== "object" ||
      typeof endpoint.exchange !== "function"
    )
      throw new ReplicationError(
        "ProtocolMismatch",
        "carrier endpoint does not expose exchange(bytes)",
      );
    if (options.signal?.aborted)
      throw new ReplicationError("Aborted", "carrier admission was aborted");
    return new ComputerEfsCarrierV1Admission(endpoint, limits, release);
  } catch (error) {
    try {
      await endpoint?.close?.();
    } finally {
      release();
    }
    throw transportFailure(error, "replication carrier endpoint construction");
  }
}
