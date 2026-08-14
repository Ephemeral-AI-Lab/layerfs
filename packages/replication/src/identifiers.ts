import { ReplicationError } from "./errors.js";
import { bytesToLowerHex } from "./sha256.js";

export type ReplicationRandomFill = (target: Uint8Array) => void;

export function validateReplicationSessionId(value: string): string {
  if (typeof value !== "string" || !/^[0-9a-f]{32}$/u.test(value))
    throw new ReplicationError(
      "ProtocolMismatch",
      "replication session id must be 128 bits encoded as 32 lowercase hex digits",
    );
  return value;
}

export function generateReplicationSessionId(fill?: ReplicationRandomFill): string {
  const bytes = new Uint8Array(16);
  if (fill) fill(bytes);
  else {
    const source = globalThis.crypto;
    if (!source)
      throw new ReplicationError(
        "ResourceLimit",
        "a cryptographic random source is unavailable",
      );
    source.getRandomValues(bytes);
  }
  return validateReplicationSessionId(bytesToLowerHex(bytes));
}
