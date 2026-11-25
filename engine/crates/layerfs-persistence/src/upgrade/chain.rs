//! The chain of steps (#804 §3): one step per format change, each
//! converting exactly one version to the next, run in order from a Store's
//! version to the latest inside the caller's one transaction.
//!
//! The runner knows no engine: a step works on the engine's open
//! transaction (`C`). The engine begins the transaction, calls [`run`],
//! checks the result once, and commits; any error from [`run`] leaves the
//! engine to roll everything back, so a chain never stops in the middle.
use super::types::UpgradeError;

/// One format change `from -> to` (`to == from + 1`).
pub struct Step<C> {
    /// The version this step accepts.
    pub from: i64,
    /// The version it produces.
    pub to: i64,
    /// The raw encodings it maps, `(before, after)`: on SQLite every
    /// `(schema, acquisition, layout)` `user_version` of `from` to the same
    /// variant of `to`; on PostgreSQL the one `schema_version`.
    pub raw: &'static [(i64, i64)],
    /// The recorded-source variants it accepts, in its frozen list's order;
    /// the first is the one the base fixture `v<from>/` holds.
    pub variants: &'static [&'static str],
    /// Names the recorded-source variant the transaction holds when it is
    /// one this step accepts exactly (rule 5); `None` refuses the Store.
    pub recognize: fn(&mut C) -> Result<Option<&'static str>, UpgradeError>,
    /// The step's statements, on the open transaction.
    pub apply: fn(&mut C) -> Result<(), UpgradeError>,
}

impl<C> Step<C> {
    /// `"2->3"`.
    pub fn name(&self) -> String {
        format!("{}->{}", self.from, self.to)
    }
}

/// The steps from `from` to `latest`, in order; refused `Unsupported` by name
/// when `from` is newer than this build or a step on the way is missing
/// (rule 3). Empty when `from == latest`.
pub fn plan<C>(steps: &[Step<C>], from: i64, latest: i64) -> Result<Vec<&Step<C>>, UpgradeError> {
    if from > latest {
        return Err(UpgradeError::Unsupported(format!(
            "format {from} is newer than this build, which writes format {latest}"
        )));
    }
    let mut planned = Vec::new();
    for version in from..latest {
        let mut found = steps.iter().filter(|step| step.from == version);
        match (found.next(), found.next()) {
            (Some(step), None) if step.to == version + 1 => planned.push(step),
            _ => {
                return Err(UpgradeError::Unsupported(format!(
                    "format {version} has no upgrade step to format {}, so format {from} \
                     cannot reach format {latest}",
                    version + 1
                )))
            }
        }
    }
    Ok(planned)
}

/// Runs every step from `from` to `latest` in order on `context`, the
/// engine's open transaction: each step first recognizes what it is given
/// (the Store's recorded source for the first, the previous step's exact
/// target for the rest), then applies. Returns the steps' names; on any
/// error the caller rolls back the whole transaction.
pub fn run<C>(
    context: &mut C,
    steps: &[Step<C>],
    from: i64,
    latest: i64,
) -> Result<Vec<String>, UpgradeError> {
    let planned = plan(steps, from, latest)?;
    let mut names = Vec::with_capacity(planned.len());
    for step in planned {
        if (step.recognize)(context)?.is_none() {
            return Err(UpgradeError::Unsupported(format!(
                "the Store's recorded schema is not one step {} recognizes (format {}, \
                 variants {})",
                step.name(),
                step.from,
                step.variants.join(", ")
            )));
        }
        (step.apply)(context)?;
        names.push(step.name());
    }
    Ok(names)
}

/// The chain guard (#804 §11): `steps` cover every version from `floor` to
/// `latest` contiguously, one step each, and each step maps its raw
/// encodings so that `variant` (the engine's acquisition/layout decoder)
/// is unchanged. The first rule broken, in words.
pub fn check_chain<C>(
    steps: &[Step<C>],
    floor: i64,
    latest: i64,
    variant: fn(i64) -> Option<(i64, i64)>,
) -> Result<(), String> {
    for version in floor..latest {
        let found: Vec<_> = steps.iter().filter(|step| step.from == version).collect();
        match found.as_slice() {
            [step] if step.to == version + 1 => {}
            [] => return Err(format!("gap: no step from format {version}")),
            _ => return Err(format!("format {version} has several or a skipping step")),
        }
    }
    if let Some(step) = steps.iter().find(|s| s.from < floor || s.to > latest) {
        return Err(format!("step {} is outside {floor}..{latest}", step.name()));
    }
    for step in steps {
        if step.raw.is_empty() || step.variants.is_empty() {
            return Err(format!("step {} maps no encoding or variant", step.name()));
        }
        for &(before, after) in step.raw {
            match (variant(before), variant(after)) {
                (Some((sb, kept)), Some((sa, kept_after))) if sb == step.from && sa == step.to => {
                    if kept != kept_after {
                        return Err(format!(
                            "step {} changes acquisition or layout: {before} -> {after}",
                            step.name()
                        ));
                    }
                }
                _ => {
                    return Err(format!(
                        "step {} maps {before} -> {after}, not format {} -> {}",
                        step.name(),
                        step.from,
                        step.to
                    ))
                }
            }
        }
    }
    Ok(())
}
