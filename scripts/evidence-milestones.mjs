const supportedEvidenceMilestones = Object.freeze(["m0", "m1", "m2", "m3"]);

export function evidenceMilestonesThrough(activeMilestone) {
  const activeIndex = supportedEvidenceMilestones.indexOf(activeMilestone);
  if (activeIndex < 0)
    throw new Error(`evidence checker has no validation schema for ${activeMilestone}`);
  return supportedEvidenceMilestones.slice(0, activeIndex + 1);
}
