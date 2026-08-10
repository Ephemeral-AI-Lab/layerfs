const property = "ev" + "al";
const evaluate = (globalThis as unknown as Record<string, (code: string) => unknown>)[
  property
];

evaluate?.("import('@ephemeralai/fs-sqlite-node')");
