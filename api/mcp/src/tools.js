// One MCP tool per call of the table, generated from the table's argument and
// option kinds (the same kinds api/scripts/node-types.mjs maps). No call,
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
  image: {
    type: "string",
    minLength: 1,
    description: "a sha256: image digest, a Docker image id, or a Docker reference such as postgres:17-bookworm",
  },
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

/**
 * The global options of a call (call surface 2.0), as schema properties:
 * `detail` for the long form, and `with` when the entry lists sections.
 */
export function globalOptions(spec) {
  const out = {
    detail: { type: "boolean", description: "the long form: every key, nulls included" },
  };
  const sections = Object.keys(spec.form.with ?? {});
  if (sections.length > 0) {
    out.with = {
      type: "array",
      items: { type: "string", enum: sections },
      uniqueItems: true,
      description: `sections to add to the answer: ${sections.join(", ")}`,
    };
  }
  return out;
}

/** The options a client may send: every option that is not a function. */
export function visibleOptions(spec) {
  return spec.options.filter((option) => option.kind !== "function" && !HIDDEN_OPTIONS.has(option.name));
}

/** `workspace.exec`'s `env`: one object of names to text (call surface 2.0). */
const EXEC_ENV = {
  type: "object",
  additionalProperties: { type: "string" },
  description: "the command's environment variables, an object of names to text",
};

/** The JSON Schema property of one argument or option. */
export function propertyOf(spec, entry) {
  if (spec.family === "workspace" && spec.call === "exec" && entry.name === "env") return EXEC_ENV;
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
    notes.push("Each stream's text is in the result up to its last 1 MiB; a longer stream answers its last 1 MiB, and stdout_file or stderr_file names the file on the machine that holds the whole stream; stdout_bytes and stderr_bytes give its full size.");
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
  for (const option of visibleOptions(spec)) {
    properties[option.name] = propertyOf(spec, option);
    if (option.required) required.push(option.name);
  }
  Object.assign(properties, globalOptions(spec));
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
