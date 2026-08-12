import { utf8ByteLength } from "../namespace/utf8.js";

export const MAX_DURABLE_IDENTIFIER_BYTES = 128;
export const MAX_BRANCH_IDENTIFIER_BYTES = 200;
export const MAX_OPERATION_IDENTIFIER_BYTES = 200;
export const MAX_ROOT_JOURNAL_IDENTIFIER_BYTES = 256;

export function validateBoundedIdentifier(
  value: string,
  label: string,
  maximumBytes: number,
): void {
  if (typeof value !== "string" || value.length === 0 || value.includes("\0"))
    throw new RangeError(`${label} is invalid`);
  if (!Number.isSafeInteger(maximumBytes) || maximumBytes <= 0)
    throw new RangeError("identifier byte limit is invalid");
  if (utf8ByteLength(value) > maximumBytes)
    throw new RangeError(`${label} exceeds ${maximumBytes} UTF-8 bytes`);
}

export function validateDurableIdentifier(value: string, label: string): void {
  const maximum =
    label === "branch identifier"
      ? MAX_BRANCH_IDENTIFIER_BYTES
      : label === "operation identifier"
        ? MAX_OPERATION_IDENTIFIER_BYTES
        : label === "root journal identifier"
          ? MAX_ROOT_JOURNAL_IDENTIFIER_BYTES
          : label === "revision writer identifier"
            ? MAX_ROOT_JOURNAL_IDENTIFIER_BYTES
            : MAX_DURABLE_IDENTIFIER_BYTES;
  validateBoundedIdentifier(value, label, maximum);
}

export function validateBranchIdentifier(value: string, label = "branch identifier") {
  validateBoundedIdentifier(value, label, MAX_BRANCH_IDENTIFIER_BYTES);
}

export function validateOperationIdentifier(
  value: string,
  label = "operation identifier",
): void {
  validateBoundedIdentifier(value, label, MAX_OPERATION_IDENTIFIER_BYTES);
}
