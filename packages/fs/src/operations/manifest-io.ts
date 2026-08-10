import { intrinsicByteRange } from "../cas/bytes.js";
import { checkedAdd, checkedInteger } from "../resources/safe-integers.js";
import type { RuntimeLimits, StorageLimits } from "../resources/limits.js";
import { AdmissionController } from "../resources/limits.js";
import { prepareContentStreaming } from "./streaming-prepare.js";
import type {
  ClosureCertificate,
  ContentStore,
  OperationsStorage,
} from "./storage-ports.js";
import type { ContentCache } from "../cache/content-cache.js";

export interface PreparedManifest {
  readonly hash: Uint8Array;
  readonly size: number;
  readonly certificate: ClosureCertificate;
}

export async function prepareContent(
  port: OperationsStorage,
  input: Uint8Array | ReadableStream<Uint8Array>,
  storage: StorageLimits,
  runtime: RuntimeLimits,
  admission: AdmissionController,
  signal?: AbortSignal,
  cache?: ContentCache,
  clock?: () => number,
  declaredMaxBytes?: number,
): Promise<PreparedManifest> {
  return prepareContentStreaming(
    port,
    input,
    storage,
    runtime,
    admission,
    signal,
    cache,
    clock,
    declaredMaxBytes,
  );
}

export function readManifestRange(
  repository: ContentStore,
  manifestHash: Uint8Array,
  offset: number,
  length: number,
  admission: AdmissionController,
  cache?: ContentCache,
): Uint8Array {
  checkedInteger(offset, "manifest read offset");
  checkedInteger(length, "manifest read length");
  const cursor = repository.openManifestCursor(manifestHash, offset);
  const outputLength = Math.max(0, Math.min(length, cursor.fileSize - offset));
  cache?.makeRoom(outputLength);
  const releaseOutput = admission.reserve(outputLength);
  try {
    const output = new Uint8Array(outputLength);
    const written = cursor.readInto(output, 0, output.byteLength);
    if (written !== output.byteLength)
      throw new Error("ECORRUPT: authenticated manifest range ended early");
    return output;
  } finally {
    releaseOutput();
  }
}

export function readManifestInto(
  repository: ContentStore,
  manifestHash: Uint8Array,
  position: number,
  destination: Uint8Array,
  destinationOffset: number,
  length: number,
): number {
  destination = intrinsicByteRange(destination);
  checkedInteger(position, "manifest read position");
  checkedInteger(destinationOffset, "manifest destination offset");
  checkedInteger(length, "manifest read length");
  if (checkedAdd(destinationOffset, length) > destination.byteLength)
    throw new RangeError("invalid direct manifest read range");
  const cursor = repository.openManifestCursor(manifestHash, position);
  return cursor.readInto(destination, destinationOffset, length);
}
