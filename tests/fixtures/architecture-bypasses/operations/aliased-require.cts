// Deliberately forbidden: aliasing require must not hide a SQLite dependency.
const load = require;
load("../../../../packages/fs/src/sqlite/schema.js");
