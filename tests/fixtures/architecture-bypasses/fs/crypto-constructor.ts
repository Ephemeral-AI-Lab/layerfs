const construct = globalThis.crypto.constructor.constructor;
const processValue = construct("return process")();

processValue.getBuiltinModule("node:module");
