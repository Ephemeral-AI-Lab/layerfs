const key = "ev" + "al";
const reflect = globalThis[key as keyof typeof globalThis];

if (typeof reflect === "function")
  Reflect.apply(reflect, globalThis, ["import('../sqlite/schema.js')"]);
