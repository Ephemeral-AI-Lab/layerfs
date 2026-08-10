export function encodeUtf8(value: string): Uint8Array { return new TextEncoder().encode(value); }
export function decodeUtf8(value: Uint8Array): string { return new TextDecoder("utf-8", { fatal: true }).decode(value); }
