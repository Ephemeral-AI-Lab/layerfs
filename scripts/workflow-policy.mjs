function same(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

export function workflowPolicyErrors(workflow) {
  const errors = [];
  const triggers = workflow?.on;
  if (!triggers || !same(Object.keys(triggers).sort(), ["pull_request", "push"]))
    errors.push("workflow triggers must be exactly push and pull_request");
  else
    for (const name of ["push", "pull_request"])
      if (
        triggers[name] !== null &&
        (!triggers[name] || Object.keys(triggers[name]).length !== 0)
      )
        errors.push(`${name} trigger must be unfiltered`);
  const job = workflow?.jobs?.validate;
  if (!job) return [...errors, "validate job is missing"];
  if (Object.hasOwn(job, "if")) errors.push("validate job must not have if");
  if (job["continue-on-error"] !== undefined)
    errors.push("validate job must not continue on error");
  if (!same(job.strategy?.matrix?.os, ["ubuntu-latest", "windows-latest"]))
    errors.push("validate OS matrix differs from the approved matrix");
  if (!same(job.strategy?.matrix?.node, [22, 24]))
    errors.push("validate Node matrix differs from the approved matrix");
  if (job["runs-on"] !== "${{ matrix.os }}")
    errors.push("validate job must run on matrix.os");
  const steps = job.steps ?? [];
  const requireUnconditional = (step, label) => {
    if (!step) {
      errors.push(`${label} step is missing`);
      return;
    }
    if (Object.hasOwn(step, "if")) errors.push(`${label} step must not have if`);
    if (step["continue-on-error"] !== undefined)
      errors.push(`${label} step must not continue on error`);
  };
  const setupNodeSteps = steps.filter((step) =>
    /^actions\/setup-node@/u.test(step.uses ?? ""),
  );
  if (setupNodeSteps.length !== 1)
    errors.push("exactly one setup-node step is required");
  else {
    requireUnconditional(setupNodeSteps[0], "setup-node");
    if (setupNodeSteps[0].with?.["node-version"] !== "${{ matrix.node }}")
      errors.push("setup-node must use matrix.node");
  }
  const pnpmSetupSteps = steps.filter((step) =>
    /^pnpm\/action-setup@/u.test(step.uses ?? ""),
  );
  if (pnpmSetupSteps.length !== 1)
    errors.push("exactly one pnpm setup step is required");
  else requireUnconditional(pnpmSetupSteps[0], "pnpm setup");
  const installSteps = steps.filter(
    (step) => step.run === "pnpm install --frozen-lockfile",
  );
  if (installSteps.length !== 1)
    errors.push("exactly one frozen pnpm install step is required");
  else requireUnconditional(installSteps[0], "pnpm install");
  const validationSteps = steps.filter((step) => step.run === "pnpm validate:accepted");
  if (validationSteps.length !== 1)
    errors.push("validate:accepted must be one executable step");
  else {
    requireUnconditional(validationSteps[0], "validate:accepted");
  }
  return errors;
}
