const root = globalThis as unknown as { eval(code: string): unknown };
const evaluate = root.eval;

evaluate("import('../sqlite/schema.js')");
