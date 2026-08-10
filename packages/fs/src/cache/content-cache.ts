import { bytesToHex, copyBytes } from "../cas/bytes.js";
import { AdmissionController } from "../resources/limits.js";

export type ContentCacheKind = "object" | "manifest-root" | "manifest-node";
interface Entry {
  readonly bytes: Uint8Array;
  readonly weight: number;
  readonly release: () => void;
}
export interface ContentCacheMetrics {
  readonly bytes: number;
  readonly highWaterBytes: number;
  readonly hits: number;
  readonly misses: number;
  readonly admissions: number;
  readonly bypasses: number;
  readonly evictions: number;
}
export interface ContentCacheReservation {
  readonly weight: number;
  release(): void;
}

export class ContentCache {
  readonly #limit: number;
  readonly #admission: AdmissionController;
  readonly #entries = new Map<string, Entry>();
  #bytes = 0;
  #highWater = 0;
  #hits = 0;
  #misses = 0;
  #admissions = 0;
  #bypasses = 0;
  #evictions = 0;
  constructor(limitBytes: number, admission: AdmissionController) {
    if (!Number.isSafeInteger(limitBytes) || limitBytes <= 0)
      throw new RangeError("cache limit must be a positive safe integer");
    this.#limit = limitBytes;
    this.#admission = admission;
  }
  get(kind: ContentCacheKind, hash: Uint8Array): Uint8Array | undefined {
    const key = `${kind}:${bytesToHex(hash)}`;
    const entry = this.#entries.get(key);
    if (!entry) {
      this.#misses += 1;
      return undefined;
    }
    this.#entries.delete(key);
    this.#entries.set(key, entry);
    this.#hits += 1;
    return copyBytes(entry.bytes);
  }
  reserve(weight: number): ContentCacheReservation | undefined {
    if (!Number.isSafeInteger(weight) || weight <= 0)
      throw new RangeError("cache reservation must be positive");
    if (weight > this.#limit) {
      this.#bypasses += 1;
      return undefined;
    }
    while (this.#bytes + weight > this.#limit && this.#entries.size)
      this.#evictOldest();
    let release: (() => void) | undefined;
    while (!release) {
      try {
        release = this.#admission.reserve(weight);
      } catch {
        if (!this.#entries.size)
          throw new RangeError(
            "managed resident memory limit cannot admit a required content value",
          );
        this.#evictOldest();
      }
    }
    let active = true;
    return Object.freeze({
      weight,
      release() {
        if (active) {
          active = false;
          release!();
        }
      },
    });
  }
  admit(
    kind: ContentCacheKind,
    hash: Uint8Array,
    bytes: Uint8Array,
    reservation: ContentCacheReservation,
  ): void {
    const key = `${kind}:${bytesToHex(hash)}`;
    const existing = this.#entries.get(key);
    if (existing) {
      reservation.release();
      this.#entries.delete(key);
      this.#entries.set(key, existing);
      return;
    }
    const entry: Entry = Object.freeze({
      bytes: copyBytes(bytes),
      weight: reservation.weight,
      release: reservation.release,
    });
    this.#entries.set(key, entry);
    this.#bytes += entry.weight;
    this.#highWater = Math.max(this.#highWater, this.#bytes);
    this.#admissions += 1;
  }
  makeRoom(additionalBytes: number): void {
    if (!Number.isSafeInteger(additionalBytes) || additionalBytes < 0)
      throw new RangeError(
        "additional cache pressure must be a nonnegative safe integer",
      );
    while (
      this.#admission.usedBytes + additionalBytes > this.#admission.limitBytes &&
      this.#entries.size
    )
      this.#evictOldest();
    if (this.#admission.usedBytes + additionalBytes > this.#admission.limitBytes)
      throw new RangeError(
        "managed resident memory limit cannot admit the requested operation",
      );
  }
  clear(): void {
    for (const entry of this.#entries.values()) entry.release();
    this.#entries.clear();
    this.#bytes = 0;
  }
  metrics(): ContentCacheMetrics {
    return Object.freeze({
      bytes: this.#bytes,
      highWaterBytes: this.#highWater,
      hits: this.#hits,
      misses: this.#misses,
      admissions: this.#admissions,
      bypasses: this.#bypasses,
      evictions: this.#evictions,
    });
  }
  #evictOldest(): void {
    const first = this.#entries.entries().next().value as [string, Entry] | undefined;
    if (!first) return;
    this.#entries.delete(first[0]);
    this.#bytes -= first[1].weight;
    first[1].release();
    this.#evictions += 1;
  }
}
