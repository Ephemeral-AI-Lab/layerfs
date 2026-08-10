const Compile = Function.bind(null);

new Compile("return import('../sqlite/schema.js')")();
