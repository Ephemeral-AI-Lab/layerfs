import { bytesToHex } from "../cas/bytes.js";
import { sha256 } from "../cas/sha256.js";
import { fastCdcChunks, type FastCdcConfiguration } from "../cdc/fastcdc.js";
import { buildManifestFromEntries, type BuiltManifest } from "../manifests/builder.js";

export function buildManifest(bytes: Uint8Array, parameters: FastCdcConfiguration): BuiltManifest & { readonly objects: ReadonlyMap<string, Uint8Array> } {
  const objects = new Map<string, Uint8Array>();
  const entries = fastCdcChunks(bytes, parameters).map(({ offset, length }) => {
    const object = bytes.slice(offset, offset + length);
    const hash = sha256(object);
    objects.set(bytesToHex(hash), object);
    return Object.freeze({ hash, length });
  });
  return Object.freeze({ ...buildManifestFromEntries(entries, parameters), objects });
}
