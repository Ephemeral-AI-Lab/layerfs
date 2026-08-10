const construct = [].filter.constructor;
const processValue = construct("return process")();

processValue.getBuiltinModule("node:module");
