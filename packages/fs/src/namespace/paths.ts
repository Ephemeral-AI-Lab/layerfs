import { decodeUtf8, encodeUtf8 } from "./utf8.js";
import { fsError } from "../filesystem/errors.js";
import type { FilesystemLimits } from "../resources/limits.js";

export interface CanonicalPath {
  readonly value: string;
  readonly segments: readonly string[];
  readonly encodedSegments: readonly Uint8Array[];
}

function validateUnicode(value: string, syscall: string, path?: string): void {
  for (let index = 0; index < value.length; index += 1) {
    const unit = value.charCodeAt(index);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = value.charCodeAt(++index);
      if (!(next >= 0xdc00 && next <= 0xdfff))
        throw fsError("EINVAL", syscall, path, "ill-formed Unicode");
    } else if (unit >= 0xdc00 && unit <= 0xdfff)
      throw fsError("EINVAL", syscall, path, "ill-formed Unicode");
  }
}

export function canonicalizePath(
  input: string,
  limits: FilesystemLimits,
  syscall: string,
): CanonicalPath {
  if (
    typeof input !== "string" ||
    input.length === 0 ||
    !input.startsWith("/") ||
    input.includes("\0")
  )
    throw fsError(
      "EINVAL",
      syscall,
      typeof input === "string" ? input : undefined,
      "path must be absolute, nonempty, and contain no NUL",
    );
  validateUnicode(input, syscall, input);
  const segments: string[] = [];
  for (const segment of input.split("/")) {
    if (!segment || segment === ".") continue;
    if (segment === "..") {
      if (!segments.length)
        throw fsError("EINVAL", syscall, input, "path escapes above root");
      segments.pop();
    } else segments.push(segment);
  }
  const value = segments.length ? `/${segments.join("/")}` : "/";
  const encodedSegments = segments.map((segment) => encodeUtf8(segment));
  if (
    encodeUtf8(value).byteLength > limits.maxPathBytes ||
    encodedSegments.some((bytes) => bytes.byteLength > limits.maxNameBytes)
  )
    throw fsError("EINVAL", syscall, value, "path exceeds configured UTF-8 limits");
  return Object.freeze({
    value,
    segments: Object.freeze(segments),
    encodedSegments: Object.freeze(encodedSegments),
  });
}

export function validateName(
  name: string,
  limits: FilesystemLimits,
  syscall: string,
): Uint8Array {
  if (
    !name ||
    name === "." ||
    name === ".." ||
    name.includes("/") ||
    name.includes("\0")
  )
    throw fsError("EINVAL", syscall, name, "invalid directory entry name");
  validateUnicode(name, syscall, name);
  const bytes = encodeUtf8(name);
  if (bytes.byteLength > limits.maxNameBytes)
    throw fsError("EINVAL", syscall, name, "name exceeds configured limit");
  return bytes;
}

export function validateSymlinkTarget(
  target: string,
  limits: FilesystemLimits,
  syscall: string,
): void {
  if (typeof target !== "string" || !target || target.includes("\0"))
    throw fsError("EINVAL", syscall, target, "invalid symbolic link target");
  validateUnicode(target, syscall, target);
  if (encodeUtf8(target).byteLength > limits.maxSymlinkTargetBytes)
    throw fsError(
      "EINVAL",
      syscall,
      target,
      "symbolic link target exceeds configured limit",
    );
}

export function compareUtf8(left: string, right: string): number {
  const a = encodeUtf8(left);
  const b = encodeUtf8(right);
  const length = Math.min(a.length, b.length);
  for (let index = 0; index < length; index += 1)
    if (a[index] !== b[index]) return a[index]! - b[index]!;
  return a.length - b.length;
}

export function assertCanonicalNameBytes(name: string, bytes: Uint8Array): void {
  if (decodeUtf8(bytes) !== name)
    throw new Error("ECORRUPT: directory name and sort key differ");
}
