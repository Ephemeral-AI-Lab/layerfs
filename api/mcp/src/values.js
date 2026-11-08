// Values across the MCP boundary, keeping what JSON cannot:
//   bytes    text, or { "base64": "..." } in and out (the Node package's form)
//   integers a whole number outside ±(2^53-1) is { "$layerfs": "int",
//            "value": "<exact decimal>" } in a result, and a decimal string in
//            an argument; a rounded JSON number is never produced or accepted
//   errors   { code, message, detail } in an isError tool result
//
// Argument shapes come from the table entry, not from this file: `readArguments`
// walks the entry's own args and visible options.

import { visibleOptions } from "./tools.js";

const WHOLE = /^-?[0-9]+$/;
const BASE64 = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;

/** A tool call whose arguments do not fit the table entry: a protocol error. */
export class InvalidArguments extends Error {
  constructor(message) {
    super(message);
    this.name = "InvalidArguments";
  }
}

const isPlain = (value) => value !== null && typeof value === "object" && !Array.isArray(value);

function refuse(at, why) {
  throw new InvalidArguments(`${at} ${why}`);
}

function whole(at, value) {
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value)) refuse(at, "must be a safe integer; send a decimal string for a larger whole number");
    return value;
  }
  if (typeof value === "string" && WHOLE.test(value)) return BigInt(value);
  refuse(at, "must be a whole number, or a decimal string");
}

function size(at, value) {
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value) || value < 0) refuse(at, "must be a non-negative safe integer, or text such as \"512M\"");
    return value;
  }
  if (typeof value === "string" && value.length > 0) return value;
  refuse(at, "must be a non-negative integer, or text such as \"512M\"");
}

function content(at, value) {
  if (typeof value === "string") return value;
  if (isPlain(value) && Object.keys(value).length === 1 && typeof value.base64 === "string") {
    if (!BASE64.test(value.base64)) refuse(at, "has a base64 field that is not base64");
    return Buffer.from(value.base64, "base64");
  }
  refuse(at, "must be text, or { \"base64\": \"...\" } for bytes");
}

/** One argument or option as the Node package takes it. */
function convert(spec, entry, at, value) {
  switch (entry.kind) {
    case "project":
    case "name":
    case "text":
      if (typeof value !== "string") refuse(at, "must be text");
      return value;
    case "int":
      return whole(at, value);
    case "bool":
      if (typeof value !== "boolean") refuse(at, "must be true or false");
      return value;
    case "size":
      return size(at, value);
    case "content":
      return content(at, value);
    case "json":
      return value;
    default:
      throw new Error(`${spec.family}.${spec.call}: no reader for kind "${entry.kind}"`);
  }
}

/** The table entry's arguments map, in positional order, and its options. */
function known(spec) {
  const entries = new Map();
  for (const arg of spec.args) entries.set(arg.name, { entry: arg, option: false });
  for (const option of visibleOptions(spec)) entries.set(option.name, { entry: option, option: true });
  return entries;
}

/**
 * `{ positionals, options }` for one call of `spec` from the protocol's
 * `arguments` object. Unknown names, missing required names and values of the
 * wrong kind are `InvalidArguments`; nothing is coerced silently.
 */
export function readArguments(spec, given) {
  const where = `${spec.family}.${spec.call}`;
  if (given === undefined || given === null) given = {};
  if (!isPlain(given)) throw new InvalidArguments(`${where}: arguments must be an object`);

  const entries = known(spec);
  for (const name of Object.keys(given)) {
    if (!entries.has(name)) throw new InvalidArguments(`${where}: unknown argument "${name}"`);
  }

  const positionals = [];
  const options = {};
  for (const arg of spec.args) {
    const value = given[arg.name];
    if (value === undefined || value === null) {
      if (!arg.optional) throw new InvalidArguments(`${where}: required argument "${arg.name}" is missing`);
      positionals.push(undefined);
    } else {
      positionals.push(convert(spec, arg, `${where}: argument "${arg.name}"`, value));
    }
  }
  for (const option of visibleOptions(spec)) {
    const value = given[option.name];
    if (value !== undefined && value !== null) {
      options[option.name] = convert(spec, option, `${where}: option "${option.name}"`, value);
    }
  }
  return { positionals, options };
}

/**
 * A result as JSON-safe data: a Buffer becomes `{ base64 }`, and a BigInt that
 * JSON cannot hold exactly becomes `{ "$layerfs": "int", value }`. Recurses.
 */
export function encodeResult(value) {
  if (typeof value === "bigint") return { $layerfs: "int", value: value.toString() };
  if (value instanceof Uint8Array) return { base64: Buffer.from(value).toString("base64") };
  if (Array.isArray(value)) return value.map(encodeResult);
  if (isPlain(value)) {
    const out = {};
    for (const [key, item] of Object.entries(value)) out[key] = encodeResult(item);
    return out;
  }
  return value;
}

/** True for the two outcomes whose effects are not known: `Unknown`, and a
 * daemon `Failed` whose message carries `attempted: true`. */
export function isUncertain(error) {
  if (error?.code === "Unknown") return true;
  if (error?.code !== "Failed") return false;
  if (error?.detail?.attempted === true) return true;
  return typeof error?.message === "string" && /\battempted: true\b/.test(error.message);
}
