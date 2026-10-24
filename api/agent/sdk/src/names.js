// Method names before #428 and their current names. Trace lines keep the
// name a call had when it ran, so a viewer that replays older trace files
// maps each `method` through this table; a current name maps to itself.

/** Old method name -> current name. */
export const RENAMED_METHODS = Object.freeze({
  "operator.inspect.history": "backend.store.project.get_history",
  "operator.store.stats": "backend.store.project.get_storage",
  "operator.store.overwritten": "backend.store.project.list_overwritten",
  "operator.store.commits": "backend.store.branch.list_commit_storage",
  "operator.files.stats": "backend.store.fs.get_storage",
  "operator.files.history": "backend.store.fs.list_revisions",
  "workspace.status": "backend.daemon.workspace.status",
  "operator.workspace.changes": "backend.daemon.workspace.list_changes",
  "operator.workspace.overlay": "backend.daemon.workspace.get_storage",
  "operator.process.start": "backend.daemon.process.start",
  "operator.process.list": "backend.daemon.process.list",
  "operator.process.cancel": "backend.daemon.process.cancel",
  "operator.inspect.snapshot": "sandbox.list_workspaces",
  "operator.sandbox.list": "project.list_sandboxes",
  "operator.project.progress": "runtime.project.get_init",
});

/** A trace line's `method` under its current name. */
export const currentMethod = (method) => RENAMED_METHODS[method] ?? method;
