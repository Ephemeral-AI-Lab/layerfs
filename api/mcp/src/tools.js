// One MCP tool per call of the table, generated from the table's argument and
// option kinds (the same kinds api/scripts/gen-node-types.mjs maps). No call,
// argument or result name is written here: the names come from the table.
//
// Kinds:
//   project  "<store>/<project>" text          name, text  text
//   int      a whole number; a decimal string keeps one beyond ±(2^53-1)
//   size     an integer of bytes, or text such as "512M"
//   bool     a boolean
//   content  text, or bytes as { "base64": "..." }
//   json     any JSON value
//   function on_progress, stdout and stderr stay in this server (progress goes
//            out as MCP progress notifications; output is buffered), and the
//            `stream` option is left out because it writes to this process's
//            stdout, which carries the protocol.

const WHOLE = /^-?[0-9]+$/;

/** Functions never cross the protocol; `stream: true` would corrupt stdout. */
const HIDDEN_OPTIONS = new Set(["stream"]);

const COMMON = {
  project: {
    type: "string",
    description: "a project, as <store>/<project>",
  },
  name: { type: "string" },
  text: { type: "string" },
  int: {
    oneOf: [{ type: "integer" }, { type: "string", pattern: WHOLE.source }],
    description: "a whole number; send a decimal string to keep one beyond ±(2^53-1) exact",
  },
  bool: { type: "boolean" },
  size: {
    oneOf: [{ type: "integer" }, { type: "string", minLength: 1 }],
    description: "a size in bytes, or text such as \"512M\" or \"8G\"",
  },
  content: {
    oneOf: [
      { type: "string" },
      {
        type: "object",
        properties: { base64: { type: "string" } },
        required: ["base64"],
        additionalProperties: false,
      },
    ],
    description: "text, or bytes as { \"base64\": \"...\" }",
  },
  json: { description: "any JSON value; nested { \"base64\": \"...\" } content is read as bytes by the library" },
};

/** The options a client may send: every option that is not a function. */
export function visibleOptions(spec) {
  return spec.options.filter((option) => option.kind !== "function" && !HIDDEN_OPTIONS.has(option.name));
}

/** The JSON Schema property of one argument or option. */
export function propertyOf(spec, entry) {
  const schema = COMMON[entry.kind];
  if (!schema) throw new Error(`${spec.family}.${spec.call}: no schema for kind "${entry.kind}"`);
  return schema;
}

function descriptionOf(spec) {
  const notes = [];
  if (spec.family === "project" && spec.call === "export") {
    notes.push("The destination is a path on the machine running this MCP server, not on the caller; a folder scope refuses this call.");
  }
  if (spec.family === "workspace" && spec.call === "exec") {
    notes.push("Output is buffered by the Node binding: text up to 1 MiB per stream is in the result; a larger stream is not delivered, and stdout_bytes, stderr_bytes and streamed=true report it.");
  }
  return [spec.help, ...notes].join(" ");
}

/** The MCP tool of one table entry: `{ name, description, inputSchema, spec }`. */
export function toolFor(spec) {
  const properties = {};
  const required = [];
  for (const arg of spec.args) {
    properties[arg.name] = propertyOf(spec, arg);
    if (!arg.optional) required.push(arg.name);
  }
  for (const option of visibleOptions(spec)) properties[option.name] = propertyOf(spec, option);
  return {
    name: `${spec.family}_${spec.call}`,
    description: descriptionOf(spec),
    inputSchema: { type: "object", properties, required, additionalProperties: false },
    // kept off the wire: the server uses it to build the call and its options
    spec,
  };
}

/** The tools of `table`, in the table's order. */
export function toolsOf(table) {
  return table.map(toolFor);
}
