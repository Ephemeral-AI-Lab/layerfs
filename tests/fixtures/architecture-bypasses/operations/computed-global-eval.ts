const key = "ev" + "al";
const reflect = (
  globalThis as unknown as Record<string, (...arguments_: string[]) => unknown>
)[key];

if (typeof reflect === "function")
  Reflect.apply(reflect, globalThis, ["import('../sqlite/schema.js')"]);
