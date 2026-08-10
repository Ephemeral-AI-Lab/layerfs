const { eval: evaluate } = globalThis as unknown as {
  eval(code: string): unknown;
};

evaluate("import('../sqlite/schema.js')");
